//! WiVRn session watch — launches and stops the per-session helpers.
//!
//! With the Monado backend the service *is* the session: plugins and the
//! overlay start right after `monado-service` accepts IPC and die with it. WiVRn
//! is different: the server idles until a headset connects, forks a Monado IPC
//! child for that connection, and tears it down on disconnect — an OpenXR
//! client (our overlay, WayVR, …) started before that blocks forever, and one
//! that outlives the session is left dangling. So while the WiVRn backend runs,
//! this thread tracks the server's `SessionRunning` property and drives the
//! after-start plugins + overlay per session: launch on the rising edge, stop on
//! the falling edge. It exits when asked or when the server is gone.

use crate::commands::{kill_plugins, launch_session_plugins};
use crate::state::AppState;
use monadeck_core::wivrn;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

const POLL: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct WivrnSessionWatch {
    stop: Option<Arc<AtomicBool>>,
    handle: Option<JoinHandle<()>>,
}

impl WivrnSessionWatch {
    /// Start watching (replacing any previous watch). `st` is the shared app
    /// state; `st.wivrn_watch` must not be held by the caller (we don't touch it).
    pub fn spawn(&mut self, st: AppState) {
        self.stop_watch();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::Builder::new()
            .name("wivrn-session-watch".into())
            .spawn(move || run(st, flag))
            .expect("spawn wivrn session watch");
        self.stop = Some(stop);
        self.handle = Some(handle);
    }

    /// Ask the thread to stop and wait for it (it wakes every `POLL`).
    pub fn stop_watch(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run(st: AppState, stop: Arc<AtomicBool>) {
    let mut session_up = false;
    // Tolerate a couple of missed polls (bus hiccup) before deciding the server
    // is gone for good.
    let mut gone_streak = 0u32;
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(POLL);
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if !wivrn::dbus_name_owned() {
            gone_streak += 1;
            if session_up {
                log::info!("WiVRn server gone; stopping session helpers");
                kill_plugins(&st);
                session_up = false;
            }
            if gone_streak >= 6 {
                break;
            }
            continue;
        }
        gone_streak = 0;
        let running = wivrn::session_running();
        if running && !session_up {
            log::info!("WiVRn headset session started; launching plugins/overlay");
            launch_session_plugins(&st);
            session_up = true;
        } else if !running && session_up {
            log::info!("WiVRn headset session ended; stopping plugins/overlay");
            kill_plugins(&st);
            session_up = false;
        }
    }
    if session_up {
        kill_plugins(&st);
    }
}
