// Mirrors monadeck-core's launch_options::steam_launch_options. Kept client-side
// so the string updates live as Settings change (no IPC round-trip / race).
import type { MonadeckConfig } from "./types";

/// The virtual pad's USB id (mirrors core::vr_games::PAD_DEVICE).
const PAD_DEVICE = "0x045e/0x028e";

function quote(v: string): string {
  return /\s/.test(v) ? `"${v}"` : v;
}

export function steamLaunchOptions(cfg: MonadeckConfig | null): string {
  if (!cfg) return "";
  // Client-side only: env vars in config.environment go to monado-service, NOT
  // the game (Envision puts only the pressure-vessel flag here too).
  const parts: string[] = [];
  if (cfg.ovr_runtime === "xrizer" && cfg.xrizer_path) {
    parts.push(`VR_OVERRIDE=${quote(cfg.xrizer_path)}`);
  }
  parts.push("PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES=1");
  // Keep gaming mode's virtual Xbox pad out of VR games (they would read it as
  // their own gamepad). GE-style Protons get this from a local fix the overlay
  // writes; Valve's Proton only honours launch options.
  parts.push(`SDL_GAMECONTROLLER_IGNORE_DEVICES=${PAD_DEVICE}`);
  parts.push(`SDL_JOYSTICK_BLACKLIST_DEVICES=${PAD_DEVICE}`);
  parts.push("%command%");
  return parts.join(" ");
}
