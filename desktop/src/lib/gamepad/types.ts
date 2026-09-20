// Gaming-mode remap profiles — the TypeScript twin of
// `crates/core/src/gamepad_profiles.rs` (same JSON on disk).

export type Hand = "left" | "right";

/// Analog inputs (0..1 or -1..1) drive axes / the mouse, and become a
/// button past the rule's threshold.
export const INPUTS = [
  { id: "trigger", label: "Trigger", analog: true, group: "Analog" },
  { id: "grip", label: "Grip (squeeze)", analog: true, group: "Analog" },
  { id: "stick_x", label: "Stick ◀▶", analog: true, group: "Analog" },
  { id: "stick_y", label: "Stick ▲▼", analog: true, group: "Analog" },
  { id: "pad_x", label: "Trackpad touch ◀▶", analog: true, group: "Analog" },
  { id: "pad_y", label: "Trackpad touch ▲▼", analog: true, group: "Analog" },
  { id: "a", label: "A", analog: false, group: "Buttons" },
  { id: "b", label: "B", analog: false, group: "Buttons" },
  { id: "stick_click", label: "Stick click", analog: false, group: "Buttons" },
  { id: "pad_press", label: "Trackpad press (anywhere)", analog: false, group: "Trackpad" },
  { id: "pad_up", label: "Trackpad press ▲", analog: false, group: "Trackpad" },
  { id: "pad_down", label: "Trackpad press ▼", analog: false, group: "Trackpad" },
  { id: "pad_left", label: "Trackpad press ◀", analog: false, group: "Trackpad" },
  { id: "pad_right", label: "Trackpad press ▶", analog: false, group: "Trackpad" },
  { id: "pad_touch", label: "Trackpad touch", analog: false, group: "Trackpad" },
  { id: "stick_up", label: "Stick ▲ (past threshold)", analog: false, group: "Stick as buttons" },
  { id: "stick_down", label: "Stick ▼ (past threshold)", analog: false, group: "Stick as buttons" },
  { id: "stick_left", label: "Stick ◀ (past threshold)", analog: false, group: "Stick as buttons" },
  { id: "stick_right", label: "Stick ▶ (past threshold)", analog: false, group: "Stick as buttons" },
] as const;

export type Input = (typeof INPUTS)[number]["id"];
export type InputInfo = (typeof INPUTS)[number];

export const INPUT_GROUPS = ["Analog", "Buttons", "Trackpad", "Stick as buttons"] as const;

export function inputInfo(id: Input): InputInfo {
  return INPUTS.find((i) => i.id === id) ?? INPUTS[0];
}

/// Inputs whose threshold means something: analog ones (button past it),
/// trackpad presses (force) and stick directions (deflection).
export function usesThreshold(id: Input): boolean {
  const info = inputInfo(id);
  return info.analog || info.group === "Trackpad" || info.group === "Stick as buttons";
}

export const BUTTONS = [
  "A", "B", "X", "Y", "LB", "RB", "Back", "Start", "Guide", "L3", "R3",
  "DpadUp", "DpadDown", "DpadLeft", "DpadRight",
] as const;
export type Btn = (typeof BUTTONS)[number];

export const BUTTON_LABELS: Record<Btn, string> = {
  A: "A", B: "B", X: "X", Y: "Y", LB: "LB", RB: "RB", Back: "Back", Start: "Start", Guide: "Guide",
  L3: "L3 (left stick click)", R3: "R3 (right stick click)",
  DpadUp: "D-pad ▲", DpadDown: "D-pad ▼", DpadLeft: "D-pad ◀", DpadRight: "D-pad ▶",
};

export const AXES = ["LX", "LY", "RX", "RY", "LT", "RT"] as const;
export type Axis = (typeof AXES)[number];

export const AXIS_LABELS: Record<Axis, string> = {
  LX: "Left stick ◀▶", LY: "Left stick ▲▼", RX: "Right stick ◀▶", RY: "Right stick ▲▼", LT: "Left trigger", RT: "Right trigger",
};

export const MOUSE_BUTTONS = [
  { code: 272, label: "Left click" },
  { code: 273, label: "Right click" },
  { code: 274, label: "Middle click" },
  { code: 275, label: "Side (back)" },
  { code: 276, label: "Extra (forward)" },
] as const;

export type Target =
  | { button: Btn }
  | { axis: Axis }
  | { key: number }
  | { mouse_button: number }
  | "mouse_x"
  | "mouse_y"
  | "wheel_x"
  | "wheel_y";

export type TargetKind = "button" | "axis" | "key" | "mouse_button" | "mouse_x" | "mouse_y" | "wheel_x" | "wheel_y";

export const TARGET_KINDS: { id: TargetKind; label: string; group: "Gamepad" | "Keyboard & mouse" }[] = [
  { id: "button", label: "Pad button", group: "Gamepad" },
  { id: "axis", label: "Pad axis", group: "Gamepad" },
  { id: "key", label: "Keyboard key", group: "Keyboard & mouse" },
  { id: "mouse_button", label: "Mouse button", group: "Keyboard & mouse" },
  { id: "mouse_x", label: "Mouse move ◀▶", group: "Keyboard & mouse" },
  { id: "mouse_y", label: "Mouse move ▲▼", group: "Keyboard & mouse" },
  { id: "wheel_y", label: "Scroll ▲▼", group: "Keyboard & mouse" },
  { id: "wheel_x", label: "Scroll ◀▶", group: "Keyboard & mouse" },
];

export function targetKind(t: Target): TargetKind {
  if (typeof t === "string") return t;
  if ("button" in t) return "button";
  if ("axis" in t) return "axis";
  if ("key" in t) return "key";
  return "mouse_button";
}

/// A sensible starting value when the kind changes.
export function defaultTarget(kind: TargetKind): Target {
  switch (kind) {
    case "button":
      return { button: "A" };
    case "axis":
      return { axis: "LX" };
    case "key":
      return { key: 57 };
    case "mouse_button":
      return { mouse_button: 272 };
    default:
      return kind;
  }
}

/// Motion / scroll targets: `speed` applies.
export function usesSpeed(t: Target): boolean {
  return typeof t === "string";
}

export interface Rule {
  hand: Hand;
  input: Input;
  target: Target;
  threshold: number;
  invert: boolean;
  speed: number;
}

export interface Profile {
  name: string;
  game: string | null;
  rules: Rule[];
}

export interface ProfileFile {
  /// File name inside the profiles folder ("" = the built-in stock layout).
  file: string;
  profile: Profile;
  stock: boolean;
}

export function newRule(hand: Hand): Rule {
  return { hand, input: "a", target: { button: "A" }, threshold: 0.5, invert: false, speed: 900 };
}
