//! Starting programs from the host system.
//!
//! The AppImage's launcher points search paths at the image (`LD_LIBRARY_PATH`,
//! `PATH`, `XDG_DATA_DIRS`, the GTK/GIO module files…), sets `PYTHONHOME` into
//! it and forces `GDK_BACKEND=x11`, and every program Monadeck starts inherits
//! that: the system's curl then loads the image's libcrypto next to its own
//! libssl, host Python can't find its standard library, apps open under
//! XWayland, the overlay picks up the image's older libpipewire. Monadeck
//! itself needs that environment (WebKit starts its helpers from the image), so
//! it's undone for each child instead: [`command`] is `Command::new` with the
//! image's entries taken back out. Outside an AppImage it changes nothing.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::process::Command;

/// A command for a host program, without what the AppImage put in the environment.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    if let Some(appdir) = std::env::var_os("APPDIR").filter(|d| !d.is_empty()) {
        for (key, value) in fixups(&appdir, std::env::vars_os()) {
            match value {
                Some(v) => cmd.env(key, v),
                None => cmd.env_remove(key),
            };
        }
    }
    cmd
}

/// The variables to change for a host program when running from the image at
/// `appdir`: each one set to a new value, or removed (`None`).
fn fixups(
    appdir: &OsStr,
    vars: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, Option<OsString>)> {
    let appdir = Path::new(appdir);
    let mut out = Vec::new();
    for (key, value) in vars {
        match key.to_str() {
            // The image's own markers: a child isn't running from it.
            Some("APPDIR" | "APPIMAGE" | "ARGV0" | "OWD") => out.push((key, None)),
            // Forced by the image's GTK hook (whatever it was before is gone).
            Some("GDK_BACKEND") if value == "x11" => out.push((key, None)),
            Some("GTK_THEME") if value.is_empty() => out.push((key, None)),
            _ => {
                if !contains(&value, appdir.as_os_str()) {
                    continue;
                }
                // A path list (or a single path): keep what isn't in the image.
                let kept: Vec<&[u8]> = value
                    .as_bytes()
                    .split(|&b| b == b':')
                    .filter(|entry| {
                        !entry.is_empty()
                            && !Path::new(OsStr::from_bytes(entry)).starts_with(appdir)
                    })
                    .collect();
                let new = (!kept.is_empty()).then(|| OsString::from_vec(kept.join(&b':')));
                if new.as_ref() != Some(&value) {
                    out.push((key, new));
                }
            }
        }
    }
    out
}

fn contains(haystack: &OsStr, needle: &OsStr) -> bool {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    !n.is_empty() && h.windows(n.len()).any(|w| w == n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(vars: &[(&str, &str)]) -> Vec<(String, Option<String>)> {
        let vars = vars
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v)));
        let mut out: Vec<(String, Option<String>)> = fixups(OsStr::new("/tmp/.mount_MonaXY"), vars)
            .into_iter()
            .map(|(k, v)| {
                (
                    k.into_string().unwrap(),
                    v.map(|v| v.into_string().unwrap()),
                )
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn undoes_the_appimage_launcher() {
        // What the v1.8 AppImage's AppRun + GTK hook leave in the environment.
        let got = run(&[
            ("APPDIR", "/tmp/.mount_MonaXY"),
            ("APPIMAGE", "/home/u/Monadeck.AppImage"),
            ("LD_LIBRARY_PATH", "/tmp/.mount_MonaXY/usr/lib/:/tmp/.mount_MonaXY/usr/lib64/:"),
            ("PATH", "/tmp/.mount_MonaXY/usr/bin/:/tmp/.mount_MonaXY/usr/sbin/:/usr/local/bin:/usr/bin"),
            ("XDG_DATA_DIRS", "/tmp/.mount_MonaXY/usr/share:/usr/share:/tmp/.mount_MonaXY/usr/share/:/usr/local/share"),
            ("PYTHONHOME", "/tmp/.mount_MonaXY/usr/"),
            ("GSETTINGS_SCHEMA_DIR", "/tmp/.mount_MonaXY//usr/share/glib-2.0/schemas"),
            ("GDK_BACKEND", "x11"),
            ("GTK_THEME", ""),
            ("HOME", "/home/u"),
            ("XR_RUNTIME_JSON", "/home/u/.local/share/monadeck/runtime.json"),
        ]);
        let want: Vec<(String, Option<String>)> = vec![
            ("APPDIR".into(), None),
            ("APPIMAGE".into(), None),
            ("GDK_BACKEND".into(), None),
            ("GSETTINGS_SCHEMA_DIR".into(), None),
            ("GTK_THEME".into(), None),
            ("LD_LIBRARY_PATH".into(), None),
            ("PATH".into(), Some("/usr/local/bin:/usr/bin".into())),
            ("PYTHONHOME".into(), None),
            (
                "XDG_DATA_DIRS".into(),
                Some("/usr/share:/usr/local/share".into()),
            ),
        ];
        assert_eq!(got, want);
    }

    #[test]
    fn leaves_a_user_choice_alone() {
        // Not the image's: GDK_BACKEND=wayland, a GTK theme, a folder whose name
        // merely starts like the image's mount point.
        assert!(run(&[
            ("GDK_BACKEND", "wayland"),
            ("GTK_THEME", "Adwaita:dark"),
            ("PATH", "/tmp/.mount_MonaXYZ/bin:/usr/bin")
        ])
        .is_empty());
    }
}
