//! The headset's speakers and microphone while VR runs: made the desktop's
//! defaults once VR is up (as soon as they show up: a headset brings its own a
//! moment after it powers on, a streamed one when it connects), handed back
//! when VR stops. See `monadeck_core::audio_devices`.

use crate::state::AppState;
use monadeck_core::audio_devices::{self, AudioDevices, Kind};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// How often the watch looks for the devices, and for VR having stopped.
const POLL: Duration = Duration::from_secs(2);

/// Watch this VR session: switch to the chosen devices when they're there,
/// and hand the old ones back if VR stops without us (a crash; Stop and
/// quitting hand them back themselves). A newer call takes over.
pub(crate) fn start(st: &AppState) {
    let gen = st.vr_audio_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let (output, input) = {
        let cfg = st.config.lock().unwrap();
        (cfg.vr_audio_output.as_ref().map(|d| d.name.clone()), cfg.vr_audio_input.as_ref().map(|d| d.name.clone()))
    };
    if output.is_none() && input.is_none() {
        return;
    }
    let st = st.clone();
    std::thread::spawn(move || {
        let mut settled = [false; 2];
        while st.vr_audio_gen.load(Ordering::SeqCst) == gen {
            if !st.runner.lock().unwrap().is_running() {
                restore(&st);
                return;
            }
            if !(settled[0] && settled[1]) {
                let mut done = st.vr_audio.lock().unwrap();
                settled[0] = settled[0] || audio_devices::switch(Kind::Output, output.as_deref(), &mut done);
                settled[1] = settled[1] || audio_devices::switch(Kind::Input, input.as_deref(), &mut done);
            }
            std::thread::sleep(POLL);
        }
    });
}

/// Hand back the devices VR took over, if any.
pub(crate) fn restore(st: &AppState) {
    let mut done = st.vr_audio.lock().unwrap();
    if done.output.is_some() || done.input.is_some() {
        audio_devices::restore(&mut done);
    }
}

/// Every output and microphone, for the Audio settings.
#[tauri::command]
pub async fn audio_devices() -> Result<AudioDevices, String> {
    tauri::async_runtime::spawn_blocking(audio_devices::list).await.map_err(|e| e.to_string())
}
