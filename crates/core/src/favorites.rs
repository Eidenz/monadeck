//! Persisted overlay favorites — a set of game keys (Steam appid or non-Steam
//! shortcut appid) the user pinned. Stored as JSON in the monadeck config dir.
use crate::paths::monadeck_config_dir;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

fn file() -> PathBuf {
    monadeck_config_dir().join("overlay_favorites.json")
}

/// Load the set of favorited game keys (empty if none / unreadable).
pub fn load() -> HashSet<String> {
    fs::read_to_string(file())
        .ok()
        .and_then(|c| serde_json::from_str::<Vec<String>>(&c).ok())
        .map(|v| v.into_iter().collect())
        .unwrap_or_default()
}

/// Persist the set of favorited game keys (best-effort).
pub fn save(favorites: &HashSet<String>) {
    let path = file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut keys: Vec<&String> = favorites.iter().collect();
    keys.sort();
    if let Ok(json) = serde_json::to_string_pretty(&keys) {
        let _ = fs::write(path, json);
    }
}
