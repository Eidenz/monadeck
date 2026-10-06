//! The headset's speakers and microphone while VR runs: made the desktop's
//! defaults whenever they show up (a headset brings its own a moment after it
//! powers on; WiVRn's come and go with the headset's connection), handed back
//! when VR stops. See `monadeck_core::audio_devices`.

use crate::state::AppState;
use monadeck_core::audio_devices::{self, AudioDevices, Kind};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// How often the watch looks for the devices, and for VR having stopped.
const POLL: Duration = Duration::from_secs(2);

/// Watch this VR session: switch to the chosen devices each time they appear
/// (and only then, so a pick of your own while they're there stays), and hand
/// the old ones back if VR stops without us (a crash; Stop and quitting hand
/// them back themselves). A newer call takes over.
pub(crate) fn start(st: &AppState) {
    let gen = st.vr_audio_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let (output, input) = st.config.lock().unwrap().vr_audio_targets();
    if output.is_none() && input.is_none() {
        return;
    }
    let st = st.clone();
    std::thread::spawn(move || {
        // Per device: there, and made the default since it appeared.
        let mut taken = [false; 2];
        while st.vr_audio_gen.load(Ordering::SeqCst) == gen {
            if !st.runner.lock().unwrap().is_running() {
                restore(&st);
                return;
            }
            for (i, (kind, want)) in [(Kind::Output, &output), (Kind::Input, &input)].into_iter().enumerate() {
                let Some(want) = want.as_deref() else { continue };
                if !audio_devices::present(kind, want) {
                    taken[i] = false;
                } else if !taken[i] {
                    let mut done = st.vr_audio.lock().unwrap();
                    taken[i] = audio_devices::switch(kind, Some(want), &mut done);
                }
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
