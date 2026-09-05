//! The finger-frame gesture config the Monado fork's detector hot-reloads
//! (`~/.config/monado/gestures.json`). Ported from monado-frame.
use std::{env, fs, path::Path};

#[derive(Clone, PartialEq)]
pub struct Settings {
    pub enabled: bool,
    pub hold_ms: i32,
    pub frame_feedback: bool,
    pub debug: bool,
}

pub fn config_path() -> String {
    let base = env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}/.config", env::var("HOME").unwrap_or_default()));
    format!("{base}/monado/gestures.json")
}

pub fn load() -> Settings {
    let mut s = Settings { enabled: true, hold_ms: 2000, frame_feedback: true, debug: false };
    if let Ok(txt) = fs::read_to_string(config_path()) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
            if let Some(b) = v.get("enabled").and_then(|x| x.as_bool()) {
                s.enabled = b;
            }
            if let Some(n) = v.get("hold_ms").and_then(|x| x.as_i64()) {
                s.hold_ms = n as i32;
            }
            if let Some(b) = v.get("frame_feedback").and_then(|x| x.as_bool()) {
                s.frame_feedback = b;
            }
            if let Some(b) = v.get("debug").and_then(|x| x.as_bool()) {
                s.debug = b;
            }
        }
    }
    s
}

pub fn save(s: &Settings) {
    let path = config_path();
    if let Some(dir) = Path::new(&path).parent() {
        let _ = fs::create_dir_all(dir);
    }
    let v = serde_json::json!({ "enabled": s.enabled, "hold_ms": s.hold_ms, "frame_feedback": s.frame_feedback, "debug": s.debug });
    match serde_json::to_string_pretty(&v) {
        Ok(txt) => match fs::write(&path, txt) {
            Ok(()) => log::info!("wrote {path} (enabled={} hold_ms={})", s.enabled, s.hold_ms),
            Err(e) => log::warn!("failed to write {path}: {e}"),
        },
        Err(e) => log::warn!("serialise gesture config: {e}"),
    }
}
