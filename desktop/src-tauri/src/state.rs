//! Shared application state. Fields are `Arc<Mutex<…>>` so commands can clone the
//! handles and move them into `spawn_blocking` for the few operations that block
//! (terminating the service, waiting for readiness, the pkexec prompt) without
//! holding the Tauri `State` borrow across an await.

use monadeck_core::cmd_runner::CmdRunner;
use monadeck_core::monado_conn::MonadoConn;
use monadeck_core::MonadeckConfig;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Mutex<MonadeckConfig>>,
    pub runner: Arc<Mutex<CmdRunner>>,
    /// PIDs of plugins we launched this session (so we could tidy them up).
    pub plugin_pids: Arc<Mutex<Vec<u32>>>,
    /// Persistent libmonado connection (one long-lived client, not per-poll).
    pub monado: Arc<MonadoConn>,
}

impl AppState {
    pub fn load() -> Self {
        Self {
            config: Arc::new(Mutex::new(MonadeckConfig::load())),
            runner: Arc::new(Mutex::new(CmdRunner::new())),
            plugin_pids: Arc::new(Mutex::new(Vec::new())),
            monado: Arc::new(MonadoConn::new()),
        }
    }
}
