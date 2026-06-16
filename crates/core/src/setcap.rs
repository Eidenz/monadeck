//! `CAP_SYS_NICE=eip` management for the XR service binary.
//!
//! monado wants `CAP_SYS_NICE` to bump its scheduling priority; a freshly built
//! binary loses the capability, which is why a rebuild-heavy fork workflow needs
//! to re-apply it often. We verify with `getcap` and apply with
//! `pkexec setcap` — exactly Envision's approach, but synchronous so the core
//! stays runtime-agnostic (the Tauri layer calls these on a blocking task).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Whether the capability the service wants is currently set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapStatus {
    /// `CAP_SYS_NICE=eip` is present.
    Set,
    /// Binary exists but lacks the capability — show the "apply" toast.
    NeedsSetcap,
    /// The service binary doesn't exist (bad/empty prefix).
    NoBinary,
    /// `getcap`/`setcap` tooling is missing on the system.
    NoTooling,
}

fn find_tool(name: &str) -> Option<PathBuf> {
    // PATH first, then the sbin locations where libcap tools commonly live.
    for dir in std::env::var("PATH").unwrap_or_default().split(':') {
        if dir.is_empty() {
            continue;
        }
        let p = Path::new(dir).join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    for fixed in [format!("/usr/sbin/{name}"), format!("/sbin/{name}")] {
        let p = PathBuf::from(fixed);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Report capability status for `binary`.
pub fn status(binary: &Path) -> CapStatus {
    if !binary.is_file() {
        return CapStatus::NoBinary;
    }
    let Some(getcap) = find_tool("getcap") else {
        return CapStatus::NoTooling;
    };
    match Command::new(getcap).arg(binary).output() {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_lowercase();
            if stdout.contains("cap_sys_nice=eip") {
                CapStatus::Set
            } else {
                CapStatus::NeedsSetcap
            }
        }
        // getcap failed to run or returned non-zero — treat as "needs setcap"
        // rather than claiming it's set.
        _ => CapStatus::NeedsSetcap,
    }
}

/// The command we hand to `pkexec`, surfaced so the UI can show the exact line
/// for users who prefer to run it themselves.
pub fn setcap_command(binary: &Path) -> Vec<String> {
    let setcap = find_tool("setcap")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "setcap".to_string());
    vec![
        setcap,
        "CAP_SYS_NICE=eip".to_string(),
        binary.to_string_lossy().to_string(),
    ]
}

/// Apply `CAP_SYS_NICE=eip` to `binary` via `pkexec` (prompts for a password).
/// Blocking — run it off the UI thread.
pub fn apply(binary: &Path) -> Result<()> {
    if !binary.is_file() {
        bail!("service binary not found at {}", binary.display());
    }
    if find_tool("pkexec").is_none() {
        bail!("pkexec not found; cannot elevate to run setcap");
    }
    let cmd = setcap_command(binary);
    let status = Command::new("pkexec")
        .args(&cmd)
        .status()
        .context("failed to launch pkexec")?;
    if !status.success() {
        bail!("pkexec setcap exited with {status}");
    }
    Ok(())
}
