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
export const autodetectWivrn = () =>
  invoke<string | null>("autodetect_wivrn");

// WiVRn backend (all over the server's D-Bus interface; fail when it's down).
export const wivrnEnablePairing = (timeoutSecs: number) =>
  invoke<string>("wivrn_enable_pairing", { timeoutSecs });
export const wivrnDisablePairing = () => invoke<void>("wivrn_disable_pairing");
export const wivrnDisconnect = () => invoke<void>("wivrn_disconnect");
export const wivrnRevokeKey = (publicKey: string) =>
  invoke<void>("wivrn_revoke_key", { publicKey });
export const wivrnRenameKey = (publicKey: string, name: string) =>
  invoke<void>("wivrn_rename_key", { publicKey, name });
export const wivrnGetConfig = () => invoke<string>("wivrn_get_config");
export const wivrnSetConfig = (json: string) =>
  invoke<void>("wivrn_set_config", { json });

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

import type { AudioDevices, FoundStation, ReceiverGroup, RoomResult, StationPower, StationState, StationVersion } from "./types";
export const runRoomSetup = () => invoke<RoomResult>("run_room_setup");
export const headHeight = () => invoke<number | null>("head_height");
export const pairingReceivers = () => invoke<ReceiverGroup[]>("pairing_receivers");
/** Returns how many seconds the receivers listen for devices. */
export const pairingStart = (serials: string[]) => invoke<number>("pairing_start", { serials });
export const bsScan = (secs: number) => invoke<FoundStation[]>("bs_scan", { secs });
export const bsState = (address: string) => invoke<StationState>("bs_state", { address });
export const bsSetPower = (address: string, version: StationVersion, power: StationPower, bsid: string | null) =>
  invoke<void>("bs_set_power", { address, version, power, bsid });
export const bsSetChannel = (address: string, channel: number) =>
  invoke<void>("bs_set_channel", { address, channel });
export const bsIdentify = (address: string) => invoke<void>("bs_identify", { address });
export const installUdevRules = () => invoke<void>("install_udev_rules");
export const steamvrInstalled = () => invoke<boolean>("steamvr_installed");
export const audioDevices = () => invoke<AudioDevices>("audio_devices");

export const surviveCalStatus = () =>
  invoke<SurviveCalStatus>("survive_cal_status");
export const runSurviveCalibration = () =>
  invoke<void>("run_survive_calibration");

import type { Installed, UevrStatus } from "./types";
export const installBuiltinMonado = () =>
  invoke<Installed>("install_builtin_monado");
/** A web link in the user's browser (the system's xdg-open, not the AppImage's). */
export const openUrl = (url: string) => invoke<void>("open_url", { url });
export const installBuiltinXrizer = () =>
  invoke<Installed>("install_builtin_xrizer");
import type { RuntimeUpdates } from "./types";
/** Newer built-in Monado/xrizer releases (empty offline; gives up within seconds). */
export const runtimeUpdates = () => invoke<RuntimeUpdates>("runtime_updates");

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
