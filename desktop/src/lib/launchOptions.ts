// Mirrors monadeck-core's launch_options::steam_launch_options. Kept client-side
// so the string updates live as Settings change (no IPC round-trip / race).
import type { MonadeckConfig } from "./types";

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
  parts.push("%command%");
  return parts.join(" ");
}
