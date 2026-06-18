//! Tauri command surface. Each command is a thin wrapper over `monadeck-core`;
//! the blocking ones (start/stop/setcap) run on `spawn_blocking` so the IPC
//! runtime stays free.

use crate::state::AppState;
use monadeck_core::active_runtime::{self, ActiveRuntimeKind};
use monadeck_core::config::MonadeckConfig;
use monadeck_core::config::OvrRuntime;
use monadeck_core::desktop::{self, InstalledApp};
use monadeck_core::devices::{self, Snapshot};
use monadeck_core::gpu::{self, AmdGpu};
use monadeck_core::installer::{self, Installed};
use monadeck_core::proton;
use monadeck_core::launch_options;
use monadeck_core::openvr_paths::{self, OvrPathsKind};
use monadeck_core::plugins::ExecWhen;
use monadeck_core::preflight::{self, PreflightReport};
use monadeck_core::setcap::{self, CapStatus};
use monadeck_core::uevr;
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

/// Stop and reap every plugin/overlay child we launched on service start.
fn kill_plugins(st: &AppState) {
    let mut children = st.plugin_children.lock().unwrap();
    for mut child in children.drain(..) {
        monadeck_core::plugins::terminate(&mut child);
    }
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
        // Compositor settings, injected like Envision's profile defaults. An
        // explicit user env var always wins (or_insert).
        if cfg.render_scale != 100 {
            env.entry("XRT_COMPOSITOR_SCALE_PERCENTAGE".to_string())
                .or_insert_with(|| cfg.render_scale.to_string());
        }
        if cfg.min_frame_period {
            env.entry("U_PACING_APP_USE_MIN_FRAME_PERIOD".to_string())
                .or_insert_with(|| "1".to_string());
        }
        if cfg.compute_compositor {
            env.entry("XRT_COMPOSITOR_COMPUTE".to_string())
                .or_insert_with(|| "1".to_string());
        }
        if cfg.debug_gui {
            env.entry("XRT_DEBUG_GUI".to_string())
                .or_insert_with(|| "1".to_string());
            env.entry("XRT_CURATED_GUI".to_string())
                .or_insert_with(|| "1".to_string());
        }
        // Simulated headset for testing the overlay without hardware. The
        // simulated builder is off unless SIMULATED_ENABLE is set; add simple
        // controllers too so there's something to point the laser with.
        if cfg.simulated_hmd {
            env.entry("SIMULATED_ENABLE".to_string())
                .or_insert_with(|| "1".to_string());
            env.entry("SIMULATED_LEFT".to_string())
                .or_insert_with(|| "simple".to_string());
            env.entry("SIMULATED_RIGHT".to_string())
                .or_insert_with(|| "simple".to_string());
        }
        // NVIDIA compositor mitigations — only when an NVIDIA GPU is actually
        // present (so it's a no-op for AMD users / portable to nvidia friends).
        if cfg.nvidia_mitigation && gpu::has_nvidia_gpu() {
            env.entry("U_PACING_COMP_TIME_FRACTION_PERCENT".to_string())
                .or_insert_with(|| "95".to_string());
            env.entry("XRT_COMPOSITOR_USE_PRESENT_WAIT".to_string())
                .or_insert_with(|| "1".to_string());
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

        // Clean slate: stop anything still tracked from a previous run before
        // launching fresh, so we never end up with two of the same plugin.
        kill_plugins(&st);
        let mut children = st.plugin_children.lock().unwrap();
        for p in cfg
            .plugins
            .iter()
            .filter(|p| p.enabled && p.when == ExecWhen::AfterStart)
        {
            match p.launch(&env) {
                Ok(child) => children.push(child),
                Err(e) => log::warn!("plugin '{}' failed to launch: {e}", p.name),
            }
        }
        // Built-in in-headset overlay — the permanent auto-launch entry.
        if cfg.overlay_enabled {
            match crate::overlay::launch(&env) {
                Ok(child) => children.push(child),
                Err(e) => log::warn!("built-in overlay failed to launch: {e}"),
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
        // Stop the plugins/overlay we launched on start (WayVR, etc.) so they don't
        // outlive the service and collide with the next start.
        kill_plugins(&st);

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
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Detected AMD GPU + its current power profile (None if not an AMD card).
#[tauri::command]
pub async fn amd_gpu() -> Option<AmdGpu> {
    tauri::async_runtime::spawn_blocking(gpu::find_amd_gpu)
        .await
        .ok()
        .flatten()
}

/// Whether an NVIDIA GPU is present (drives the mitigations toggle visibility).
#[tauri::command]
pub fn has_nvidia() -> bool {
    gpu::has_nvidia_gpu()
}

/// Set the AMD VR power profile (prompts for a password via pkexec).
#[tauri::command]
pub async fn set_amd_vr_profile() -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(|| gpu::set_vr_profile().map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Whether `PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES` is set session-wide.
#[tauri::command]
pub fn import_openxr_status() -> bool {
    proton::is_set()
}

/// Write the `environment.d` config that sets it (needs a reboot/relogin).
#[tauri::command]
pub fn write_import_openxr() -> CmdResult<()> {
    proton::write_env_file().map_err(|e| e.to_string())
}

/// Runtime prerequisite checks (xr-hardware udev rules, pkexec). All-ok on a
/// properly set-up box; surfaces what's missing when run on another machine.
#[tauri::command]
pub async fn preflight_check() -> CmdResult<PreflightReport> {
    tauri::async_runtime::spawn_blocking(preflight::run)
        .await
        .map_err(|e| e.to_string())
}

/// Download + install the latest portable Monado fork build from GitHub Releases,
/// then point the config's prefix at it. Blocking (network + extract).
#[tauri::command]
pub async fn install_builtin_monado(state: State<'_, AppState>) -> CmdResult<Installed> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<Installed> {
        let installed = installer::install_monado().map_err(|e| e.to_string())?;
        let mut cfg = st.config.lock().unwrap();
        cfg.monado_prefix = PathBuf::from(&installed.path);
        cfg.save().map_err(|e| e.to_string())?;
        Ok(installed)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Download + install the latest xrizer release and register it as the OpenVR
/// runtime in config (path + ovr_runtime). Blocking (network + extract).
#[tauri::command]
pub async fn install_builtin_xrizer(state: State<'_, AppState>) -> CmdResult<Installed> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<Installed> {
        let installed = installer::install_xrizer().map_err(|e| e.to_string())?;
        let mut cfg = st.config.lock().unwrap();
        cfg.xrizer_path = Some(PathBuf::from(&installed.path));
        cfg.ovr_runtime = OvrRuntime::Xrizer;
        cfg.save().map_err(|e| e.to_string())?;
        Ok(installed)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// UEVR ("VR Mod") tooling status for the desktop settings card: whether
/// `protontricks-launch` is on PATH, and where the chihuahua injector is (if any).
#[derive(Serialize)]
pub struct UevrStatus {
    pub protontricks: bool,
    pub chihuahua: Option<String>,
}

#[tauri::command]
pub async fn uevr_status() -> UevrStatus {
    tauri::async_runtime::spawn_blocking(|| UevrStatus {
        protontricks: uevr::protontricks_available(),
        chihuahua: uevr::detect_chihuahua().map(|p| p.to_string_lossy().into_owned()),
    })
    .await
    .unwrap_or(UevrStatus { protontricks: false, chihuahua: None })
}

/// Download the chihuahua injector ahead of time. `force` re-downloads the latest
/// even if a copy already exists; otherwise it's a no-op when one is present.
/// Returns the resolved path. Blocking (network + unzip).
#[tauri::command]
pub async fn install_chihuahua(force: bool) -> CmdResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let r = if force { uevr::reinstall_chihuahua() } else { uevr::ensure_chihuahua() };
        r.map(|p| p.to_string_lossy().into_owned()).map_err(|e| e.to_string())
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
    let child = plugin.launch(&env_map(&cfg)).map_err(|e| e.to_string())?;
    let pid = child.id();
    state.plugin_children.lock().unwrap().push(child);
    Ok(pid)
}
