//! Tauri commands for the gamepad remap editor — thin wrappers over
//! `core::gamepad_profiles` (the overlay reads the same files).

use monadeck_core::gamepad_profiles::{self, Profile, ProfileFile};

type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub fn gamepad_profiles_list() -> Vec<ProfileFile> {
    gamepad_profiles::list_files()
}

#[tauri::command]
pub fn gamepad_profiles_dir() -> String {
    gamepad_profiles::dir().to_string_lossy().to_string()
}

/// Save a profile; an empty `file` creates one named after the profile.
/// Returns the file name used.
#[tauri::command]
pub fn gamepad_profile_save(file: String, profile: Profile) -> CmdResult<String> {
    gamepad_profiles::save(&file, &profile).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn gamepad_profile_delete(file: String) -> CmdResult<()> {
    gamepad_profiles::delete(&file).map_err(|e| e.to_string())
}
