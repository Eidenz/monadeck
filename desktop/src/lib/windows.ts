// Cross-window helpers. The settings window is defined in tauri.conf.json
// (created hidden at startup), so its event listener is always live — we just
// show/focus it and tell it which section to open.
import { Window } from "@tauri-apps/api/window";
import { emit } from "@tauri-apps/api/event";

export type SettingsSection =
  | "general"
  | "environment"
  | "plugins"
  | "logs"
  | "about";

export async function openSettings(section: SettingsSection = "general") {
  const win = await Window.getByLabel("settings");
  if (win) {
    await win.show();
    await win.unminimize();
    await win.setFocus();
  }
  await emit("monadeck:section", section);
}
