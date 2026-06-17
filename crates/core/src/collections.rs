//! Persisted user collections — named groups of games beyond Favorites (e.g.
//! "Seated", "Standing", "Short sessions"). Each collection holds game keys
//! (Steam appid or non-Steam shortcut appid), the same key favorites use.
use crate::paths::monadeck_config_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    pub members: Vec<String>,
}

fn file() -> PathBuf {
    monadeck_config_dir().join("overlay_collections.json")
}

/// Load the ordered list of collections (empty if none / unreadable).
pub fn load() -> Vec<Collection> {
    fs::read_to_string(file())
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

/// Persist the collections (best-effort).
pub fn save(collections: &[Collection]) {
    let path = file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(collections) {
        let _ = fs::write(path, json);
    }
}

/// Add `key` to collection `idx` if missing, or remove it if present.
pub fn toggle_member(collections: &mut [Collection], idx: usize, key: &str) {
    if let Some(c) = collections.get_mut(idx) {
        if let Some(pos) = c.members.iter().position(|m| m == key) {
            c.members.remove(pos);
        } else {
            c.members.push(key.to_string());
        }
    }
}
