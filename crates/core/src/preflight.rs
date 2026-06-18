//! Pre-flight system checks — the *runtime* (not build-time) prerequisites an XR
//! session needs that live OUTSIDE Monadeck's control.
//!
//! This is the small, distribution-relevant slice of Envision's depcheck. We do
//! NOT check build tools (Monadeck doesn't build monado) — only the two things
//! that, when missing on someone else's machine, make Monado silently fail to
//! see the HMD or refuse to elevate:
//!
//! - the `xr-hardware` **udev rules**, which grant a non-root user access to VR
//!   USB devices, and
//! - **`pkexec`**, which our `setcap` and AMD-VR-power-profile fixes use to
//!   elevate.
//!
//! On a properly set-up box (the developer's) every check passes and the UI
//! shows nothing; the value is in handing Monadeck to someone whose machine
//! lacks these.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// The udev rules file shipped by the `xr-hardware` package (a Monado-project
/// artifact), covering a broad set of HMDs/controllers.
const XR_HARDWARE_RULE: &str = "70-xrhardware.rules";

/// Directories udev reads rules from, in the order a distro populates them.
const UDEV_RULE_DIRS: &[&str] = &[
    "/usr/lib/udev/rules.d",
    "/etc/udev/rules.d",
    "/usr/local/lib/udev/rules.d",
    "/run/udev/rules.d",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Missing this likely breaks VR on someone else's box (e.g. no HMD access).
    Important,
    /// A degraded-but-usable feature (one-click elevation won't work).
    Optional,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreflightCheck {
    /// Stable id for the frontend.
    pub id: String,
    /// Short human label.
    pub label: String,
    /// Whether the check passed.
    pub ok: bool,
    pub severity: Severity,
    /// What this is for / what breaks without it.
    pub detail: String,
    /// A suggested install command, present only when the check failed.
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreflightReport {
    pub checks: Vec<PreflightCheck>,
    /// True when every check passed (the UI hides itself).
    pub all_ok: bool,
    /// Detected distro family label (best-effort), shown beside the fix hints.
    pub distro: Option<String>,
}

/// Search `$PATH` plus the common sbin dirs for an executable.
fn find_exe(name: &str) -> bool {
    for dir in std::env::var("PATH").unwrap_or_default().split(':') {
        if !dir.is_empty() && Path::new(dir).join(name).is_file() {
            return true;
        }
    }
    ["/usr/bin", "/usr/sbin", "/sbin", "/bin"]
        .iter()
        .any(|d| Path::new(d).join(name).is_file())
}

fn find_udev_rule(filename: &str) -> Option<PathBuf> {
    UDEV_RULE_DIRS.iter().find_map(|dir| {
        let p = Path::new(dir).join(filename);
        p.is_file().then_some(p)
    })
}

/// A coarse distro family — enough to pick a package name and an installer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Distro {
    Arch,
    Debian,
    Fedora,
    Suse,
    Gentoo,
    Alpine,
}

impl Distro {
    fn detect() -> Option<Self> {
        Self::from_os_release(&std::fs::read_to_string("/etc/os-release").ok()?)
    }

    /// Match against the `ID=` and `ID_LIKE=` fields (covers most derivatives via
    /// `ID_LIKE`, e.g. Nobara→fedora, Pop!_OS→ubuntu/debian).
    fn from_os_release(text: &str) -> Option<Self> {
        let mut hay = String::new();
        for line in text.lines() {
            if let Some(v) = line
                .strip_prefix("ID=")
                .or_else(|| line.strip_prefix("ID_LIKE="))
            {
                hay.push(' ');
                hay.push_str(&v.trim().trim_matches('"').to_lowercase());
            }
        }
        if hay.contains("arch") || hay.contains("manjaro") {
            Some(Self::Arch)
        } else if hay.contains("ubuntu") || hay.contains("debian") {
            Some(Self::Debian)
        } else if hay.contains("fedora") || hay.contains("rhel") || hay.contains("centos") {
            Some(Self::Fedora)
        } else if hay.contains("suse") {
            Some(Self::Suse)
        } else if hay.contains("gentoo") {
            Some(Self::Gentoo)
        } else if hay.contains("alpine") {
            Some(Self::Alpine)
        } else {
            None
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Arch => "Arch",
            Self::Debian => "Debian/Ubuntu",
            Self::Fedora => "Fedora",
            Self::Suse => "openSUSE",
            Self::Gentoo => "Gentoo",
            Self::Alpine => "Alpine",
        }
    }

    fn install(self, pkg: &str) -> String {
        match self {
            Self::Arch => format!("sudo pacman -S {pkg}"),
            Self::Debian => format!("sudo apt install {pkg}"),
            Self::Fedora => format!("sudo dnf install {pkg}"),
            Self::Suse => format!("sudo zypper install {pkg}"),
            Self::Gentoo => format!("sudo emerge {pkg}"),
            Self::Alpine => format!("sudo apk add {pkg}"),
        }
    }
}

/// Pick the package name for the detected distro, falling back to `default`.
fn pkg_for(distro: Option<Distro>, table: &[(Distro, &str)], default: &str) -> String {
    distro
        .and_then(|d| table.iter().find(|(k, _)| *k == d).map(|(_, v)| v.to_string()))
        .unwrap_or_else(|| default.to_string())
}

/// Build the install hint for a failed check (full command when we know the
/// distro, otherwise a generic "install X" line).
fn fix_hint(distro: Option<Distro>, pkg: &str) -> String {
    match distro {
        Some(d) => d.install(pkg),
        None => format!("install the '{pkg}' package with your package manager"),
    }
}

/// Run the checks and return a report. Cheap (a handful of `stat`s), but does
/// touch the filesystem, so callers run it off the UI thread.
pub fn run() -> PreflightReport {
    let distro = Distro::detect();
    let mut checks = Vec::new();

    // 1. xr-hardware udev rules — HMD/controller access without root.
    let udev_ok = find_udev_rule(XR_HARDWARE_RULE).is_some();
    let xr_pkg = pkg_for(
        distro,
        &[
            (Distro::Arch, "xr-hardware"),
            (Distro::Debian, "xr-hardware"),
            (Distro::Fedora, "xr-hardware"),
            (Distro::Alpine, "xr-hardware"),
        ],
        "xr-hardware",
    );
    checks.push(PreflightCheck {
        id: "xr_hardware_udev".into(),
        label: "VR hardware udev rules".into(),
        ok: udev_ok,
        severity: Severity::Important,
        detail: "Lets your user access HMDs and controllers without root. Without it \
                 monado may not see the headset. (On Arch it's in the AUR.)"
            .into(),
        fix: (!udev_ok).then(|| fix_hint(distro, &xr_pkg)),
    });

    // 2. pkexec — privilege elevation for setcap + the AMD VR power profile.
    let pkexec_ok = find_exe("pkexec");
    let pk_pkg = pkg_for(
        distro,
        &[
            (Distro::Arch, "polkit"),
            (Distro::Debian, "pkexec"),
            (Distro::Fedora, "polkit"),
            (Distro::Alpine, "polkit"),
            (Distro::Gentoo, "sys-auth/polkit"),
            (Distro::Suse, "polkit"),
        ],
        "polkit",
    );
    checks.push(PreflightCheck {
        id: "pkexec".into(),
        label: "pkexec (polkit)".into(),
        ok: pkexec_ok,
        severity: Severity::Optional,
        detail: "Used to set CAP_SYS_NICE on the service and the AMD VR power profile. \
                 Without it those one-click fixes can't prompt for a password."
            .into(),
        fix: (!pkexec_ok).then(|| fix_hint(distro, &pk_pkg)),
    });

    let all_ok = checks.iter().all(|c| c.ok);
    PreflightReport {
        checks,
        all_ok,
        distro: distro.map(|d| d.label().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_ubiquitous_executable() {
        // `sh` exists on every system this runs on.
        assert!(find_exe("sh"));
        assert!(!find_exe("definitely-not-a-real-binary-xyz"));
    }

    #[test]
    fn report_has_both_checks() {
        let r = run();
        assert_eq!(r.checks.len(), 2);
        assert!(r.checks.iter().any(|c| c.id == "xr_hardware_udev"));
        assert!(r.checks.iter().any(|c| c.id == "pkexec"));
        // A passing check carries no fix; a failing one does.
        for c in &r.checks {
            assert_eq!(c.ok, c.fix.is_none());
        }
    }

    #[test]
    fn detects_distro_family_via_id_like() {
        // Nobara declares ID=nobara ID_LIKE=fedora.
        let osr = "NAME=\"Nobara Linux\"\nID=nobara\nID_LIKE=fedora\n";
        assert_eq!(Distro::from_os_release(osr), Some(Distro::Fedora));

        let pop = "ID=pop\nID_LIKE=\"ubuntu debian\"\n";
        assert_eq!(Distro::from_os_release(pop), Some(Distro::Debian));

        let arch = "ID=arch\n";
        assert_eq!(Distro::from_os_release(arch), Some(Distro::Arch));

        assert_eq!(Distro::from_os_release("ID=plan9\n"), None);
    }
}
