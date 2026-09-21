//! Shared application state. Fields are `Arc<Mutex<…>>` so commands can clone the
//! handles and move them into `spawn_blocking` for the few operations that block
//! (terminating the service, waiting for readiness, the pkexec prompt) without
//! holding the Tauri `State` borrow across an await.

use crate::wivrn_watch::WivrnSessionWatch;
use monadeck_core::cmd_runner::CmdRunner;
use monadeck_core::kwin_freeze::KwinFreezeWatch;
use monadeck_core::monado_conn::MonadoConn;
use monadeck_core::MonadeckConfig;
use std::process::Child;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// How the automatic restart after a freeze went.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartState {
    /// No restart for this event (it only disabled an output, or this was the
    /// second freeze in a row).
    None,
    Pending,
    Ok,
    Failed,
}

/// What the kwin freeze watch did, kept until the next manual start. Every
/// window polls the same report (the watch's own result is take-once, which
/// used to leave whichever window lost the race thinking Monado had crashed).
#[derive(Clone, Debug, Serialize)]
pub struct FreezeReport {
    /// Bumped per freeze event, so a window can remember a dismissal.
    pub seq: u64,
    pub disabled_outputs: Vec<String>,
    pub service_stopped: bool,
    pub restarting: bool,
    pub restart: RestartState,
    pub restart_error: Option<String>,
    #[serde(skip)]
    pub created: Instant,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Mutex<MonadeckConfig>>,
    pub runner: Arc<Mutex<CmdRunner>>,
    /// Separate runner for the go-bsb-cams Bigscreen Beyond eye-camera server,
    /// managed independently of the monado-service lifecycle.
    pub eye_runner: Arc<Mutex<CmdRunner>>,
    /// Child handles for the plugins/overlay we launched on service start, so we
    /// can stop and reap them on service stop (otherwise they linger and a fresh
    /// start spawns colliding second instances).
    pub plugin_children: Arc<Mutex<Vec<Child>>>,
    /// Persistent libmonado connection (one long-lived client, not per-poll).
    pub monado: Arc<MonadoConn>,
    /// Short-lived watch for the kwin cold-start HMD-adoption freeze, armed
    /// around each service launch. See core::kwin_freeze.
    pub freeze_watch: Arc<Mutex<KwinFreezeWatch>>,
    /// The freeze watch already restarted the service once since the last
    /// manual start: a second stuck launch is stopped but not retried again.
    pub freeze_auto_restarted: Arc<AtomicBool>,
    /// The freeze event as every window should see it.
    pub freeze_report: Arc<Mutex<Option<FreezeReport>>>,
    pub freeze_seq: Arc<AtomicU64>,
    /// The freeze watch is stopping + restarting the service right now: the UI
    /// keeps showing "Warming up…" instead of flashing "Stopped".
    pub recovering: Arc<AtomicBool>,
    /// Stop was pressed while recovering: don't start it again behind their back.
    pub recovery_cancelled: Arc<AtomicBool>,
    /// The last stop was ours (the Stop button or the freeze recovery), not a
    /// crash. Cleared when the service is started.
    pub deliberate_stop: Arc<AtomicBool>,
    /// WiVRn backend only: tracks the server's headset session and launches /
    /// stops the plugins + overlay per session. See `wivrn_watch`.
    pub wivrn_watch: Arc<Mutex<WivrnSessionWatch>>,
}

impl AppState {
    pub fn load() -> Self {
        let config = MonadeckConfig::load();
        // The socket helpers (and every child we spawn) dispatch on this.
        monadeck_core::devices::set_backend(config.backend);
        Self {
            config: Arc::new(Mutex::new(config)),
            runner: Arc::new(Mutex::new(CmdRunner::new())),
            eye_runner: Arc::new(Mutex::new(CmdRunner::new())),
            plugin_children: Arc::new(Mutex::new(Vec::new())),
            monado: Arc::new(MonadoConn::new()),
            freeze_watch: Arc::new(Mutex::new(KwinFreezeWatch::default())),
            freeze_auto_restarted: Arc::new(AtomicBool::new(false)),
            freeze_report: Arc::new(Mutex::new(None)),
            freeze_seq: Arc::new(AtomicU64::new(0)),
            recovering: Arc::new(AtomicBool::new(false)),
            recovery_cancelled: Arc::new(AtomicBool::new(false)),
            deliberate_stop: Arc::new(AtomicBool::new(false)),
            wivrn_watch: Arc::new(Mutex::new(WivrnSessionWatch::default())),
        }
    }
}
