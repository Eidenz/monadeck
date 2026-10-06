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
  base_stations: SavedStation[];
  base_stations_auto: boolean; // on when VR starts, off when it stops
  base_stations_off: StationPower; // what "off" means: sleep or standby
  controllers_off_on_stop: boolean; // Lighthouse controllers/trackers off when VR stops
  vr_audio_output: AudioDevice | null; // default output while VR runs (null: leave it)
  vr_audio_input: AudioDevice | null; // default microphone while VR runs (null: leave it)
  vr_audio_auto: boolean; // WiVRn: the headset's own output and microphone instead
  dismissed_updates: string[]; // "monado:<tag>" / "xrizer:<tag>" put off by the user
}

// A newer release of a runtime Monadeck installed itself.
export interface RuntimeUpdate {
  installed: string;
  latest: string;
}

export interface RuntimeUpdates {
  monado: RuntimeUpdate | null;
  xrizer: RuntimeUpdate | null;
}

// --- Lighthouse: base stations, receivers, room setup ------------------------

export type StationVersion = "v1" | "v2";
export type StationPower = "on" | "sleep" | "standby";

export interface SavedStation {
  address: string;
  name: string;
  version: StationVersion;
  bsid: string | null; // 1.0 only: the ID printed on its back
}

export interface FoundStation {
  address: string;
  name: string;
  version: StationVersion;
  rssi: number | null; // set when it answered the latest scan
}

export interface StationState {
  power: "on" | "sleep" | "standby" | "waking" | null;
  channel: number | null;
}

export interface Receiver {
  serial: string;
  name: string;
  node: string;
  active: boolean | null; // a device is connected through it (known with VR running)
}

export type ReceiverKind = "headset_left" | "headset_right" | "multi_dongle" | "vive_dongle" | "other";

// One physical receiver: a dongle, or one of a headset's own. A Tundra dongle
// holds several receivers, each pairing one device.
export interface ReceiverGroup {
  label: string; // "Headset left controller", "Tundra SW3", "Vive dongle", or its own name
  kind: ReceiverKind;
  receivers: Receiver[];
}

export interface RoomResult {
  universe: string;
  previous_height: number | null; // m above the previous floor
  moved: number | null; // m the centre moved
  applied: boolean; // the running Monado picked it up
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
  /** Powered on / linked; null when the runtime doesn't say (treat as on). */
  connected?: boolean | null;
  /** Pose fully tracked right now; raw, debounced by the strip. */
  tracking?: boolean | null;
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
  // What the kwin freeze watch did (see core::kwin_freeze), until the next
  // manual start. The same for every window and every poll.
  freeze_recovery: FreezeRecovery | null;
  // The freeze watch is stopping + restarting the service right now.
  recovering: boolean;
  // The last stop was ours (Stop button / freeze recovery), not a crash.
  deliberate_stop: boolean;
}

export interface FreezeRecovery {
  // Bumped per freeze event, so a dismissal can be remembered.
  seq: number;
  // The automatic restart: none | pending | ok | failed.
  restart: "none" | "pending" | "ok" | "failed";
  restart_error: string | null;
  // Outputs the watch told kwin to drop (the wrongly adopted HMD connector).
  // Empty means the freeze was detected but the output couldn't be identified.
  disabled_outputs: string[];
  // The spam kept going a second after the output step (the second variant of
  // the freeze, where nothing can be disabled): the watch stopped the service
  // to release the headset, and is starting it once more if `restarting`.
  service_stopped: boolean;
  restarting: boolean;
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
  action: "install_udev_rules" | null; // a fix Monadeck applies itself
}

export interface PreflightReport {
  checks: PreflightCheck[];
  all_ok: boolean;
  distro: string | null;
}

export interface FloorCalStatus {
  available: boolean; // SteamVR's vrcmd was found (the fallback calibration can run)
  calibrated: boolean; // a room setup exists for the universe the driver knows
  native: boolean; // the runtime reports the headset's pose: room setup without SteamVR
  has_universe: boolean; // the lighthouse driver has found base stations
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

// --- Audio: the defaults while VR runs ---------------------------------------

export interface AudioDevice {
  name: string; // node name (alsa_output.usb-…), stable across reboots
  description: string; // what the desktop calls it
}

export interface AudioDevices {
  available: boolean; // pactl answered
  outputs: AudioDevice[];
  inputs: AudioDevice[];
  default_output: string | null;
  default_input: string | null;
}

// One entry of the in-page dropdown (components/Select.svelte).
export interface SelectOption {
  value: string;
  label: string;
}
