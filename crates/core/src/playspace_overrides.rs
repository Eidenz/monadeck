//! Per-game playspace offsets — a SteamVR-style per-app override of the global
//! playspace nudge. Keyed by the game's cover id (Steam appid or non-Steam
//! shortcut appid), the value is `[x, y, z, yaw_degrees]` (metres + degrees, the
//! same units the overlay edits and `OverlayConfig.playspace_*` stores).
//!
//! The overlay applies a game's override while it is the running app and falls
//! back to the global offset otherwise. Stored as JSON in the monadeck config dir
//! (same pattern as [`crate::favorites`] / [`crate::collections`]).
use crate::paths::monadeck_config_dir;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn file() -> PathBuf {
    monadeck_config_dir().join("playspace_overrides.json")
}

/// Load the per-game playspace offsets (empty if none / unreadable).
pub fn load() -> HashMap<String, [f32; 4]> {
    fs::read_to_string(file())
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

/// Persist the per-game playspace offsets (best-effort).
pub fn save(overrides: &HashMap<String, [f32; 4]>) {
    let path = file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(overrides) {
        let _ = fs::write(path, json);
    }
}
