import { invoke } from "@tauri-apps/api/core";
import type {
  CapStatus,
  FloorCalStatus,
  LogChunk,
  MonadeckConfig,
  PreflightReport,
  RuntimeStatus,
  ServiceStatus,
  Snapshot,
  SurviveCalStatus,
} from "./types";

export const appVersion = () => invoke<string>("app_version");

export const getConfig = () => invoke<MonadeckConfig>("get_config");
export const setConfig = (config: MonadeckConfig) =>
  invoke<void>("set_config", { config });
export const autodetectPrefix = () =>
  invoke<string | null>("autodetect_prefix");
export const autodetectXrizer = () =>
  invoke<string | null>("autodetect_xrizer");

export const serviceStatus = () => invoke<ServiceStatus>("service_status");
export const runtimeStatus = () => invoke<RuntimeStatus>("runtime_status");

export const capabilitiesStatus = () =>
  invoke<CapStatus>("capabilities_status");
export const applyCapabilities = () => invoke<void>("apply_capabilities");

export const startService = () => invoke<void>("start_service");
export const stopService = () => invoke<void>("stop_service");

export const getSnapshot = () => invoke<Snapshot>("get_snapshot");

import type { AmdGpu } from "./types";
export const amdGpu = () => invoke<AmdGpu | null>("amd_gpu");
export const hasNvidia = () => invoke<boolean>("has_nvidia");
export const setAmdVrProfile = () => invoke<void>("set_amd_vr_profile");
export const importOpenxrStatus = () => invoke<boolean>("import_openxr_status");
export const writeImportOpenxr = () => invoke<void>("write_import_openxr");
export const preflightCheck = () => invoke<PreflightReport>("preflight_check");

export const floorCalStatus = () => invoke<FloorCalStatus>("floor_cal_status");
export const runFloorCalibration = () =>
  invoke<void>("run_floor_calibration");

export const surviveCalStatus = () =>
  invoke<SurviveCalStatus>("survive_cal_status");
export const runSurviveCalibration = () =>
  invoke<void>("run_survive_calibration");

import type { Installed, UevrStatus } from "./types";
export const installBuiltinMonado = () =>
  invoke<Installed>("install_builtin_monado");
export const installBuiltinXrizer = () =>
  invoke<Installed>("install_builtin_xrizer");

export const uevrStatus = () => invoke<UevrStatus>("uevr_status");
export const installChihuahua = (force: boolean) =>
  invoke<string>("install_chihuahua", { force });
export const getLogs = (since: number) =>
  invoke<LogChunk>("get_logs", { since });

import type { InstalledApp } from "./types";
export const listInstalledApps = () =>
  invoke<InstalledApp[]>("list_installed_apps");

export const launchPlugin = (index: number) =>
  invoke<number>("launch_plugin", { index });

import type { EyeStatus } from "./types";
export const beyondPresent = () => invoke<boolean>("beyond_present");
export const eyetrackingStatus = () => invoke<EyeStatus>("eyetracking_status");
export const eyetrackingStart = () => invoke<void>("eyetracking_start");
export const eyetrackingStop = () => invoke<void>("eyetracking_stop");
export const installBsbcams = () => invoke<Installed>("install_bsbcams_cmd");
export const installBsbcamsRule = () => invoke<void>("install_bsbcams_rule");
export const setBsbcamsPath = (path: string) =>
  invoke<void>("set_bsbcams_path", { path });
