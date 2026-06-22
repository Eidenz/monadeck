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
  render_scale: number;
  min_frame_period: boolean;
  compute_compositor: boolean;
  debug_gui: boolean;
  nvidia_mitigation: boolean;
  lighthouse_driver: string;
  simulated_hmd: boolean;
  overlay_enabled: boolean;
  environment: Record<string, string>;
  plugins: Plugin[];
}

export interface AmdGpu {
  card: string;
  profile_path: string;
  current_mode: string;
  vr_active: boolean;
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

export type PreflightSeverity = "important" | "optional";

export interface PreflightCheck {
  id: string;
  label: string;
  ok: boolean;
  severity: PreflightSeverity;
  detail: string;
  fix: string | null; // install hint, present only when !ok
}

export interface PreflightReport {
  checks: PreflightCheck[];
  all_ok: boolean;
  distro: string | null;
}

export interface FloorCalStatus {
  available: boolean; // SteamVR's vrcmd tool was found (calibration is possible)
  calibrated: boolean; // a chaperone_info.vrchap exists (room setup has been run)
}

export interface Installed {
  tag: string; // release tag installed, e.g. "v25.1.0-eidenz1"
  path: string; // monado prefix, or xrizer runtime dir
}

export interface EyeStatus {
  present: boolean; // a Bigscreen Beyond is connected (USB 35bd)
  running: boolean; // go-bsb-cams is running
  rule_installed: boolean; // camera-access udev rule is in place
  binary: string | null; // resolved go-bsb-cams path, or null if not found
  port: number; // MJPEG stream port
}

export interface UevrStatus {
  protontricks: boolean; // protontricks-launch on PATH (needed for VR-Mod launches)
  chihuahua: string | null; // resolved path to the injector, or null if not installed
}
