// Tauri command bridge for the binding editor. Arg keys are camelCase — Tauri
// maps them to the snake_case Rust params.
import { invoke } from "@tauri-apps/api/core";

export interface BindingFile {
  controllerType: string;
  filePath: string;
  fileName: string;
}

export interface DetectedGame {
  name: string;
  appId: string | null;
  gamePath: string;
  actionsPath: string;
  bindingFiles: BindingFile[];
  source: string;
}

export const scanSteamGames = () => invoke<DetectedGame[]>("scan_steam_games");

export const loadGameBindings = (actionsPath: string, bindingPath: string) =>
  invoke<[string, string]>("load_game_bindings", { actionsPath, bindingPath });

export const readJsonFile = (path: string) => invoke<string>("read_json_file", { path });

export const writeJsonFile = (path: string, content: string) =>
  invoke<void>("write_json_file", { path, content });

export const gameCover = (appId: string, gameKey?: string | null) =>
  invoke<string>("game_cover", { appId, gameKey: gameKey ?? null });

export const getCustomPaths = () => invoke<string[]>("get_custom_paths");
export const setCustomPaths = (paths: string[]) =>
  invoke<void>("set_custom_paths", { paths });
