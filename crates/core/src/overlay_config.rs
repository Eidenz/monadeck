//! Persisted in-headset overlay preferences (separate from the desktop config).
//! Currently just UI-sound settings; room to grow (panel distance, curve, etc.).
use crate::paths::monadeck_config_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayConfig {
    pub audio_enabled: bool,
    pub audio_volume: f32,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self { audio_enabled: true, audio_volume: 0.55 }
    }
}

impl OverlayConfig {
    fn file() -> PathBuf {
        monadeck_config_dir().join("overlay.json")
    }

    pub fn load() -> Self {
        fs::read_to_string(Self::file())
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::file();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }
}
