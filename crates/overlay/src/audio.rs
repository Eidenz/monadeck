//! Embedded UI confirmation sounds. The WAVs are baked into the binary
//! (`include_bytes!`) so there are no asset files to ship, and rodio plays them
//! through the system default output (the headset in VR) — cross-platform via
//! cpal. Gracefully silent if no audio device is available.
use std::io::Cursor;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Source};

static SELECT: &[u8] = include_bytes!("../assets/sounds/select.wav");
static LAUNCH: &[u8] = include_bytes!("../assets/sounds/launch.wav");
static TAB: &[u8] = include_bytes!("../assets/sounds/tab.wav");
static ALARM: &[u8] = include_bytes!("../assets/sounds/alarm.wav");

pub struct Audio {
    // Kept alive for the stream to keep playing; not Send, so Audio lives on the
    // render thread (where it's triggered).
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    enabled: bool,
    volume: f32,
}

impl Audio {
    pub fn new(enabled: bool, volume: f32) -> Self {
        match OutputStream::try_default() {
            Ok((stream, handle)) => {
                log::info!("audio ready");
                Self { _stream: Some(stream), handle: Some(handle), enabled, volume }
            }
            Err(e) => {
                log::warn!("UI sounds off: no audio output ({e})");
                Self { _stream: None, handle: None, enabled, volume }
            }
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    fn play(&self, bytes: &'static [u8]) {
        if !self.enabled || self.volume <= 0.001 {
            return;
        }
        let Some(handle) = &self.handle else { return };
        match Decoder::new(Cursor::new(bytes)) {
            Ok(decoder) => {
                let src = decoder.convert_samples::<f32>().amplify(self.volume);
                let _ = handle.play_raw(src);
            }
            Err(e) => log::warn!("decode UI sound: {e}"),
        }
    }

    /// Selecting a game (soft tick).
    pub fn select(&self) {
        self.play(SELECT);
    }
    /// Launching a game (two-tone confirm).
    pub fn launch(&self) {
        self.play(LAUNCH);
    }
    /// Switching a side tab / recenter (minimal blip).
    pub fn tab(&self) {
        self.play(TAB);
    }
    /// Toast notification / timer fired (rising chime).
    pub fn alarm(&self) {
        self.play(ALARM);
    }
}
