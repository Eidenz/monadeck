//! Monadeck desktop backend. Thin Tauri command layer over `monadeck-core`:
//! it owns the app state (config, the running service, launched plugin PIDs) and
//! exposes a flat command surface to the SvelteKit frontend, mirroring the
//! `invoke(...)` style used in NemuriXR/udcap-control.

mod commands;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::load())
        // The settings window only hides on close, so it would keep the process
        // alive after the deck is closed. When the deck closes: hand the runtime
        // files back (best-effort) and quit the whole app. The monado-service
        // child exits on its own when our process dies (it sees EOF on the stdin
        // pipe we hold). A hard kill skips this — that's what Stop is for.
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    let _ = monadeck_core::active_runtime::restore_backup();
                    let _ = monadeck_core::openvr_paths::restore_backup();
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::get_config,
            commands::set_config,
            commands::autodetect_prefix,
            commands::autodetect_xrizer,
            commands::service_status,
            commands::runtime_status,
            commands::capabilities_status,
            commands::apply_capabilities,
            commands::start_service,
            commands::stop_service,
            commands::get_snapshot,
            commands::get_logs,
            commands::list_installed_apps,
            commands::launch_plugin,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Monadeck");
}
