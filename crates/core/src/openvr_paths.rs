//! Manage `~/.config/openvr/openvrpaths.vrpath` so OpenVR apps (via xrizer) load
//! the right runtime — with a backup of the original (usually SteamVR's) so we
//! can hand control back cleanly.
//!
//! Ported from Envision's `openvrpaths_vrpath.rs`. We preserve every field of the
//! existing file and only swap the `runtime` list, then keep a one-off backup of
//! the pre-Monadeck file.

use crate::paths::{backup_dir, openvr_config_dir, openvrpaths_path};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, create_dir_all};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenVrPaths {
    pub config: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_drivers: Option<Vec<String>>,
    pub jsonid: String,
    pub log: Vec<PathBuf>,
    pub runtime: Vec<PathBuf>,
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OvrPathsKind {
    Xrizer,
    SteamVr,
    Other,
    None,
}

fn backup_file() -> PathBuf {
    backup_dir().join("openvrpaths.vrpath.bak")
}

fn read(path: &Path) -> Option<OpenVrPaths> {
    serde_json::from_reader(fs::File::open(path).ok()?).ok()
}

pub fn current() -> Option<OpenVrPaths> {
    read(&openvrpaths_path())
}

fn runtime_is_steamvr(ovr: &OpenVrPaths) -> bool {
    ovr.runtime.iter().any(|rt| {
        rt.to_string_lossy()
            .to_lowercase()
            .trim_end_matches('/')
            .ends_with("steamvr")
    })
}

fn runtime_is_xrizer(ovr: &OpenVrPaths) -> bool {
    ovr.runtime
        .iter()
        .any(|rt| rt.to_string_lossy().to_lowercase().contains("xrizer"))
}

/// Classify the registered OpenVR runtime for display.
pub fn kind() -> OvrPathsKind {
    match current() {
        None => OvrPathsKind::None,
        Some(ovr) if runtime_is_xrizer(&ovr) => OvrPathsKind::Xrizer,
        Some(ovr) if runtime_is_steamvr(&ovr) => OvrPathsKind::SteamVr,
        Some(_) => OvrPathsKind::Other,
    }
}

fn default_paths() -> OpenVrPaths {
    let cfg = openvr_config_dir();
    OpenVrPaths {
        config: vec![cfg.clone()],
        external_drivers: None,
        jsonid: "vrpathreg".to_string(),
        log: vec![crate::paths::home().join(".local/share/openvr")],
        runtime: vec![],
        version: 1,
    }
}

/// Register `xrizer_path` as the OpenVR runtime, backing up the original once.
pub fn set_to_xrizer(xrizer_path: &Path) -> Result<()> {
    if !xrizer_path.is_dir() {
        bail!("xrizer path {} is not a directory", xrizer_path.display());
    }
    let dest = openvrpaths_path();
    create_dir_all(dest.parent().expect("has parent"))?;

    let existing = current();

    // Back up the pre-Monadeck file once (don't overwrite an existing backup with
    // one we already modified — we want the genuine SteamVR original preserved).
    if let Some(ref ovr) = existing {
        if !runtime_is_xrizer(ovr) {
            let bak = backup_file();
            create_dir_all(bak.parent().expect("has parent"))?;
            if !bak.exists() {
                fs::copy(&dest, &bak)
                    .with_context(|| format!("backing up {}", dest.display()))?;
            }
        }
    }

    let mut paths = existing.unwrap_or_else(default_paths);
    paths.runtime = vec![xrizer_path.to_path_buf()];

    let f = fs::File::create(&dest).with_context(|| format!("writing {}", dest.display()))?;
    serde_json::to_writer_pretty(f, &paths)?;
    Ok(())
}

/// Restore the backed-up OpenVR runtime registration (hands control back to
/// SteamVR), undoing [`set_to_xrizer`].
pub fn restore_backup() -> Result<()> {
    let dest = openvrpaths_path();
    let bak = backup_file();
    if bak.is_file() {
        fs::copy(&bak, &dest)
            .with_context(|| format!("restoring {} -> {}", bak.display(), dest.display()))?;
        Ok(())
    } else {
        // No backup means we never replaced a SteamVR file; leave things as-is
        // rather than guessing.
        Ok(())
    }
}
