// Mirrors the serde shapes returned by the Rust commands in src-tauri.

export type OvrRuntime = "xrizer" | "none";
export type Backend = "monado" | "wivrn";
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
  backend: Backend;
  wivrn_server_path: string | null;
  xrizer_path: string | null;
  ovr_runtime: OvrRuntime;
  minimize_to_tray: boolean;
  auto_start: boolean;
  kill_steamvr_on_start: boolean;
  setup_seen: boolean;
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

// "not_needed": the WiVRn backend — its server runs fine without CAP_SYS_NICE.
export type CapStatus =
  | "set"
  | "needs_setcap"
  | "no_binary"
  | "no_tooling"
  | "not_needed";
export type ActiveRuntimeKind = "monado" | "wivrn" | "steam_vr" | "other" | "none";
export type OvrPathsKind = "xrizer" | "steam_vr" | "other" | "none";

export interface KnownHeadset {
  name: string;
  public_key: string;
  last_connection: number; // seconds since epoch, 0 = never
}

// Snapshot of the WiVRn server's D-Bus properties (io.github.wivrn.Server).
export interface WivrnStatus {
  headset_connected: boolean;
  session_running: boolean;
  pairing_enabled: boolean;
  encryption_enabled: boolean;
  pin: string; // pairing PIN while pairing is enabled
  system_name: string; // headset model once connected
  steam_command: string; // launch-options prefix for Steam games
  known_keys: KnownHeadset[];
  preferred_refresh_rate: number;
  available_refresh_rates: number[];
  bitrate: number; // bits/s, 0 until connected
  supported_codecs: string[];
}

export interface ServiceStatus {
  backend: Backend;
  available: boolean; // the selected backend's service binary was found
  running: boolean;
  connected: boolean;
  // WiVRn: a server we didn't start owns the bus name (dashboard / systemd).
  external: boolean;
  wivrn: WivrnStatus | null;
  exit_code: number | null;
  // One-shot: set on the poll right after the kwin freeze watch recovered the
  // desktop from a cold-start HMD adoption (see core::kwin_freeze).
  freeze_recovery: FreezeRecovery | null;
}

export interface FreezeRecovery {
  // Outputs the watch told kwin to drop (the wrongly adopted HMD connector).
  // Empty means the freeze was detected but the output couldn't be identified.
  disabled_outputs: string[];
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

export interface SurviveCalStatus {
  available: boolean; // survive-cli was found (libsurvive calibration is possible)
  source_present: boolean; // a SteamVR lighthousedb.json exists to import from
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
