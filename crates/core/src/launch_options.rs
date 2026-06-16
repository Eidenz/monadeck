//! Builds the Steam launch-options string games need to pick up monado + xrizer.
//!
//! This is where Monadeck improves on Envision: Envision's helper only emits
//! `PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES=1 %command%` and relies on
//! `openvrpaths.vrpath` for OpenVR discovery — but that registration doesn't
//! survive the Proton / Steam-Linux-Runtime sandbox, so OpenVR games need an
//! explicit `VR_OVERRIDE` pointing straight at the xrizer runtime. We include it.

use crate::config::{MonadeckConfig, OvrRuntime};
use crate::paths::home;
use std::path::PathBuf;

/// Wrap in double quotes only when the value would otherwise split on whitespace.
fn quote(value: &str) -> String {
    if value.chars().any(char::is_whitespace) {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

/// The launch-options line, ready to paste into a Steam game's properties.
///
/// Client-side only: `VR_OVERRIDE` (when registering xrizer) + the pressure-
/// vessel import flag + `%command%`. The user's `config.environment` vars are
/// server-side (they go to monado-service), so they are deliberately NOT
/// included here.
pub fn steam_launch_options(cfg: &MonadeckConfig) -> String {
    let mut parts: Vec<String> = Vec::new();

    if cfg.ovr_runtime == OvrRuntime::Xrizer {
        if let Some(xr) = cfg.xrizer_path.as_ref() {
            parts.push(format!("VR_OVERRIDE={}", quote(&xr.to_string_lossy())));
        }
    }

    parts.push("PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES=1".to_string());
    parts.push("%command%".to_string());
    parts.join(" ")
}

/// Best-effort guess at the xrizer runtime dir, following the layout recommended
/// on the xrizer README: `~/.local/share/xrizer/<version>`, preferring
/// `xrizer-nightly`, else the first version directory present.
pub fn detect_xrizer_path() -> Option<PathBuf> {
    let base = home().join(".local/share/xrizer");
    let nightly = base.join("xrizer-nightly");
    if nightly.is_dir() {
        return Some(nightly);
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&base)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.into_iter().next_back()
}
