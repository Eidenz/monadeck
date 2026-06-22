//! Monadeck desktop backend. Thin Tauri command layer over `monadeck-core`:
//! it owns the app state (config, the running service, launched plugin processes) and
//! exposes a flat command surface to the SvelteKit frontend, mirroring the
//! `invoke(...)` style used in NemuriXR/udcap-control.

mod beyond;
mod bindings;
mod commands;
mod overlay;
mod state;

use state::AppState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

/// Cleanly shut down and quit: stop the service (SIGTERM→SIGKILL, so it releases
/// the HMD/DRM lease), hand the runtime files back, then exit.
fn cleanup_and_exit(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.runner.lock().unwrap().terminate();
    }
    let _ = monadeck_core::active_runtime::restore_backup();
    let _ = monadeck_core::openvr_paths::restore_backup();
    app.exit(0);
}

fn toggle_deck(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.unminimize();
            let _ = win.set_focus();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::load())
        .setup(|app| {
            // Tray icon: left-click toggles the deck; menu shows it or quits.
            let show = MenuItem::with_id(app, "show", "Show Monadeck", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("Monadeck")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(win) = app.get_webview_window("main") {
                            let _ = win.show();
                            let _ = win.unminimize();
                            let _ = win.set_focus();
                        }
                    }
                    "quit" => cleanup_and_exit(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_deck(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        // Closing the deck either hides it to the tray (default) or quits. The
        // settings window only hides on close (it would otherwise keep the
        // process alive); the deck is what governs quitting.
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let app = window.app_handle();
                    let to_tray = app
                        .try_state::<AppState>()
                        .map(|s| s.config.lock().unwrap().minimize_to_tray)
                        .unwrap_or(false);
                    if to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                    } else {
                        cleanup_and_exit(app);
                    }
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
            commands::amd_gpu,
            commands::has_nvidia,
            commands::set_amd_vr_profile,
            commands::import_openxr_status,
            commands::write_import_openxr,
            commands::preflight_check,
            commands::floor_cal_status,
            commands::run_floor_calibration,
            commands::survive_cal_status,
            commands::run_survive_calibration,
            commands::install_builtin_monado,
            commands::install_builtin_xrizer,
            commands::uevr_status,
            commands::install_chihuahua,
            commands::list_installed_apps,
            commands::launch_plugin,
            bindings::scan_steam_games,
            bindings::load_game_bindings,
            bindings::read_json_file,
            bindings::write_json_file,
            bindings::game_cover,
            bindings::set_custom_cover,
            bindings::remove_custom_cover,
            bindings::get_custom_paths,
            bindings::set_custom_paths,
            beyond::beyond_present,
            beyond::eyetracking_status,
            beyond::eyetracking_start,
            beyond::eyetracking_stop,
            beyond::install_bsbcams_cmd,
            beyond::install_bsbcams_rule,
            beyond::set_bsbcams_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Monadeck");
}
