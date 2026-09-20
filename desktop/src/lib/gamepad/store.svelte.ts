// Gamepad remap editor state (its own window). The list mirrors the files in
// the profiles folder; `draft` is the copy being edited, saved back as JSON.
// The overlay watches the folder, so a save shows up on the watch within a
// couple of seconds.
import * as api from "./api";
import { newRule, type Hand, type Profile, type ProfileFile, type Rule } from "./types";

export const gp = $state({
  files: [] as ProfileFile[],
  dir: "",
  selected: null as number | null,
  draft: null as Profile | null,
  /// JSON of the draft as loaded, for dirty tracking.
  baseline: "",
  /// File the draft belongs to ("" = unsaved new profile).
  file: "",
  isNew: false,
  loading: false,
  saving: false,
  error: "",
  status: "",
});

export function isDirty(): boolean {
  return !!gp.draft && JSON.stringify(gp.draft) !== gp.baseline;
}

/// The stock layout is read-only: duplicate it to edit.
export function isReadOnly(): boolean {
  return gp.selected !== null && !gp.isNew && !!gp.files[gp.selected]?.stock;
}

export async function load(keepFile?: string) {
  gp.loading = true;
  gp.error = "";
  try {
    const [files, dir] = await Promise.all([api.listProfiles(), api.profilesDir()]);
    gp.files = files;
    gp.dir = dir;
    const want = keepFile ?? gp.file;
    const i = files.findIndex((f) => f.file === want && want !== "");
    if (i >= 0) select(i);
    else if (gp.draft && gp.isNew) {
      /* keep the unsaved draft */
    } else if (files.length) select(0);
    else clear();
  } catch (e) {
    gp.error = String(e);
  } finally {
    gp.loading = false;
  }
}

function clear() {
  gp.selected = null;
  gp.draft = null;
  gp.baseline = "";
  gp.file = "";
  gp.isNew = false;
}

export function select(i: number) {
  const f = gp.files[i];
  if (!f) return;
  // `$state.snapshot` — a plain deep copy; structuredClone can't take a
  // state proxy.
  gp.draft = $state.snapshot(f.profile) as Profile;
  gp.selected = i;
  gp.baseline = JSON.stringify(gp.draft);
  gp.file = f.file;
  gp.isNew = false;
  gp.status = "";
}

function uniqueName(base: string): string {
  const taken = new Set(gp.files.map((f) => f.profile.name.toLowerCase()));
  if (!taken.has(base.toLowerCase())) return base;
  let n = 2;
  while (taken.has(`${base} ${n}`.toLowerCase())) n++;
  return `${base} ${n}`;
}

/// A fresh profile (from the stock layout, so every input already does
/// something sensible).
export function newProfile() {
  const stock = gp.files.find((f) => f.stock)?.profile;
  const rules = stock ? ($state.snapshot(stock.rules) as Rule[]) : [];
  gp.draft = { name: uniqueName("New profile"), game: null, rules };
  gp.baseline = "";
  gp.file = "";
  gp.isNew = true;
  gp.selected = null;
  gp.status = "";
}

export function duplicate() {
  if (!gp.draft) return;
  const copy = $state.snapshot(gp.draft) as Profile;
  copy.name = uniqueName(`${copy.name} copy`);
  gp.draft = copy;
  gp.baseline = "";
  gp.file = "";
  gp.isNew = true;
  gp.selected = null;
  gp.status = "";
}

export async function save() {
  if (!gp.draft || isReadOnly()) return;
  const name = gp.draft.name.trim();
  if (!name) {
    gp.error = "Give the profile a name first.";
    return;
  }
  gp.draft.name = name;
  if (gp.draft.game !== null && !gp.draft.game.trim()) gp.draft.game = null;
  gp.saving = true;
  gp.error = "";
  try {
    const file = await api.saveProfile(gp.file, gp.draft);
    gp.status = `Saved ${file} · the overlay picks it up in a moment`;
    await load(file);
  } catch (e) {
    gp.error = String(e);
  } finally {
    gp.saving = false;
  }
}

export async function remove() {
  if (!gp.file || isReadOnly()) return;
  gp.error = "";
  try {
    await api.deleteProfile(gp.file);
    gp.status = "Deleted";
    gp.file = "";
    gp.draft = null;
    await load();
  } catch (e) {
    gp.error = String(e);
  }
}

/// Throw away the draft's edits.
export function revert() {
  if (gp.selected !== null) select(gp.selected);
  else if (gp.isNew) clear();
}

export function addRule(hand: Hand) {
  if (!gp.draft || isReadOnly()) return;
  gp.draft.rules.push(newRule(hand));
}

export function updateRule(i: number, patch: Partial<Rule>) {
  if (!gp.draft || isReadOnly()) return;
  const r = gp.draft.rules[i];
  if (r) Object.assign(r, patch);
}

export function removeRule(i: number) {
  if (!gp.draft || isReadOnly()) return;
  gp.draft.rules.splice(i, 1);
}
