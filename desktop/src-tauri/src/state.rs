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
use std::sync::{Arc, Mutex};

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
            wivrn_watch: Arc::new(Mutex::new(WivrnSessionWatch::default())),
        }
    }
}
