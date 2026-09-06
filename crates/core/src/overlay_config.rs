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
    /// Seconds chihuahua waits after launching a UEVR game before injecting.
    pub uevr_delay: u32,
    /// Seconds to count down before a "Freeze controllers" actually applies, so
    /// you can settle into position first.
    pub freeze_delay_secs: f32,
    /// xdg-desktop-portal ScreenCast restore token (desktop viewer): lets the
    /// next launch re-use the approved monitors without the share dialog.
    pub screencast_token: Option<String>,
    /// Physical width of mirrored screens, metres.
    pub screen_width_m: f32,
    /// Bottom-bar order of the mirrored screens (output names, first = leftmost).
    pub screen_order: Vec<String>,
    /// Re-apply the last used desktop layout when the screens become available.
    pub restore_layout: bool,
    /// Show the wrist watch (left controller).
    pub watch_enabled: bool,
    /// Extra time zones on the watch (IANA names, e.g. "Asia/Tokyo").
    pub watch_timezones: Vec<String>,
    /// 24-hour clock on the watch and bottom bar.
    pub watch_24h: bool,
    /// Watch position locked (unlock from the watch to grip-move it).
    pub watch_locked: bool,
    /// Watch pose relative to the left controller's aim pose `[x,y,z,qx,qy,qz,qw]`
    /// (None = built-in default).
    pub watch_offset: Option<[f32; 7]>,
    /// Pause a screen's capture after a couple of seconds out of view.
    pub gaze_pause: bool,
    /// Double-B restore: bring screens back centred in view (one screen / one
    /// docked group) or head-relative (several undocked), unless it's an
    /// untouched loaded layout.
    pub recenter_on_toggle: bool,
    /// Restored screens tilt to match the headset's pitch (else upright).
    pub screen_restore_tilt: bool,
    /// VR keyboard size multiplier.
    pub keyboard_scale: f32,
    /// Cap the compositor's screencast frame rate (0 = unlimited). Frames above
    /// the headset rate are never seen, so this only saves compositor work.
    pub capture_max_fps: u32,
    /// Downscale mirrored screens to this height in VR (0 = native).
    pub capture_max_height: u32,
    /// 360° background while no game runs.
    pub skybox_enabled: bool,
    /// Custom equirectangular JPEG/PNG for the background (None = built-in).
    pub skybox_path: Option<String>,
    // Screenshots (from monado-frame).
    pub qr_detect: bool,
    pub qr_autodelete: bool,
    pub skip_wrist_photo: bool,
    pub skip_wrist_qr: bool,
    /// Delete screenshots older than this on launch (0 = keep forever).
    pub cleanup_days: i32,
    /// % trimmed off each edge of new framed shots (0 = off).
    pub crop_margin: i32,
    /// monado-frame's config.json was imported once.
    pub photos_settings_imported: bool,
    /// Mirror desktop (D-Bus) notifications as toasts.
    pub notifications_enabled: bool,
    /// Listen for XSOverlay-protocol notifications (udp/42069).
    pub notifications_xso: bool,
    pub notifications_sound: bool,
    /// Notification sound level, 0 (mute) ..= 1, on top of the UI volume.
    pub notifications_volume: f32,
    /// The watch's four quick buttons (ids: keyboard, recenter, layouts, freeze,
    /// timer, screenshot, screens, mute, photos).
    pub watch_buttons: Vec<String>,
    /// Keyboard follows its docked screen's visibility, and pops up when a text
    /// field gets focus on the desktop (accessibility bus). Off by default.
    pub keyboard_auto: bool,
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
            uevr_delay: 30,
            freeze_delay_secs: 3.0,
            screencast_token: None,
            screen_width_m: 1.35,
            screen_order: Vec::new(),
            restore_layout: true,
            watch_enabled: true,
            watch_timezones: vec!["America/New_York".into(), "Asia/Tokyo".into()],
            watch_24h: false,
            watch_locked: true,
            watch_offset: None,
            gaze_pause: true,
            recenter_on_toggle: true,
            screen_restore_tilt: false,
            keyboard_scale: 1.0,
            capture_max_fps: 90,
            capture_max_height: 0,
            skybox_enabled: true,
            skybox_path: None,
            qr_detect: false,
            qr_autodelete: false,
            skip_wrist_photo: false,
            skip_wrist_qr: false,
            cleanup_days: 0,
            crop_margin: 0,
            photos_settings_imported: false,
            notifications_enabled: true,
            notifications_xso: true,
            notifications_sound: true,
            notifications_volume: 0.7,
            watch_buttons: vec!["keyboard".into(), "recenter".into(), "layouts".into(), "freeze".into()],
            keyboard_auto: false,
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
