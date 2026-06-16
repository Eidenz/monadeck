// Mirrors the serde shapes returned by the Rust commands in src-tauri.

export type OvrRuntime = "xrizer" | "none";
export type ExecWhen = "after-start" | "after-stop";

export interface Plugin {
  name: string;
  path: string;
  args: string[];
  when: ExecWhen;
  enabled: boolean;
}

export interface InstalledApp {
  name: string;
  path: string; // absolute path to the .desktop file
}

export interface MonadeckConfig {
  monado_prefix: string;
  xrizer_path: string | null;
  ovr_runtime: OvrRuntime;
  minimize_to_tray: boolean;
  auto_start: boolean;
  lighthouse_driver: string;
  environment: Record<string, string>;
  plugins: Plugin[];
}

export type DeviceKind =
  | "hmd"
  | "controller"
  | "glove"
  | "tracker"
  | "gamepad"
  | "basestation"
  | "unknown";

export interface Battery {
  charging: boolean;
  charge: number; // 0..1
}

export interface DeviceInfo {
  index: number;
  name_id: number;
  name: string;
  role: string | null;
  kind: DeviceKind;
  serial: string | null;
  battery: Battery | null;
}

export interface ClientInfo {
  name: string;
  focused: boolean;
  primary: boolean;
  overlay: boolean;
}

export interface Snapshot {
  devices: DeviceInfo[];
  clients: ClientInfo[];
}

export type CapStatus = "set" | "needs_setcap" | "no_binary" | "no_tooling";
export type ActiveRuntimeKind = "monado" | "steam_vr" | "other" | "none";
export type OvrPathsKind = "xrizer" | "steam_vr" | "other" | "none";

export interface ServiceStatus {
  running: boolean;
  connected: boolean;
  exit_code: number | null;
}

export interface RuntimeStatus {
  openxr: ActiveRuntimeKind;
  openvr: OvrPathsKind;
}

export interface LogChunk {
  cursor: number;
  lines: string[];
}
