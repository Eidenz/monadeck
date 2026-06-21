//! Bigscreen Beyond eye tracking: manage the `go-bsb-cams` server (a small HTTP
//! MJPEG server that exposes the Beyond's eye cameras for Babble/VRCFT) so the
//! user doesn't have to keep a terminal open for it.
//!
//! go-bsb-cams is a separate GPLv2 program; Monadeck only orchestrates it (runs
//! it as a child process, optionally downloads the prebuilt binary from its
//! GitHub release), so nothing GPL ships inside Monadeck. The whole feature is
//! gated on a Beyond actually being present (USB vendor 35bd).

use monadeck_core::installer::{install_bsbcams, Installed};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::State;

use crate::state::AppState;

/// udev rule granting the active user libusb access to the Beyond's eye cameras
/// (USB 35bd:0202). Without it go-bsb-cams can't open the cameras (or needs sudo),
/// and its own installer is interactive, so it's useless when we spawn it.
const RULE_PATH: &str = "/etc/udev/rules.d/72-monadeck-bsb-cams.rules";
const RULE: &str =
    "# Bigscreen Beyond eye cameras (go-bsb-cams, managed by Monadeck)\nSUBSYSTEM==\"usb\", ATTRS{idVendor}==\"35bd\", ATTRS{idProduct}==\"0202\", TAG+=\"uaccess\"\n";

/// Is a Bigscreen Beyond present? Scans for any USB device with vendor 35bd,
/// the same hardware signal NemuriXR gates its Beyond features on.
pub fn present() -> bool {
    let Ok(entries) = std::fs::read_dir("/sys/bus/usb/devices") else {
        return false;
    };
    for e in entries.flatten() {
        if let Ok(vid) = std::fs::read_to_string(e.path().join("idVendor")) {
            if vid.trim().eq_ignore_ascii_case("35bd") {
                return true;
            }
        }
    }
    false
}

/// Whether our udev rule is in place.
fn rule_installed() -> bool {
    Path::new(RULE_PATH).exists()
}

/// Resolve the go-bsb-cams binary: the configured path first, then autodetect
/// `~/go-bsb-cams` and `$PATH`.
fn resolve_binary(cfg_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = cfg_path {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let p = PathBuf::from(home).join("go-bsb-cams");
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for d in path.split(':').filter(|d| !d.is_empty()) {
            let p = Path::new(d).join("go-bsb-cams");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Install the udev rule via pkexec, then reload + trigger udev so the connected
/// Beyond picks it up. Blocking (waits on the polkit dialog).
fn install_rule_blocking() -> Result<(), String> {
    // Stage in a user-writable temp file, then let the privileged shell copy it
    // into place (avoids quoting the rule through pkexec). Mirrors NemuriXR.
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let tmp = format!("{dir}/monadeck-bsb-cams.rules");
    std::fs::write(&tmp, RULE).map_err(|e| format!("Couldn't stage the rule: {e}"))?;

    let script = format!(
        "install -m 0644 \"$1\" '{RULE_PATH}' && udevadm control --reload-rules && udevadm trigger --subsystem-match=usb"
    );
    let status = Command::new("pkexec")
        .arg("/bin/sh")
        .arg("-c")
        .arg(&script)
        .arg("monadeck") // $0
        .arg(&tmp) // $1
        .status();
    let _ = std::fs::remove_file(&tmp);

    match status {
        Ok(s) if s.success() => Ok(()),
        // pkexec: 126 = not authorized / dismissed, 127 = auth could not be obtained.
        Ok(s) if matches!(s.code(), Some(126) | Some(127)) => {
            Err("Authorization was dismissed or denied".to_string())
        }
        Ok(_) => Err("Failed to install the rule".to_string()),
        Err(e) => Err(format!("Couldn't run pkexec (is polkit installed?): {e}")),
    }
}

/// Eye-tracking status for the Beyond page.
#[derive(Serialize)]
pub struct EyeStatus {
    /// A Bigscreen Beyond is connected.
    pub present: bool,
    /// go-bsb-cams is currently running.
    pub running: bool,
    /// Our camera-access udev rule is installed.
    pub rule_installed: bool,
    /// Resolved go-bsb-cams binary path, if found.
    pub binary: Option<String>,
    /// Port it serves the MJPEG stream on.
    pub port: u16,
}

/// Is a Bigscreen Beyond present (drives whether the whole section shows).
#[tauri::command]
pub async fn beyond_present() -> bool {
    tauri::async_runtime::spawn_blocking(present)
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub async fn eyetracking_status(state: State<'_, AppState>) -> Result<EyeStatus, String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (cfg_path, port) = {
            let cfg = st.config.lock().unwrap();
            (cfg.bsb_cams_path.clone(), cfg.bsb_cams_port)
        };
        let binary = resolve_binary(cfg_path.as_deref());
        let running = st.eye_runner.lock().unwrap().is_running();
        EyeStatus {
            present: present(),
            running,
            rule_installed: rule_installed(),
            binary: binary.map(|p| p.to_string_lossy().into_owned()),
            port,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn eyetracking_start(state: State<'_, AppState>) -> Result<(), String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let (cfg_path, port) = {
            let cfg = st.config.lock().unwrap();
            (cfg.bsb_cams_path.clone(), cfg.bsb_cams_port)
        };
        let bin = resolve_binary(cfg_path.as_deref())
            .ok_or_else(|| "go-bsb-cams binary not found — download it or set its path".to_string())?;

        let mut runner = st.eye_runner.lock().unwrap();
        if runner.is_running() {
            return Ok(());
        }
        let args = vec!["-port".to_string(), port.to_string()];
        runner
            .start(&bin.to_string_lossy(), &args, &HashMap::new())
            .map_err(|e| format!("Couldn't start go-bsb-cams: {e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn eyetracking_stop(state: State<'_, AppState>) -> Result<(), String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        st.eye_runner.lock().unwrap().terminate();
    })
    .await
    .map_err(|e| e.to_string())
}

/// Download the latest go-bsb-cams binary and point the config at it.
#[tauri::command]
pub async fn install_bsbcams_cmd(state: State<'_, AppState>) -> Result<Installed, String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Installed, String> {
        let installed = install_bsbcams().map_err(|e| e.to_string())?;
        let mut cfg = st.config.lock().unwrap();
        cfg.bsb_cams_path = Some(PathBuf::from(&installed.path));
        cfg.save().map_err(|e| e.to_string())?;
        Ok(installed)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Install the camera-access udev rule (prompts for authorization via pkexec).
#[tauri::command]
pub async fn install_bsbcams_rule() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(install_rule_blocking)
        .await
        .map_err(|e| e.to_string())?
}

/// Point the config at a user-provided go-bsb-cams binary (empty clears it back
/// to autodetect).
#[tauri::command]
pub async fn set_bsbcams_path(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let mut cfg = st.config.lock().unwrap();
        let trimmed = path.trim();
        cfg.bsb_cams_path = if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        };
        cfg.save().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
