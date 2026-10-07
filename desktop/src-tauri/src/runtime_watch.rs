//! Keeps xrizer the OpenVR runtime while VR runs.
//!
//! SteamVR wakes itself when a Lighthouse controller's button is pressed (its
//! `power.autoLaunchSteamVROnButtonPress`, on by default), even with Monado
//! running. The first thing its startup does is put SteamVR back in
//! `openvrpaths.vrpath`; it then finds the headset taken and gives up, and
//! games launched after that load SteamVR instead of xrizer, so they start on
//! the desktop. The kernel tells us when the file changes (nothing runs in
//! between), and we put xrizer back; when VR stops, everything is handed back.

use crate::state::AppState;
use monadeck_core::active_runtime;
use monadeck_core::config::OvrRuntime;
use monadeck_core::openvr_paths::{self, OvrPathsKind};
use monadeck_core::watch::FileWatch;
use std::path::Path;

/// Whether VR holds the runtimes, and the watch on the OpenVR file meanwhile.
#[derive(Default)]
pub struct RuntimeWatch {
    active: bool,
    file: Option<FileWatch>,
}

/// Watch this VR session's OpenVR registration (an older session's watch ends).
pub(crate) fn start(st: &AppState) {
    let (runtime, xrizer) = {
        let cfg = st.config.lock().unwrap();
        (cfg.ovr_runtime, cfg.xrizer_path.clone())
    };
    let mut w = st.runtime_watch.lock().unwrap();
    w.file = None;
    w.active = true;
    let Some(xrizer) = xrizer.filter(|_| runtime == OvrRuntime::Xrizer) else { return };
    let watched = st.clone();
    match FileWatch::new(monadeck_core::paths::openvrpaths_path(), move || reclaim(&watched, &xrizer)) {
        Ok(f) => w.file = Some(f),
        Err(e) => log::warn!("can't watch the OpenVR runtime file ({e}): SteamVR may take it back mid-session"),
    }
}

/// The OpenVR file changed: if SteamVR took it while VR runs, take it back.
fn reclaim(st: &AppState, xrizer: &Path) {
    // Under the lock `hand_back` takes, so a stop never finds xrizer put back
    // after it handed the file over.
    let w = st.runtime_watch.lock().unwrap();
    if !w.active || !st.runner.lock().unwrap().is_running() || openvr_paths::kind() == OvrPathsKind::Xrizer {
        return;
    }
    match openvr_paths::set_to_xrizer(xrizer) {
        Ok(()) => {
            let note = "[monadeck] SteamVR made itself the OpenVR runtime again (it wakes when a controller's button \
                        is pressed): xrizer is back";
            log::info!("{note}");
            st.runner.lock().unwrap().note(note);
        }
        Err(e) => log::warn!("couldn't put xrizer back as the OpenVR runtime: {e}"),
    }
}

/// VR stopped: hand the OpenXR and OpenVR runtimes back to what they were
/// (SteamVR, usually), and end the watch.
pub(crate) fn hand_back(st: &AppState) {
    let mut w = st.runtime_watch.lock().unwrap();
    w.active = false;
    w.file = None;
    let _ = active_runtime::restore_backup();
    let _ = openvr_paths::restore_backup();
}
