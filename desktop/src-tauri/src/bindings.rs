//! Tauri commands for the binding editor — thin wrappers over `core::steam`
//! (game discovery, cover art, custom scan paths) plus raw JSON read/write.

use monadeck_core::steam::{self, DetectedGame};
use std::path::Path;

type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub async fn scan_steam_games() -> CmdResult<Vec<DetectedGame>> {
    // Walks the Steam libraries / Proton prefixes — keep off the UI thread.
    tauri::async_runtime::spawn_blocking(steam::scan_games)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_game_bindings(actions_path: String, binding_path: String) -> CmdResult<(String, String)> {
    steam::load_game_bindings(&actions_path, &binding_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_json_file(path: String) -> CmdResult<String> {
    std::fs::read_to_string(&path).map_err(|e| format!("failed to read {path}: {e}"))
}

#[tauri::command]
pub fn write_json_file(path: String, content: String) -> CmdResult<()> {
    // Create parent dirs (e.g. a new xrizer/ override folder) if missing.
    if let Some(parent) = Path::new(&path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, &content).map_err(|e| format!("failed to write {path}: {e}"))
}

#[tauri::command]
pub async fn game_cover(app_id: String, game_key: Option<String>) -> CmdResult<String> {
    tauri::async_runtime::spawn_blocking(move || steam::game_cover(&app_id, game_key.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn set_custom_cover(game_key: String, image_path: String) -> CmdResult<()> {
    steam::set_custom_cover(&game_key, &image_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_custom_cover(game_key: String) {
    steam::remove_custom_cover(&game_key);
}

#[tauri::command]
pub fn get_custom_paths() -> Vec<String> {
    steam::get_custom_paths()
}

#[tauri::command]
pub fn set_custom_paths(paths: Vec<String>) -> CmdResult<()> {
    steam::set_custom_paths(&paths).map_err(|e| e.to_string())
}
