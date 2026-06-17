//! Persisted per-game tracked playtime — total seconds the overlay observed each
//! game running, keyed by game key (Steam appid or non-Steam shortcut appid), the
//! same key favorites use. This is what gives non-Steam games (and anything Steam
//! hasn't synced) a "played" figure.
use crate::paths::monadeck_config_dir;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn file() -> PathBuf {
    monadeck_config_dir().join("overlay_playtime.json")
}

/// Load the map of game key -> total tracked seconds (empty if none / unreadable).
pub fn load() -> HashMap<String, u64> {
    fs::read_to_string(file())
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

/// Persist the map (best-effort).
pub fn save(map: &HashMap<String, u64>) {
    let path = file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(map) {
        let _ = fs::write(path, json);
    }
}
