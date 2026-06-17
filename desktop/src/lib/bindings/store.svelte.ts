// Binding-editor state (its own window): scan games, pick a binding file, then
// edit it either visually (controller diagram + panels) or as raw JSON.
//
// `bindingJson` is the raw text (Raw tab); `bindingConfig` is the parsed object
// the visual editor mutates. They're kept in sync: a visual edit re-serializes
// to `bindingJson`; a raw edit re-parses into `bindingConfig`.
import * as api from "./api";
import type { BindingFile, DetectedGame } from "./api";
import type {
  ActionManifest,
  BindingConfig,
  HapticEntry,
  SourceEntry,
} from "./types";
import { getProfile } from "./data/profileRegistry";
import { getMirrorPath, type ControllerProfile } from "./data/controllers";
import { getActionName } from "./types";

export const editor = $state({
  // game browser
  games: [] as DetectedGame[],
  scanning: false,
  scanned: false,
  game: null as DetectedGame | null,
  binding: null as BindingFile | null,
  // raw + parsed
  actionsJson: "",
  bindingJson: "",
  actionManifest: null as ActionManifest | null,
  bindingConfig: null as BindingConfig | null,
  // editor selection
  activeActionSet: null as string | null,
  selectedInput: null as string | null,
  hoveredInput: null as string | null,
  mirrorMode: false,
  // user-added scan folders
  customPaths: [] as string[],
  // preferred controller, auto-selected when opening a game (persisted)
  defaultController: "knuckles",
  // status
  dirty: false,
  saving: false,
  error: "",
});

const DEFAULT_CTRL_KEY = "monadeck.bindings.defaultController";

export function loadDefaultController() {
  try {
    editor.defaultController = localStorage.getItem(DEFAULT_CTRL_KEY) ?? "knuckles";
  } catch {
    /* no localStorage during prerender — keep the default */
  }
}

export function setDefaultController(ctrl: string) {
  editor.defaultController = ctrl;
  try {
    localStorage.setItem(DEFAULT_CTRL_KEY, ctrl);
  } catch {
    /* ignore */
  }
}

export async function loadCustomPaths() {
  try {
    editor.customPaths = await api.getCustomPaths();
  } catch {
    editor.customPaths = [];
  }
}

export async function addCustomPath(path: string) {
  if (editor.customPaths.includes(path)) return;
  editor.customPaths = [...editor.customPaths, path];
  await api.setCustomPaths([...editor.customPaths]);
  await scan();
}

export async function removeCustomPath(path: string) {
  editor.customPaths = editor.customPaths.filter((p) => p !== path);
  await api.setCustomPaths([...editor.customPaths]);
  await scan();
}

// --- game browser -------------------------------------------------------------

export async function scan() {
  editor.scanning = true;
  editor.error = "";
  try {
    editor.games = await api.scanSteamGames();
  } catch (e) {
    editor.error = `Scan failed: ${e}`;
  } finally {
    editor.scanning = false;
    editor.scanned = true;
  }
}

export async function selectGame(game: DetectedGame) {
  editor.game = game;
  resetEditing();
  // Prefer the user's default controller, falling back to the first available.
  const bf =
    game.bindingFiles.find((b) => b.controllerType === editor.defaultController) ??
    game.bindingFiles[0];
  if (bf) await selectBinding(bf);
}

export async function selectBinding(binding: BindingFile) {
  if (!editor.game) return;
  editor.binding = binding;
  editor.error = "";
  try {
    const [actions, bind] = await api.loadGameBindings(
      editor.game.actionsPath,
      binding.filePath,
    );
    editor.actionsJson = actions;
    editor.bindingJson = bind;
    editor.actionManifest = tryParse<ActionManifest>(actions);
    editor.bindingConfig = tryParse<BindingConfig>(bind);
    editor.activeActionSet = editor.bindingConfig
      ? (Object.keys(editor.bindingConfig.bindings)[0] ?? null)
      : null;
    editor.selectedInput = null;
    editor.dirty = false;
  } catch (e) {
    editor.error = `Failed to load bindings: ${e}`;
  }
}

function resetEditing() {
  editor.binding = null;
  editor.actionsJson = "";
  editor.bindingJson = "";
  editor.actionManifest = null;
  editor.bindingConfig = null;
  editor.activeActionSet = null;
  editor.selectedInput = null;
  editor.dirty = false;
}

function tryParse<T>(text: string): T | null {
  try {
    return JSON.parse(text) as T;
  } catch {
    return null;
  }
}

/** Raw-tab edit: update the text and re-parse into the config when it's valid. */
export function editBindingText(text: string) {
  editor.bindingJson = text;
  editor.dirty = true;
  const parsed = tryParse<BindingConfig>(text);
  if (parsed) editor.bindingConfig = parsed;
}

function validJson(): boolean {
  try {
    JSON.parse(editor.bindingJson);
    return true;
  } catch (e) {
    editor.error = `Invalid JSON, not saved: ${e}`;
    return false;
  }
}

/** Save in place (overwrites the file currently open). */
export async function save() {
  if (!editor.binding || !validJson()) return;
  editor.saving = true;
  editor.error = "";
  try {
    await api.writeJsonFile(editor.binding.filePath, editor.bindingJson);
    editor.dirty = false;
  } catch (e) {
    editor.error = `Save failed: ${e}`;
  } finally {
    editor.saving = false;
  }
}

/** Whether the open binding is already a per-game xrizer override. */
export function isOverride(): boolean {
  return !!editor.game?.source.includes("xrizer (game override)");
}

/** Write to `<game>/xrizer/<ctrl>.json` so the game's default file (which Steam
 *  may overwrite on update) is left untouched — the proper xrizer workflow. xrizer
 *  loads its overrides as `<controller>.json` directly (e.g. `knuckles.json`), NOT
 *  the `bindings_<ctrl>.json` SteamVR-style name. */
export async function saveAsOverride() {
  if (!editor.game || !editor.binding || !validJson()) return;
  const ctrl = editor.binding.controllerType;
  const gamePath = editor.game.gamePath;
  const path = `${gamePath}/xrizer/${ctrl}.json`;
  editor.saving = true;
  editor.error = "";
  try {
    await api.writeJsonFile(path, editor.bindingJson);
    editor.dirty = false;
    await scan(); // the new override now appears (and hides the default entry)
    const overrideGame = editor.games.find(
      (g) => g.gamePath === gamePath && g.source.includes("xrizer (game override)"),
    );
    if (overrideGame) {
      editor.game = overrideGame;
      const bf =
        overrideGame.bindingFiles.find((b) => b.controllerType === ctrl) ??
        overrideGame.bindingFiles[0];
      if (bf) await selectBinding(bf);
    }
  } catch (e) {
    editor.error = `Save failed: ${e}`;
  } finally {
    editor.saving = false;
  }
}

// --- selection ---------------------------------------------------------------

export function setActiveActionSet(setPath: string) {
  editor.activeActionSet = setPath;
  editor.selectedInput = null;
}
export function setSelectedInput(path: string | null) {
  editor.selectedInput = path;
}
export function setHoveredInput(path: string | null) {
  editor.hoveredInput = path;
}
export function setMirrorMode(enabled: boolean) {
  editor.mirrorMode = enabled;
}

// --- source operations (mirror-aware), ported from xrbind editorStore --------

function mirrorPath(path: string, profile: ControllerProfile | null): string {
  if (profile) return getMirrorPath(profile, path) || path;
  if (path.includes("/hand/left/")) return path.replace("/hand/left/", "/hand/right/");
  if (path.includes("/hand/right/")) return path.replace("/hand/right/", "/hand/left/");
  return path;
}

export function handFromPath(path: string): "left" | "right" | null {
  if (path.includes("/hand/left/")) return "left";
  if (path.includes("/hand/right/")) return "right";
  return null;
}

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));

/** Re-serialize the config to the raw text and flag unsaved. */
function commitConfig() {
  if (editor.bindingConfig) {
    editor.bindingJson = JSON.stringify(editor.bindingConfig, null, 3);
  }
  editor.dirty = true;
}

function ensureSet(setPath: string) {
  const c = editor.bindingConfig!;
  if (!c.bindings[setPath]) c.bindings[setPath] = { sources: [], haptics: [] };
}

export function addSource(setPath: string, source: SourceEntry) {
  const c = editor.bindingConfig;
  if (!c) return;
  const profile = getProfile(c.controller_type);
  ensureSet(setPath);
  c.bindings[setPath].sources.push(source);
  if (editor.mirrorMode) {
    c.bindings[setPath].sources.push({
      ...clone(source),
      path: mirrorPath(source.path, profile),
    });
  }
  commitConfig();
}

export function updateSource(setPath: string, index: number, source: SourceEntry) {
  const c = editor.bindingConfig;
  if (!c) return;
  const profile = getProfile(c.controller_type);
  const sources = c.bindings[setPath]?.sources;
  if (!sources?.[index]) return;
  const oldPath = sources[index].path;
  sources[index] = source;
  if (editor.mirrorMode) {
    const mirroredOld = mirrorPath(oldPath, profile);
    const mi = sources.findIndex(
      (s, i) => i !== index && s.path === mirroredOld && s.mode === source.mode,
    );
    if (mi !== -1) {
      sources[mi] = { ...clone(source), path: mirrorPath(source.path, profile) };
    }
  }
  commitConfig();
}

export function removeSource(setPath: string, index: number) {
  const c = editor.bindingConfig;
  if (!c) return;
  const profile = getProfile(c.controller_type);
  const sources = c.bindings[setPath]?.sources;
  if (!sources) return;
  const removed = sources[index];
  sources.splice(index, 1);
  if (editor.mirrorMode && removed) {
    const mp = mirrorPath(removed.path, profile);
    const mi = sources.findIndex((s) => s.path === mp && s.mode === removed.mode);
    if (mi !== -1) sources.splice(mi, 1);
  }
  commitConfig();
}

export function addHaptic(setPath: string, haptic: HapticEntry) {
  const c = editor.bindingConfig;
  if (!c) return;
  const profile = getProfile(c.controller_type);
  ensureSet(setPath);
  if (!c.bindings[setPath].haptics) c.bindings[setPath].haptics = [];
  c.bindings[setPath].haptics!.push(haptic);
  if (editor.mirrorMode) {
    c.bindings[setPath].haptics!.push({
      ...haptic,
      path: mirrorPath(haptic.path, profile),
    });
  }
  commitConfig();
}

export function removeHaptic(setPath: string, index: number) {
  const c = editor.bindingConfig;
  if (!c) return;
  const profile = getProfile(c.controller_type);
  const haptics = c.bindings[setPath]?.haptics;
  if (!haptics) return;
  const removed = haptics[index];
  haptics.splice(index, 1);
  if (editor.mirrorMode && removed) {
    const mp = mirrorPath(removed.path, profile);
    const mi = haptics.findIndex((h) => h.path === mp && h.output === removed.output);
    if (mi !== -1) haptics.splice(mi, 1);
  }
  commitConfig();
}

// --- selectors (call from $derived / template; they read editor.* reactively) -

export function activeProfile(): ControllerProfile {
  return getProfile(editor.bindingConfig?.controller_type ?? "knuckles");
}

export function actionSets(): string[] {
  return editor.bindingConfig ? Object.keys(editor.bindingConfig.bindings) : [];
}

export function activeBindingSources(): SourceEntry[] {
  if (!editor.bindingConfig || !editor.activeActionSet) return [];
  return editor.bindingConfig.bindings[editor.activeActionSet]?.sources ?? [];
}

/** Input (non-vibration/pose/skeleton) action paths, scoped to the active set if any. */
export function inputActions(): string[] {
  const m = editor.actionManifest;
  if (!m) return [];
  const usable = m.actions.filter(
    (a) => a.type !== "vibration" && a.type !== "pose" && a.type !== "skeleton",
  );
  if (editor.activeActionSet) {
    const prefix = editor.activeActionSet + "/";
    const scoped = usable.filter((a) => a.name.startsWith(prefix));
    if (scoped.length > 0) return scoped.map((a) => a.name);
  }
  return usable.map((a) => a.name);
}

/** Sources bound to a given physical input path, with their index in the set. */
export function sourcesForInput(
  inputPath: string,
): { source: SourceEntry; globalIndex: number }[] {
  return activeBindingSources()
    .map((source, globalIndex) => ({ source, globalIndex }))
    .filter(({ source }) => source.path === inputPath);
}

/** Output (vibration) action paths, scoped to the active set if any. */
export function outputActions(): string[] {
  const m = editor.actionManifest;
  if (!m) return [];
  const vib = m.actions.filter((a) => a.type === "vibration");
  if (editor.activeActionSet) {
    const prefix = editor.activeActionSet + "/";
    const scoped = vib.filter((a) => a.name.startsWith(prefix));
    if (scoped.length > 0) return scoped.map((a) => a.name);
  }
  return vib.map((a) => a.name);
}

export function activeHaptics(): HapticEntry[] {
  if (!editor.bindingConfig || !editor.activeActionSet) return [];
  return editor.bindingConfig.bindings[editor.activeActionSet]?.haptics ?? [];
}

/** Friendly name for an action path (uses the manifest localization if present). */
export function localizeAction(path: string): string {
  const loc = editor.actionManifest?.localization;
  if (loc) {
    for (const entry of loc) if (entry[path]) return entry[path];
  }
  return getActionName(path);
}
