// evdev KEY_* codes (linux/input-event-codes.h) — names for the picker and
// a browser `KeyboardEvent.code` → evdev map for "press a key" capture.

export const KEY_NAMES: Record<number, string> = {
  1: "Esc", 2: "1", 3: "2", 4: "3", 5: "4", 6: "5", 7: "6", 8: "7", 9: "8", 10: "9", 11: "0",
  12: "-", 13: "=", 14: "Backspace", 15: "Tab",
  16: "Q", 17: "W", 18: "E", 19: "R", 20: "T", 21: "Y", 22: "U", 23: "I", 24: "O", 25: "P", 26: "[", 27: "]", 28: "Enter",
  29: "Left Ctrl", 30: "A", 31: "S", 32: "D", 33: "F", 34: "G", 35: "H", 36: "J", 37: "K", 38: "L", 39: ";", 40: "'", 41: "`",
  42: "Left Shift", 43: "\\", 44: "Z", 45: "X", 46: "C", 47: "V", 48: "B", 49: "N", 50: "M", 51: ",", 52: ".", 53: "/",
  54: "Right Shift", 55: "Numpad *", 56: "Left Alt", 57: "Space", 58: "Caps Lock",
  59: "F1", 60: "F2", 61: "F3", 62: "F4", 63: "F5", 64: "F6", 65: "F7", 66: "F8", 67: "F9", 68: "F10",
  69: "Num Lock", 70: "Scroll Lock",
  71: "Numpad 7", 72: "Numpad 8", 73: "Numpad 9", 74: "Numpad -", 75: "Numpad 4", 76: "Numpad 5", 77: "Numpad 6", 78: "Numpad +",
  79: "Numpad 1", 80: "Numpad 2", 81: "Numpad 3", 82: "Numpad 0", 83: "Numpad .",
  86: "< (ISO)", 87: "F11", 88: "F12", 96: "Numpad Enter", 97: "Right Ctrl", 98: "Numpad /", 99: "Print Screen", 100: "Right Alt",
  102: "Home", 103: "↑", 104: "Page Up", 105: "←", 106: "→", 107: "End", 108: "↓", 109: "Page Down", 110: "Insert", 111: "Delete",
  113: "Mute", 114: "Volume −", 115: "Volume +", 119: "Pause", 125: "Left Super", 126: "Right Super", 127: "Menu",
  164: "Play/Pause", 163: "Next track", 165: "Previous track",
};

export function keyName(code: number): string {
  return KEY_NAMES[code] ?? `key ${code}`;
}

/// Picker options: letters, digits and the usual suspects first, then the rest.
export const KEY_OPTIONS: { code: number; label: string }[] = Object.entries(KEY_NAMES)
  .map(([c, label]) => ({ code: Number(c), label }))
  .sort((a, b) => a.code - b.code);

const LETTERS: Record<string, number> = {
  A: 30, B: 48, C: 46, D: 32, E: 18, F: 33, G: 34, H: 35, I: 23, J: 36, K: 37, L: 38, M: 50,
  N: 49, O: 24, P: 25, Q: 16, R: 19, S: 31, T: 20, U: 22, V: 47, W: 17, X: 45, Y: 21, Z: 44,
};

export const BROWSER_CODE_TO_EVDEV: Record<string, number> = {
  ...Object.fromEntries(Object.entries(LETTERS).map(([l, c]) => [`Key${l}`, c])),
  Digit1: 2, Digit2: 3, Digit3: 4, Digit4: 5, Digit5: 6, Digit6: 7, Digit7: 8, Digit8: 9, Digit9: 10, Digit0: 11,
  Minus: 12, Equal: 13, Backspace: 14, Tab: 15, BracketLeft: 26, BracketRight: 27, Enter: 28, ControlLeft: 29,
  Semicolon: 39, Quote: 40, Backquote: 41, ShiftLeft: 42, Backslash: 43, Comma: 51, Period: 52, Slash: 53,
  ShiftRight: 54, NumpadMultiply: 55, AltLeft: 56, Space: 57, CapsLock: 58,
  F1: 59, F2: 60, F3: 61, F4: 62, F5: 63, F6: 64, F7: 65, F8: 66, F9: 67, F10: 68, NumLock: 69, ScrollLock: 70,
  Numpad7: 71, Numpad8: 72, Numpad9: 73, NumpadSubtract: 74, Numpad4: 75, Numpad5: 76, Numpad6: 77, NumpadAdd: 78,
  Numpad1: 79, Numpad2: 80, Numpad3: 81, Numpad0: 82, NumpadDecimal: 83, IntlBackslash: 86, F11: 87, F12: 88,
  NumpadEnter: 96, ControlRight: 97, NumpadDivide: 98, PrintScreen: 99, AltRight: 100,
  Home: 102, ArrowUp: 103, PageUp: 104, ArrowLeft: 105, ArrowRight: 106, End: 107, ArrowDown: 108, PageDown: 109,
  Insert: 110, Delete: 111, Pause: 119, MetaLeft: 125, MetaRight: 126, ContextMenu: 127, Escape: 1,
};
