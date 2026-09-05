//! Persisted desktop-viewer layouts: named presets of the whole VR screen
//! arrangement (which screens are shown, where, how big, how curved) plus the
//! keyboard's spot — e.g. "Standing" vs "Lying down". Poses are in the OpenXR
//! LOCAL space, which is stable across sessions unless the playspace is
//! recentred.
use crate::paths::monadeck_config_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// `[x, y, z, qx, qy, qz, qw]`
pub type Pose = [f32; 7];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenPlacement {
    /// Output name (e.g. `DP-3`).
    pub name: String,
    pub shown: bool,
    pub pose: Pose,
    pub width_m: f32,
    #[serde(default)]
    pub curve: f32,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default)]
    pub keep_in_game: bool,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardPlacement {
    pub visible: bool,
    /// Docked under this screen (output name), else free at `pose`.
    pub attached: Option<String>,
    pub pose: Pose,
    #[serde(default = "one")]
    pub scale: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopLayout {
    pub name: String,
    pub screens: Vec<ScreenPlacement>,
    pub keyboard: Option<KeyboardPlacement>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopLayouts {
    pub layouts: Vec<DesktopLayout>,
    /// Name of the layout applied/saved most recently (restored on start).
    pub last_used: Option<String>,
}

fn file() -> PathBuf {
    monadeck_config_dir().join("desktop_layouts.json")
}

pub fn load() -> DesktopLayouts {
    fs::read_to_string(file())
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

pub fn save(store: &DesktopLayouts) {
    let path = file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(store) {
        let _ = fs::write(path, json);
    }
}

impl DesktopLayouts {
    pub fn find(&self, name: &str) -> Option<&DesktopLayout> {
        self.layouts.iter().find(|l| l.name == name)
    }

    /// Insert or replace by name.
    pub fn upsert(&mut self, layout: DesktopLayout) {
        match self.layouts.iter_mut().find(|l| l.name == layout.name) {
            Some(slot) => *slot = layout,
            None => self.layouts.push(layout),
        }
    }

    pub fn rename(&mut self, idx: usize, name: String) {
        if let Some(l) = self.layouts.get_mut(idx) {
            if self.last_used.as_deref() == Some(l.name.as_str()) {
                self.last_used = Some(name.clone());
            }
            l.name = name;
        }
    }

    pub fn move_by(&mut self, idx: usize, delta: i32) -> bool {
        let j = idx as i32 + delta;
        if idx >= self.layouts.len() || j < 0 || j as usize >= self.layouts.len() {
            return false;
        }
        self.layouts.swap(idx, j as usize);
        true
    }

    pub fn remove(&mut self, idx: usize) {
        if idx < self.layouts.len() {
            let name = self.layouts.remove(idx).name;
            if self.last_used.as_deref() == Some(name.as_str()) {
                self.last_used = None;
            }
        }
    }
}
