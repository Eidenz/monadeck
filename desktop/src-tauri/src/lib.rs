//! Monadeck desktop backend. Thin Tauri command layer over `monadeck-core`:
//! it owns the app state (config, the running service, launched plugin processes) and
//! exposes a flat command surface to the SvelteKit frontend, mirroring the
//! `invoke(...)` style used in NemuriXR/udcap-control.

mod beyond;
mod bindings;
mod commands;
mod gamepad;
mod lighthouse;
mod overlay;
mod runtime_watch;
mod state;
mod vr_audio;
mod wivrn_watch;

use state::AppState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// Cleanly shut down and quit: stop the service (SIGTERM→SIGKILL, so it releases
/// the HMD/DRM lease), hand the runtime files back, then exit.
fn cleanup_and_exit(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.wivrn_watch.lock().unwrap().stop_watch();
        let was_running = state.runner.lock().unwrap().is_running();
        let grace = commands::stop_grace(&state.config.lock().unwrap());
        state.monado.release();
        state.runner.lock().unwrap().terminate_within(grace);
        vr_audio::restore(&state);
        if was_running {
            // Out of sight while the base stations are switched off.
            for win in app.webview_windows().values() {
                let _ = win.hide();
            }
            lighthouse::switch_base_stations_off_now(&state);
        }
        runtime_watch::hand_back(&state);
    } else {
        let _ = monadeck_core::active_runtime::restore_backup();
        let _ = monadeck_core::openvr_paths::restore_backup();
    }
    app.exit(0);
}

/// The window that stands for the app (tray, close button): the first-run
/// welcome while it's open, the deck otherwise.
fn home_label(app: &tauri::AppHandle) -> &'static str {
    if app.get_webview_window("welcome").is_some() {
        "welcome"
    } else {
        "main"
    }
}

fn show_home(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window(home_label(app)) {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

fn toggle_deck(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window(home_label(app)) {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            show_home(app);
        }
    }
}

/// A first run opens the welcome instead of the deck; it hands over through
/// `finish_welcome`. The deck starts hidden (tauri.conf.json) either way.
fn open_first_window(app: &tauri::App) {
    let seen = app
        .try_state::<AppState>()
        .is_none_or(|s| s.config.lock().unwrap().setup_seen);
    if !seen {
        let welcome = WebviewWindowBuilder::new(app, "welcome", WebviewUrl::App("welcome".into()))
            .title("Welcome to Monadeck")
            .inner_size(720.0, 780.0)
            .min_inner_size(600.0, 560.0)
            .decorations(false)
            .center()
            .build();
        match welcome {
            Ok(_) => return,
            Err(e) => log::warn!("welcome window: {e}"),
        }
    }
    if let Some(deck) = app.get_webview_window("main") {
        let _ = deck.show();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // WebKitGTK's DMA-BUF renderer leaves the window blank on NVIDIA's own
    // driver: use its fallback there. Before any GTK/WebKit setup; a value
    // already in the environment (e.g. 0 to opt out) wins.
    const NO_DMABUF: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    if std::env::var_os(NO_DMABUF).is_none() && std::path::Path::new("/proc/driver/nvidia/version").exists() {
        std::env::set_var(NO_DMABUF, "1");
        log::info!("NVIDIA driver: {NO_DMABUF}=1");
    }

    tauri::Builder::default()
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
                    "show" => show_home(app),
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
            open_first_window(app);
            Ok(())
        })
        // Closing the deck (or the welcome, before it) either hides it to the
        // tray (default) or quits. The settings window only hides on close (it
        // would otherwise keep the process alive); the deck is what governs quitting.
        .on_window_event(|window, event| {
            if window.label() == home_label(window.app_handle()) {
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
            commands::finish_welcome,
            commands::autodetect_prefix,
            commands::autodetect_xrizer,
            commands::autodetect_wivrn,
            commands::wivrn_enable_pairing,
            commands::wivrn_disable_pairing,
            commands::wivrn_disconnect,
            commands::wivrn_revoke_key,
            commands::wivrn_rename_key,
            commands::wivrn_get_config,
            commands::wivrn_set_config,
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
            lighthouse::floor_cal_status,
            lighthouse::run_room_setup,
            lighthouse::head_height,
            lighthouse::pairing_receivers,
            vr_audio::audio_devices,
            lighthouse::pairing_start,
            lighthouse::bs_scan,
            lighthouse::bs_state,
            lighthouse::bs_set_power,
            lighthouse::bs_set_channel,
            lighthouse::bs_identify,
            lighthouse::install_udev_rules,
            lighthouse::steamvr_installed,
            commands::run_floor_calibration,
            commands::survive_cal_status,
            commands::run_survive_calibration,
            commands::install_builtin_monado,
            commands::install_builtin_xrizer,
            commands::runtime_updates,
            commands::open_url,
            commands::uevr_status,
            commands::install_chihuahua,
            commands::list_installed_apps,
            commands::launch_plugin,
            bindings::bind_games,
            bindings::bind_own_personal,
            bindings::bind_live_reload,
            bindings::bind_controllers,
            bindings::bind_modes,
            bindings::bind_open,
            bindings::bind_view,
            bindings::bind_edit,
            bindings::bind_save,
            bindings::bind_reset,
            bindings::bind_usual_controller,
            bindings::game_cover,
            bindings::set_custom_cover,
            bindings::remove_custom_cover,
            bindings::get_custom_paths,
            bindings::set_custom_paths,
            gamepad::gamepad_profiles_list,
            gamepad::gamepad_profiles_dir,
            gamepad::gamepad_profile_save,
            gamepad::gamepad_profile_delete,
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
