// Binding-editor state (its own window), the desktop twin of the in-headset
// editor: the games that have bindings (and Monadeck's own controls), then one
// binding being edited. The binding is JSON the page keeps; every edit goes
// through core (`bind_edit`), and what's drawn comes back from `bind_view`.
import * as api from "./api";
import type { Controller, Doc, EditOp, Game, HandId, Manifest, ModeDef, Target, View } from "./api";

export type Popup =
  | { kind: "addMode"; hand: HandId; input: string }
  | { kind: "changeMode"; index: number }
  | { kind: "action"; index: number; slot: string }
  | { kind: "controller" }
  | { kind: "haptics" }
  | { kind: "poses" }
  | { kind: "chords" }
  | { kind: "leave"; to: "list" | { controller: string } };

export interface Editing {
  target: Target;
  /** Monadeck's own controls (else a game's). */
  own: boolean;
  game: Game | null;
  name: string;
  ty: string;
  ctrl: Controller;
  manifest: Manifest;
  doc: Doc;
  /** The doc as last opened / saved (JSON), for "unsaved changes". */
  saved: string;
  personal: boolean;
  set: string;
  mirror: boolean;
  view: View | null;
  popup: Popup | null;
  busy: boolean;
  /** Editing the JSON by hand instead. */
  json: boolean;
  jsonText: string;
  jsonError: string;
}

export const bind = $state({
  games: [] as Game[],
  scanning: false,
  scanned: false,
  onlyCustomized: false,
  query: "",
  ownPersonal: [] as string[],
  customPaths: [] as string[],
  modes: [] as ModeDef[],
  gameControllers: [] as Controller[],
  ownControllers: [] as Controller[],
  usual: "knuckles",
  editing: null as Editing | null,
  /** The actions path of a game being opened ("own" for Monadeck's). */
  opening: null as string | null,
  error: "",
  notice: "",
});

let noticeTimer: ReturnType<typeof setTimeout> | undefined;
function notify(msg: string) {
  bind.notice = msg;
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => (bind.notice = ""), 2600);
}

export async function init() {
  try {
    [bind.modes, bind.gameControllers, bind.ownControllers, bind.customPaths, bind.usual] = await Promise.all([
      api.modes(),
      api.controllers(false),
      api.controllers(true),
      api.getCustomPaths(),
      api.usualController(),
    ]);
  } catch (e) {
    bind.error = `${e}`;
  }
}

export async function scan() {
  if (bind.scanning) return;
  bind.scanning = true;
  try {
    const [games, own] = await Promise.all([api.games(), api.ownPersonal()]);
    bind.games = games;
    bind.ownPersonal = own;
    // Keep an open game's editor on it (its list entry was replaced).
    const e = bind.editing;
    if (e?.game) e.game = games.find((g) => g.actionsPath === e.game!.actionsPath) ?? e.game;
  } catch (e) {
    bind.error = `Couldn't look for games: ${e}`;
  } finally {
    bind.scanning = false;
    bind.scanned = true;
  }
}

export async function addCustomPath(path: string) {
  if (bind.customPaths.includes(path)) return;
  bind.customPaths = [...bind.customPaths, path];
  await api.setCustomPaths([...bind.customPaths]);
  await scan();
}

export async function removeCustomPath(path: string) {
  bind.customPaths = bind.customPaths.filter((p) => p !== path);
  await api.setCustomPaths([...bind.customPaths]);
  await scan();
}

// --- opening -----------------------------------------------------------------------------------

const LAST = (own: boolean) => `monadeck.bindings.controller.${own ? "own" : "games"}`;

function remembered(own: boolean): string | null {
  try {
    return localStorage.getItem(LAST(own));
  } catch {
    return null;
  }
}

function remember(own: boolean, ty: string) {
  try {
    localStorage.setItem(LAST(own), ty);
  } catch {
    /* no storage: fine */
  }
}

/** The controller to open on: the last one picked, the usual one, the first. */
function pickController(own: boolean, available: string[]): string {
  return [remembered(own), bind.usual].find((t): t is string => !!t && available.includes(t)) ?? available[0] ?? "knuckles";
}

export function openGame(game: Game, ty?: string, set?: string) {
  return openTarget({ actionsPath: game.actionsPath, dirs: game.dirs }, false, game, game.name, ty ?? pickController(false, game.controllers), set);
}

export function openOwn(ty?: string, set?: string) {
  const all = bind.ownControllers.map((c) => c.ty);
  return openTarget({}, true, null, "Monadeck", ty ?? pickController(true, all), set);
}

async function openTarget(target: Target, own: boolean, game: Game | null, name: string, ty: string, set?: string) {
  const ctrl = (own ? bind.ownControllers : bind.gameControllers).find((c) => c.ty === ty);
  if (!ctrl) return;
  bind.opening = own ? "own" : (target.actionsPath ?? "");
  bind.error = "";
  try {
    const o = await api.open(target, ty);
    // Open on the first set that binds something, else the first shown one.
    const probe = await api.view(o.doc, "", own);
    const shown = o.manifest.sets.filter((s) => !s.hidden || (probe.counts[s.key] ?? 0) > 0);
    const first = set ?? (shown.find((s) => (probe.counts[s.key] ?? 0) > 0) ?? shown[0])?.key ?? "";
    const mirror = bind.editing?.mirror ?? false;
    bind.editing = {
      target,
      own,
      game,
      name,
      ty,
      ctrl,
      manifest: o.manifest,
      doc: o.doc,
      saved: JSON.stringify(o.doc),
      personal: o.personal,
      set: first,
      mirror,
      view: null,
      popup: null,
      busy: false,
      json: false,
      jsonText: "",
      jsonError: "",
    };
    remember(own, ty);
    await refresh();
  } catch (e) {
    bind.error = `Couldn't open it: ${e}`;
  } finally {
    bind.opening = null;
  }
}

export function close() {
  bind.editing = null;
  bind.error = "";
}

// --- editing -----------------------------------------------------------------------------------

export async function refresh() {
  const e = bind.editing;
  if (!e) return;
  try {
    e.view = await api.view(e.doc, e.set, e.own);
  } catch (err) {
    bind.error = `${err}`;
  }
}

/** Apply one edit. Returns the new source / chord index for adds. */
export async function apply(op: EditOp): Promise<number | null> {
  const e = bind.editing;
  if (!e) return null;
  try {
    const r = await api.edit(e.doc, e.ty, e.own, e.set, e.mirror, op);
    e.doc = r.doc;
    await refresh();
    return r.index;
  } catch (err) {
    bind.error = `${err}`;
    return null;
  }
}

export function setSet(key: string) {
  const e = bind.editing;
  if (!e || e.set === key) return;
  e.set = key;
  e.view = null;
  refresh();
}

export function dirty(): boolean {
  const e = bind.editing;
  return !!e && JSON.stringify(e.doc) !== e.saved;
}

export async function save(then?: "list" | { controller: string }) {
  const e = bind.editing;
  if (!e || e.busy) return;
  e.busy = true;
  bind.error = "";
  try {
    await api.save(e.target, e.ty, e.doc);
    e.saved = JSON.stringify(e.doc);
    e.personal = true;
    if (e.own) {
      bind.ownPersonal = await api.ownPersonal();
      notify("Saved · in use in the headset now");
    } else {
      if (e.game && !e.game.personal.includes(e.ty)) e.game.personal = [...e.game.personal, e.ty];
      notify(`Saved · ${e.name} uses it next time it starts`);
    }
    if (then === "list") close();
    else if (then) switchController(then.controller, true);
  } catch (err) {
    bind.error = `Couldn't save: ${err}`;
  } finally {
    if (bind.editing === e) e.busy = false;
  }
}

/** Back to the default binding (the personal one is kept as `.bak`). */
export async function reset() {
  const e = bind.editing;
  if (!e || e.busy) return;
  e.busy = true;
  try {
    await api.reset(e.target, e.ty);
    if (e.own) {
      bind.ownPersonal = await api.ownPersonal();
      notify("Back to Monadeck's default controls");
    } else {
      if (e.game) e.game.personal = e.game.personal.filter((t) => t !== e.ty);
      notify(`${e.name} is back to its own controls`);
    }
    await reopen(e.ty, e.set);
  } catch (err) {
    bind.error = `Couldn't reset: ${err}`;
    e.busy = false;
  }
}

function reopen(ty: string, set?: string) {
  const e = bind.editing;
  if (!e) return;
  return e.own ? openOwn(ty, set) : e.game ? openGame(e.game, ty, set) : undefined;
}

export function undo() {
  const e = bind.editing;
  if (!e) return;
  e.doc = JSON.parse(e.saved);
  refresh();
}

/** Another controller's binding (asks first when there are unsaved changes). */
export function switchController(ty: string, force = false) {
  const e = bind.editing;
  if (!e || (e.ty === ty && !force)) return;
  if (!force && dirty()) {
    e.popup = { kind: "leave", to: { controller: ty } };
    return;
  }
  reopen(ty);
}

/** Back to the list (asks first when there are unsaved changes). */
export function leave() {
  const e = bind.editing;
  if (!e) return;
  if (dirty()) e.popup = { kind: "leave", to: "list" };
  else close();
}

/** The JSON view: edits the binding as text (applied while it parses). */
export function setJson(on: boolean) {
  const e = bind.editing;
  if (!e) return;
  e.json = on;
  e.jsonText = JSON.stringify(e.doc, null, 2);
  e.jsonError = "";
}

export async function editJson(text: string) {
  const e = bind.editing;
  if (!e) return;
  e.jsonText = text;
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (err) {
    e.jsonError = `${err}`;
    return;
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    e.jsonError = "A binding is a JSON object";
    return;
  }
  e.jsonError = "";
  e.doc = parsed as Doc;
  await refresh();
}
