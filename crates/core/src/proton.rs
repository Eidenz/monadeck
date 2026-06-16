//! Proton 11 / Steam-Linux-Runtime 4 OpenXR import.
//!
//! Newer Proton/SLR need `PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES=1` set for the
//! whole session (a per-game launch option isn't enough), or OpenXR apps fail to
//! launch in VR. We can drop a `~/.config/environment.d/*.conf` so systemd's
//! user environment picks it up after a reboot — the same fix Envision offers.

use crate::paths::config_home;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

const VAR: &str = "PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES";

pub fn env_file_path() -> PathBuf {
    config_home()
        .join("environment.d")
        .join("com.eidenz.monadeck-import-openxr.conf")
}

/// Whether the import var is effectively set: present in this session's
/// environment, or our `environment.d` file exists (applies after reboot).
pub fn is_set() -> bool {
    std::env::var(VAR).map(|v| v == "1").unwrap_or(false) || env_file_path().is_file()
}

/// Write the `environment.d` config file (takes effect after a reboot / relogin).
pub fn write_env_file() -> Result<()> {
    let path = env_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&path, format!("{VAR}=1\n")).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}
