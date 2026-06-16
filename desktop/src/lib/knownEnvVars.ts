// Curated monado env vars for the "add known var" picker. Descriptions adapted
// from Envision's reference list + the fork's README. The compositor vars
// (XRT_COMPOSITOR_*, U_PACING_APP_USE_MIN_FRAME_PERIOD, XRT_DEBUG/CURATED_GUI)
// are intentionally excluded — they have dedicated controls in Compositor.
export interface KnownVar {
  name: string;
  value: string; // a sensible default to pre-fill
  desc: string;
}

export const KNOWN_ENV_VARS: KnownVar[] = [
  {
    name: "LH_HANDTRACKING",
    value: "1",
    desc: "Optical hand tracking: 0 off, 1 auto (only with no controllers), 2 on (even with controllers).",
  },
  {
    name: "LH_LOG",
    value: "info",
    desc: "Lighthouse log level: trace, debug, info, warn, error.",
  },
  {
    name: "XRT_JSON_LOG",
    value: "1",
    desc: "Set to 1 for JSON logging — better log visualization and level filtering.",
  },
  {
    name: "QWERTY_ENABLE",
    value: "1",
    desc: "Enable the QWERTY simulated driver — use monado without an HMD/controllers (mixable with other drivers).",
  },
  {
    name: "MONADO_SCREENSHOT_DIR",
    value: "~/Pictures/Monado",
    desc: "Output directory for the fork's in-headset screenshots.",
  },
  {
    name: "MONADO_SCREENSHOT_HEIGHT",
    value: "1080",
    desc: "Screenshot capture height in pixels (0 = full native render resolution).",
  },
  {
    name: "MONADO_SCREENSHOT_NO_SOUND",
    value: "1",
    desc: "Set to mute the shutter sound on capture.",
  },
  {
    name: "MONADO_SCREENSHOT_SOUND_CMD",
    value: "",
    desc: "Command used to play the screenshot shutter sound.",
  },
];
