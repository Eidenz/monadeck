//! Tauri command surface. Each command is a thin wrapper over `monadeck-core`;
//! the blocking ones (start/stop/setcap) run on `spawn_blocking` so the IPC
//! runtime stays free.

use crate::state::AppState;
use monadeck_core::active_runtime::{self, ActiveRuntimeKind};
use monadeck_core::config::MonadeckConfig;
use monadeck_core::desktop::{self, InstalledApp};
use monadeck_core::devices::{self, Snapshot};
use monadeck_core::launch_options;
use monadeck_core::openvr_paths::{self, OvrPathsKind};
use monadeck_core::plugins::ExecWhen;
use monadeck_core::setcap::{self, CapStatus};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tauri::State;

type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
pub struct ServiceStatus {
    /// Our child process is alive.
    running: bool,
    /// libmonado can reach the service (it's actually serving IPC).
    connected: bool,
    exit_code: Option<i32>,
}

#[derive(Serialize)]
pub struct RuntimeStatus {
    openxr: ActiveRuntimeKind,
    openvr: OvrPathsKind,
}

#[derive(Serialize)]
pub struct LogChunk {
    cursor: u64,
    lines: Vec<String>,
}

#[tauri::command]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> MonadeckConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_config(state: State<AppState>, config: MonadeckConfig) -> CmdResult<()> {
    config.save().map_err(|e| e.to_string())?;
    *state.config.lock().unwrap() = config;
    Ok(())
}

/// Search `$PATH` for an executable.
fn which(name: &str) -> Option<PathBuf> {
    std::env::var("PATH").ok()?.split(':').find_map(|dir| {
        let p = PathBuf::from(dir).join(name);
        p.is_file().then_some(p)
    })
}

/// Best-effort guess at the monado build prefix: prefer `monado-service` on
/// `$PATH`, fall back to deriving it from the current active runtime.
/// Best-effort guess at the xrizer runtime directory.
#[tauri::command]
pub fn autodetect_xrizer() -> Option<String> {
    launch_options::detect_xrizer_path().map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub fn autodetect_prefix() -> Option<String> {
    if let Some(bin) = which("monado-service") {
        if let Some(prefix) = bin.parent().and_then(|b| b.parent()) {
            return Some(prefix.to_string_lossy().to_string());
        }
    }
    if let Some(ar) = active_runtime::current() {
        let lib = ar.runtime.library_path;
        if lib.to_string_lossy().to_lowercase().contains("monado") {
            if let Some(prefix) = lib.parent().and_then(|l| l.parent()) {
                return Some(prefix.to_string_lossy().to_string());
            }
        }
    }
    None
}

// These run on a blocking task (not the main thread): they lock the runner mutex
// — which stop_service holds for up to ~2s while terminating — or touch the
// filesystem. A sync command would block the UI thread and freeze the window.
#[tauri::command]
pub async fn service_status(state: State<'_, AppState>) -> CmdResult<ServiceStatus> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runner = st.runner.lock().unwrap();
        let running = runner.is_running();
        let exit_code = match runner.status() {
            monadeck_core::cmd_runner::RunnerStatus::Stopped(c) => c,
            _ => None,
        };
        ServiceStatus {
            running,
            connected: devices::service_connected(),
            exit_code,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn runtime_status() -> CmdResult<RuntimeStatus> {
    tauri::async_runtime::spawn_blocking(|| RuntimeStatus {
        openxr: active_runtime::kind(),
        openvr: openvr_paths::kind(),
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn capabilities_status(state: State<'_, AppState>) -> CmdResult<String> {
    let bin = state.config.lock().unwrap().monado_service_bin();
    tauri::async_runtime::spawn_blocking(move || {
        match setcap::status(&bin) {
            CapStatus::Set => "set",
            CapStatus::NeedsSetcap => "needs_setcap",
            CapStatus::NoBinary => "no_binary",
            CapStatus::NoTooling => "no_tooling",
        }
        .to_string()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_capabilities(state: State<'_, AppState>) -> CmdResult<()> {
    let bin = state.config.lock().unwrap().monado_service_bin();
    tauri::async_runtime::spawn_blocking(move || setcap::apply(&bin).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_snapshot(state: State<'_, AppState>) -> CmdResult<Snapshot> {
    // Routed through the persistent connection worker: one long-lived libmonado
    // client (no connect/disconnect churn in monado's log), off the UI thread.
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || st.monado.snapshot())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_logs(state: State<'_, AppState>, since: u64) -> CmdResult<LogChunk> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (cursor, lines) = st.runner.lock().unwrap().lines_since(since);
        LogChunk { cursor, lines }
    })
    .await
    .map_err(|e| e.to_string())
}

fn env_map(cfg: &MonadeckConfig) -> HashMap<String, String> {
    cfg.environment
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[tauri::command]
pub async fn start_service(state: State<'_, AppState>) -> CmdResult<()> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        let cfg = st.config.lock().unwrap().clone();
        if !cfg.prefix_looks_valid() {
            return Err(format!(
                "monado-service not found at {}",
                cfg.monado_service_bin().display()
            ));
        }

        // Wire up runtimes (each backs up what it replaces).
        active_runtime::set_to_monado(&cfg).map_err(|e| e.to_string())?;
        if cfg.ovr_runtime == monadeck_core::config::OvrRuntime::Xrizer {
            if let Some(xr) = cfg.xrizer_path.as_ref() {
                openvr_paths::set_to_xrizer(xr).map_err(|e| e.to_string())?;
            }
        }

        let mut env = env_map(&cfg);
        // Pick the lighthouse driver exactly like Envision: the SteamVR wrapper
        // (for the Beyond / SteamVR-tracked HMDs) is enabled via STEAMVR_LH_ENABLE
        // and must NOT set LH_DRIVER (LH_DRIVER=steamvr actively errors); vive and
        // survive go through LH_DRIVER. An explicit user LH_DRIVER wins.
        if !env.contains_key("LH_DRIVER") {
            if cfg.lighthouse_driver.eq_ignore_ascii_case("steamvr") {
                env.entry("STEAMVR_LH_ENABLE".to_string())
                    .or_insert_with(|| "true".to_string());
            } else {
                env.insert("LH_DRIVER".to_string(), cfg.lighthouse_driver.to_lowercase());
            }
        }
        let bin = cfg.monado_service_bin();
        st.runner
            .lock()
            .unwrap()
            .start(&bin.to_string_lossy(), &[], &env)
            .map_err(|e| format!("failed to start monado-service: {e}"))?;

        // Wait briefly for the service to accept IPC before launching plugins.
        for _ in 0..25 {
            if devices::service_connected() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }

        let mut pids = st.plugin_pids.lock().unwrap();
        for p in cfg
            .plugins
            .iter()
            .filter(|p| p.enabled && p.when == ExecWhen::AfterStart)
        {
            match p.launch(&env) {
                Ok(pid) => pids.push(pid),
                Err(e) => log::warn!("plugin '{}' failed to launch: {e}", p.name),
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn stop_service(state: State<'_, AppState>) -> CmdResult<()> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        st.runner.lock().unwrap().terminate();

        let cfg = st.config.lock().unwrap().clone();
        let env = env_map(&cfg);
        for p in cfg
            .plugins
            .iter()
            .filter(|p| p.enabled && p.when == ExecWhen::AfterStop)
        {
            if let Err(e) = p.launch(&env) {
                log::warn!("after-stop plugin '{}' failed: {e}", p.name);
            }
        }

        // Hand the runtimes back so SteamVR keeps working when monado is off.
        let _ = active_runtime::restore_backup();
        let _ = openvr_paths::restore_backup();
        st.plugin_pids.lock().unwrap().clear();
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Installed `.desktop` applications, for the "add installed app" plugin picker.
#[tauri::command]
pub async fn list_installed_apps() -> CmdResult<Vec<InstalledApp>> {
    tauri::async_runtime::spawn_blocking(desktop::list_installed_apps)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn launch_plugin(state: State<AppState>, index: usize) -> CmdResult<u32> {
    let cfg = state.config.lock().unwrap().clone();
    let plugin = cfg
        .plugins
        .get(index)
        .ok_or_else(|| "no such plugin".to_string())?;
    let pid = plugin.launch(&env_map(&cfg)).map_err(|e| e.to_string())?;
    state.plugin_pids.lock().unwrap().push(pid);
    Ok(pid)
}
