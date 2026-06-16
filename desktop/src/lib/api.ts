import { invoke } from "@tauri-apps/api/core";
import type {
  CapStatus,
  LogChunk,
  MonadeckConfig,
  RuntimeStatus,
  ServiceStatus,
  Snapshot,
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
export const getLogs = (since: number) =>
  invoke<LogChunk>("get_logs", { since });

import type { InstalledApp } from "./types";
export const listInstalledApps = () =>
  invoke<InstalledApp[]>("list_installed_apps");

export const launchPlugin = (index: number) =>
  invoke<number>("launch_plugin", { index });
