// Words for the editor, as the in-headset one says them.
import type { HandId, Manifest, ModeDef } from "./api";

/** `Stick_Click` → "Stick click", `SkeletonLeftHand` → "Skeleton left hand"
 *  (mirrors core's `pretty_name`). */
export function prettyName(path: string): string {
  const last = path.split("/").pop() ?? path;
  const spaced = last.replace(/[_-]/g, " ").replace(/([a-z0-9])([A-Z])/g, "$1 $2");
  const words = spaced.split(/\s+/).filter(Boolean);
  if (!words.length) return last;
  return words
    .map((w, i) => {
      if (w.length > 1 && w === w.toUpperCase() && /[A-Z]/.test(w)) return w;
      const lower = w.toLowerCase();
      return i === 0 ? lower.charAt(0).toUpperCase() + lower.slice(1) : lower;
    })
    .join(" ");
}

/** The game's name for an action, else one made from its path. */
export function actionName(manifest: Manifest, key: string): string {
  const k = key.toLowerCase();
  return manifest.actions.find((a) => a.key === k)?.name ?? prettyName(key);
}

/** "Left trigger", "Left A button". */
export function inputLabel(hand: HandId, label: string): string {
  const first = label.split(" ")[0] ?? "";
  const l = first.length > 1 ? label.charAt(0).toLowerCase() + label.slice(1) : label;
  return `${hand === "left" ? "Left" : "Right"} ${l}`;
}

export function modeTitle(mode: string, modes: ModeDef[]): string {
  const m = modes.find((x) => x.id === mode);
  if (m?.id === "none") return "Does nothing";
  return `Use as ${(m?.label ?? mode.replace(/_/g, " ")).toLowerCase()}`;
}

/** What a slot needs, in words. */
export function kindWords(kind: string): string {
  switch (kind) {
    case "boolean":
      return "an on / off action";
    case "vector1":
      return "an action that takes an amount (0 to 1)";
    case "vector2":
      return "an action that takes a direction";
    default:
      return "a matching action";
  }
}

export const MODE_ICONS: Record<string, string> = {
  button: "radio-button",
  toggle_button: "toggle-right",
  trigger: "arrow-line-down",
  joystick: "joystick",
  trackpad: "circle-dashed",
  dpad: "arrows-out-cardinal",
  scroll: "mouse-scroll",
  force_sensor: "gauge",
  grab: "hand-grabbing",
  scalar_constant: "hash",
  none: "prohibit",
};

export const KIND_ICONS: Record<string, string> = {
  boolean: "cursor-click",
  vector1: "sliders-horizontal",
  vector2: "joystick",
  vibration: "vibrate",
  pose: "hand",
};

export const POSE_POINTS: [string, string][] = [
  ["raw", "Raw"],
  ["tip", "Tip"],
  ["gdc2015", "GDC 2015"],
];

/** The drawings (static/bindings), each of a RIGHT controller, and their
 *  aspect ratios; Touch's has no letters (it's mirrored for the left). */
export const ART: Record<string, { url: string; aspect: number }> = {
  index: { url: "/bindings/indexcontroller_right.svg", aspect: 376 / 545.1 },
  touch: { url: "/bindings/oculus_touch_right_blank.svg", aspect: 571 / 581 },
  vive: { url: "/bindings/vive_wand.svg", aspect: 400 / 792 },
};
