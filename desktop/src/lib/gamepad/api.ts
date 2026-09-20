// Tauri command bridge for the gamepad remap editor.
import { invoke } from "@tauri-apps/api/core";
import type { Profile, ProfileFile } from "./types";

export const listProfiles = () => invoke<ProfileFile[]>("gamepad_profiles_list");
export const profilesDir = () => invoke<string>("gamepad_profiles_dir");
/// Empty `file` creates a new file named after the profile; returns the file used.
export const saveProfile = (file: string, profile: Profile) =>
  invoke<string>("gamepad_profile_save", { file, profile });
export const deleteProfile = (file: string) => invoke<void>("gamepad_profile_delete", { file });
