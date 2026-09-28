// Tauri command bridge for the binding editor. The binding itself travels as
// JSON: the page keeps it, core (Rust) reads and edits it — the same model the
// in-headset editor uses. Arg keys are camelCase (Tauri maps them to the
// snake_case Rust params).
import { invoke } from "@tauri-apps/api/core";

export type HandId = "left" | "right";
/** A binding document (SteamVR binding JSON), opaque to the page. */
export type Doc = Record<string, unknown>;

export interface Game {
  name: string;
  coverId: string | null;
  actionsPath: string;
  dirs: string[];
  controllers: string[];
  personal: string[];
  lastPlayed: number | null;
}

/** Whose binding: a game's, or (no actionsPath) Monadeck's own. */
export interface Target {
  actionsPath?: string;
  dirs?: string[];
}

export interface InputDef {
  id: string;
  label: string;
  side: "both" | "left" | "right";
  modes: string[];
  touch: boolean;
  note: string;
}

export interface Spot {
  id: string;
  x: number;
  y: number;
  r: number;
}

export interface Controller {
  ty: string;
  name: string;
  inputs: InputDef[];
  mirror: [string, string][];
  art: "index" | "touch" | "vive" | null;
  spots: Spot[];
}

export interface Slot {
  key: string;
  label: string;
  kind: string;
}

export interface ModeDef {
  id: string;
  label: string;
  blurb: string;
  slots: Slot[];
}

export interface ActionSet {
  key: string;
  name: string;
  hidden: boolean;
}

export interface Action {
  key: string;
  name: string;
  kind: string;
  mandatory: boolean;
  set: string;
}

export interface Manifest {
  sets: ActionSet[];
  actions: Action[];
}

export interface Opened {
  manifest: Manifest;
  doc: Doc;
  personal: boolean;
  personalPath: string | null;
}

export interface Source {
  index: number;
  hand: HandId | null;
  input: string;
  mode: string;
  known: boolean;
  outputs: [string, string][];
}

export interface Chord {
  index: number;
  action: string;
  inputs: { hand: HandId | null; input: string; slot: string }[];
}

export interface View {
  sources: Source[];
  chords: Chord[];
  counts: Record<string, number>;
  /** Every action something drives, in any set (lowercased). */
  bound: string[];
  unreadable: [string, string][];
  haptics: [string | null, string | null];
  poses: { action: string; hand: HandId; point: string }[];
  blocker: string | null;
}

export type EditOp =
  | { op: "add"; hand: HandId; input: string; mode: string }
  | { op: "bind"; index: number; slot: string; action: string | null }
  | { op: "changeMode"; index: number; mode: string }
  | { op: "remove"; index: number }
  | { op: "haptic"; hand: HandId; action: string | null }
  | { op: "pose"; action: string; hand: HandId; point: string | null }
  | { op: "addChord"; action: string }
  | { op: "chordAction"; index: number; action: string }
  | { op: "chordToggle"; index: number; hand: HandId; input: string }
  | { op: "removeChord"; index: number }
  | { op: "dropUnreadable" };

export const games = () => invoke<Game[]>("bind_games");
export const liveReload = () => invoke<boolean>("bind_live_reload");
export const ownPersonal = () => invoke<string[]>("bind_own_personal");
export const controllers = (ownBindings: boolean) => invoke<Controller[]>("bind_controllers", { ownBindings });
export const modes = () => invoke<ModeDef[]>("bind_modes");
export const open = (target: Target, ty: string) => invoke<Opened>("bind_open", { target, ty });
export const view = (doc: Doc, set: string, ownBindings: boolean) => invoke<View>("bind_view", { doc, set, ownBindings });
export const edit = (doc: Doc, ty: string, ownBindings: boolean, set: string, mirror: boolean, op: EditOp) =>
  invoke<{ doc: Doc; index: number | null }>("bind_edit", { doc, ty, ownBindings, set, mirror, op });
export const save = (target: Target, ty: string, doc: Doc) => invoke<{ path: string; live: boolean }>("bind_save", { target, ty, doc });
export const reset = (target: Target, ty: string) => invoke<boolean>("bind_reset", { target, ty });
export const usualController = () => invoke<string>("bind_usual_controller");

export const gameCover = (appId: string) => invoke<string>("game_cover", { appId, gameKey: null });
export const getCustomPaths = () => invoke<string[]>("get_custom_paths");
export const setCustomPaths = (paths: string[]) => invoke<void>("set_custom_paths", { paths });
