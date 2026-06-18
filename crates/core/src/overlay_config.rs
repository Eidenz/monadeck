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
    /// Summon the dashboard tilted to match the headset's pitch (vs. always
    /// upright facing you).
    pub summon_tilt: bool,
    /// Distance the dashboard sits in front of you, metres.
    pub panel_dist: f32,
    /// Overall panel size multiplier (1.0 = default).
    pub panel_scale: f32,
    /// Cylinder curvature multiplier (1.0 = wraps around you; larger = flatter).
    pub panel_curve: f32,
    /// Playspace tracking-origin offset (OVRAS-style): metres + yaw in degrees.
    pub playspace_x: f32,
    pub playspace_y: f32,
    pub playspace_z: f32,
    pub playspace_yaw: f32,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            audio_enabled: true,
            audio_volume: 0.55,
            summon_tilt: false,
            panel_dist: 1.5,
            panel_scale: 1.0,
            panel_curve: 1.0,
            playspace_x: 0.0,
            playspace_y: 0.0,
            playspace_z: 0.0,
            playspace_yaw: 0.0,
        }
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
