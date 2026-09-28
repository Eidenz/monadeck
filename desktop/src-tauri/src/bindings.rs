//! Tauri commands for the binding editor: thin wrappers over `core::bindings`
//! (the same model the in-headset editor uses). The page holds the binding as
//! JSON and sends it back with each edit; every rule (mirror mode, chords,
//! what xrizer reads, Monadeck's own bindings) lives in core. Plus cover art
//! and the user's extra scan folders.

use monadeck_core::bindings::{self as core, own, BindingDoc, Controller, Hand, Mode};
use monadeck_core::steam;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

type CmdResult<T> = Result<T, String>;

// --- what the page is shown --------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDto {
    pub name: String,
    /// Steam appid or non-Steam shortcut id (cover art).
    pub cover_id: Option<String>,
    pub actions_path: String,
    pub dirs: Vec<String>,
    pub controllers: Vec<String>,
    pub personal: Vec<String>,
    pub last_played: Option<u64>,
}

/// Whose binding: a game's (its actions.json + folders), else Monadeck's own.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetDto {
    pub actions_path: Option<String>,
    #[serde(default)]
    pub dirs: Vec<String>,
}

impl TargetDto {
    fn dirs(&self) -> Vec<PathBuf> {
        self.dirs.iter().map(PathBuf::from).collect()
    }
}

#[derive(Serialize)]
pub struct InputDto {
    id: &'static str,
    label: &'static str,
    side: &'static str,
    modes: Vec<&'static str>,
    touch: bool,
    note: &'static str,
}

#[derive(Serialize)]
pub struct SpotDto {
    id: &'static str,
    x: f32,
    y: f32,
    r: f32,
}

#[derive(Serialize)]
pub struct ControllerDto {
    ty: &'static str,
    name: &'static str,
    inputs: Vec<InputDto>,
    /// (left id, right id) pairs mirror mode swaps.
    mirror: Vec<(&'static str, &'static str)>,
    art: Option<&'static str>,
    spots: Vec<SpotDto>,
}

fn controller_dto(c: &Controller) -> ControllerDto {
    ControllerDto {
        ty: c.ty,
        name: c.name,
        inputs: c
            .inputs
            .iter()
            .map(|i| InputDto {
                id: i.id,
                label: i.label,
                side: match i.side {
                    core::Side::Both => "both",
                    core::Side::Left => "left",
                    core::Side::Right => "right",
                },
                modes: i.modes.iter().map(|m| m.id()).collect(),
                touch: i.touch,
                note: i.note,
            })
            .collect(),
        mirror: c.mirror.to_vec(),
        art: core::art(c.ty),
        spots: core::spots(c.ty).iter().map(|&(id, x, y, r)| SpotDto { id, x, y, r }).collect(),
    }
}

#[derive(Serialize)]
pub struct SlotDto {
    key: &'static str,
    label: &'static str,
    kind: &'static str,
}

#[derive(Serialize)]
pub struct ModeDto {
    id: &'static str,
    label: &'static str,
    blurb: &'static str,
    slots: Vec<SlotDto>,
}

#[derive(Serialize)]
pub struct SetDto {
    key: String,
    name: String,
    hidden: bool,
}

#[derive(Serialize)]
pub struct ActionDto {
    key: String,
    name: String,
    kind: &'static str,
    mandatory: bool,
    set: String,
}

#[derive(Serialize)]
pub struct ManifestDto {
    sets: Vec<SetDto>,
    actions: Vec<ActionDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedDto {
    manifest: ManifestDto,
    doc: Value,
    personal: bool,
    personal_path: Option<String>,
}

#[derive(Serialize)]
pub struct SourceDto {
    index: usize,
    hand: Option<&'static str>,
    input: String,
    mode: String,
    /// Mode known to xrizer / Monadeck.
    known: bool,
    outputs: Vec<(String, String)>,
}

#[derive(Serialize)]
pub struct ChordInputDto {
    hand: Option<&'static str>,
    input: String,
    slot: String,
}

#[derive(Serialize)]
pub struct ChordDto {
    index: usize,
    action: String,
    inputs: Vec<ChordInputDto>,
}

#[derive(Serialize)]
pub struct PoseDto {
    action: String,
    hand: &'static str,
    point: String,
}

/// Everything the page draws for one action set of a binding.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewDto {
    sources: Vec<SourceDto>,
    chords: Vec<ChordDto>,
    /// Inputs in use, per set key (the tabs' badges).
    counts: HashMap<String, usize>,
    /// Every action something drives, in any set (lowercased).
    bound: Vec<String>,
    /// (set, mode) of sources xrizer can't read (games only).
    unreadable: Vec<(String, String)>,
    haptics: [Option<String>; 2],
    poses: Vec<PoseDto>,
    /// Why it can't be saved yet (Monadeck's own need a dashboard button).
    blocker: Option<String>,
}

fn hand_id(h: Hand) -> &'static str {
    h.id()
}

fn hand_of(s: &str) -> CmdResult<Hand> {
    match s {
        "left" => Ok(Hand::Left),
        "right" => Ok(Hand::Right),
        _ => Err(format!("no such hand: {s}")),
    }
}

fn mode_of(s: &str) -> CmdResult<Mode> {
    Mode::from_id(s).ok_or_else(|| format!("no such mode: {s}"))
}

fn controller_of(own_bindings: bool, ty: &str) -> CmdResult<&'static Controller> {
    if own_bindings { own::controller(ty) } else { core::controller(ty) }.ok_or_else(|| format!("no such controller: {ty}"))
}

// --- commands ----------------------------------------------------------------------------------

/// Every game with editable bindings, most recently played first.
#[tauri::command]
pub async fn bind_games() -> CmdResult<Vec<GameDto>> {
    let mut games = tauri::async_runtime::spawn_blocking(core::bindable_games).await.map_err(|e| e.to_string())?;
    games.sort_by(|a, b| b.last_played.unwrap_or(0).cmp(&a.last_played.unwrap_or(0)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(games
        .into_iter()
        .map(|g| GameDto {
            name: g.name,
            cover_id: g.app_id.or(g.shortcut_id),
            actions_path: g.actions_path.to_string_lossy().to_string(),
            dirs: g.dirs.iter().map(|d| d.to_string_lossy().to_string()).collect(),
            controllers: g.controllers.iter().map(|c| c.to_string()).collect(),
            personal: g.personal.iter().map(|c| c.to_string()).collect(),
            last_played: g.last_played,
        })
        .collect())
}

/// Whether the xrizer games run on picks up a saved binding while they run.
#[tauri::command]
pub async fn bind_live_reload() -> bool {
    tauri::async_runtime::spawn_blocking(core::xrizer_reloads_live).await.unwrap_or(false)
}

/// Monadeck's own controller types with a personal binding.
#[tauri::command]
pub fn bind_own_personal() -> Vec<&'static str> {
    own::CONTROLLERS.iter().map(|c| c.ty).filter(|t| own::has_personal(t)).collect()
}

#[tauri::command]
pub fn bind_controllers(own_bindings: bool) -> Vec<ControllerDto> {
    let list = if own_bindings { own::CONTROLLERS } else { core::CONTROLLERS };
    list.iter().map(controller_dto).collect()
}

#[tauri::command]
pub fn bind_modes() -> Vec<ModeDto> {
    Mode::ALL
        .iter()
        .map(|m| ModeDto {
            id: m.id(),
            label: m.label(),
            blurb: m.blurb(),
            slots: m.slots().iter().map(|s| SlotDto { key: s.key, label: s.label, kind: s.kind.id() }).collect(),
        })
        .collect()
}

#[tauri::command]
pub async fn bind_open(target: TargetDto, ty: String) -> CmdResult<OpenedDto> {
    let o = tauri::async_runtime::spawn_blocking(move || match &target.actions_path {
        Some(actions) => core::open(std::path::Path::new(actions), &target.dirs(), &ty),
        None => own::open(&ty),
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(OpenedDto {
        manifest: ManifestDto {
            sets: o.manifest.sets.iter().map(|s| SetDto { key: s.key.clone(), name: s.name.clone(), hidden: s.hidden }).collect(),
            actions: o
                .manifest
                .actions
                .iter()
                .map(|a| ActionDto { key: a.key.clone(), name: a.name.clone(), kind: a.kind.id(), mandatory: a.mandatory, set: a.set.clone() })
                .collect(),
        },
        doc: o.doc.value().clone(),
        personal: o.personal,
        personal_path: o.personal_path.map(|p| p.to_string_lossy().to_string()),
    })
}

#[tauri::command]
pub fn bind_view(doc: Value, set: String, own_bindings: bool) -> CmdResult<ViewDto> {
    let d = BindingDoc::from_value(doc)?;
    let counts = d.set_keys().into_iter().map(|k| (k.clone(), d.bound_count(&k))).collect();
    let mut bound: Vec<String> = d
        .set_keys()
        .iter()
        .flat_map(|k| {
            let from_sources = d.sources(k).into_iter().flat_map(|s| s.outputs.into_iter().map(|(_, a)| a.to_ascii_lowercase()));
            let from_chords = d.chords(k).into_iter().filter(|c| !c.inputs.is_empty()).map(|c| c.action.to_ascii_lowercase());
            from_sources.chain(from_chords).collect::<Vec<_>>()
        })
        .collect();
    bound.sort();
    bound.dedup();
    Ok(ViewDto {
        sources: d
            .sources(&set)
            .into_iter()
            .map(|s| SourceDto { index: s.index, hand: s.hand.map(hand_id), known: Mode::from_id(&s.mode).is_some(), input: s.input, mode: s.mode, outputs: s.outputs })
            .collect(),
        chords: d
            .chords(&set)
            .into_iter()
            .map(|c| ChordDto {
                index: c.index,
                action: c.action,
                inputs: c.inputs.into_iter().map(|i| ChordInputDto { hand: i.hand.map(hand_id), input: i.input, slot: i.slot }).collect(),
            })
            .collect(),
        counts,
        bound,
        unreadable: if own_bindings { Vec::new() } else { d.unreadable_modes().into_iter().map(|(s, _, m)| (s, m)).collect() },
        haptics: [d.haptic(&set, Hand::Left), d.haptic(&set, Hand::Right)],
        poses: d.poses(&set).into_iter().map(|p| PoseDto { action: p.action, hand: hand_id(p.hand), point: p.point }).collect(),
        blocker: (own_bindings && !own::drives(&d, own::DASHBOARD))
            .then(|| "Give “Open or close the dashboard” a button first: without one there's no way back into the dashboard".to_string()),
    })
}

/// One change to a binding.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum EditOp {
    Add { hand: String, input: String, mode: String },
    Bind { index: usize, slot: String, action: Option<String> },
    ChangeMode { index: usize, mode: String },
    Remove { index: usize },
    Haptic { hand: String, action: Option<String> },
    Pose { action: String, hand: String, point: Option<String> },
    AddChord { action: String },
    ChordAction { index: usize, action: String },
    ChordToggle { index: usize, hand: String, input: String },
    RemoveChord { index: usize },
    DropUnreadable,
}

#[derive(Serialize)]
pub struct EditResult {
    doc: Value,
    /// The new source / chord, for `add` / `addChord`.
    index: Option<usize>,
}

/// Apply one edit (mirror mode copies it to the other hand, as in the headset).
#[tauri::command]
pub fn bind_edit(doc: Value, ty: String, own_bindings: bool, set: String, mirror: bool, op: EditOp) -> CmdResult<EditResult> {
    let ctrl = controller_of(own_bindings, &ty)?;
    let m = mirror.then_some(ctrl);
    let mut d = BindingDoc::from_value(doc)?;
    let mut index = None;
    match op {
        EditOp::Add { hand, input, mode } => index = Some(d.add_source(&set, hand_of(&hand)?, &input, mode_of(&mode)?, m)),
        EditOp::Bind { index, slot, action } => d.set_output(&set, index, &slot, action.as_deref(), m),
        EditOp::ChangeMode { index, mode } => d.change_mode(&set, index, mode_of(&mode)?, m),
        EditOp::Remove { index } => d.remove_source(&set, index, m),
        EditOp::Haptic { hand, action } => d.set_haptic(&set, hand_of(&hand)?, action.as_deref(), mirror),
        EditOp::Pose { action, hand, point } => d.set_pose(&set, &action, hand_of(&hand)?, point.as_deref(), mirror),
        EditOp::AddChord { action } => index = Some(d.add_chord(&set, &action)),
        EditOp::ChordAction { index, action } => d.set_chord_action(&set, index, &action),
        EditOp::ChordToggle { index, hand, input } => d.toggle_chord_input(&set, index, hand_of(&hand)?, &input, "click"),
        EditOp::RemoveChord { index } => d.remove_chord(&set, index),
        EditOp::DropUnreadable => {
            d.drop_unreadable_modes();
        }
    }
    Ok(EditResult { doc: d.value().clone(), index })
}

#[derive(Serialize)]
pub struct SavedDto {
    pub path: String,
    /// A running game picks it up right away (xrizer reloads bindings live).
    pub live: bool,
}

/// Save as the personal binding (a game's own file is never touched).
#[tauri::command]
pub async fn bind_save(target: TargetDto, ty: String, doc: Value) -> CmdResult<SavedDto> {
    let d = BindingDoc::from_value(doc)?;
    if target.actions_path.is_none() && !own::drives(&d, own::DASHBOARD) {
        return Err("Nothing opens the dashboard: give it a button first".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let (path, live) = match target.actions_path {
            Some(_) => (core::save_personal(&target.dirs(), &ty, &d)?, core::xrizer_reloads_live()),
            None => (own::save(&ty, &d)?, true),
        };
        Ok(SavedDto { path: path.to_string_lossy().to_string(), live })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Back to the default binding (the personal one is kept as `.bak`).
#[tauri::command]
pub async fn bind_reset(target: TargetDto, ty: String) -> CmdResult<bool> {
    tauri::async_runtime::spawn_blocking(move || match target.actions_path {
        Some(_) => core::reset_personal(&target.dirs(), &ty),
        None => own::reset(&ty),
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The controller type to open on when nothing's been picked yet: Quest-style
/// on WiVRn, Index otherwise.
#[tauri::command]
pub fn bind_usual_controller() -> &'static str {
    if monadeck_core::devices::current_backend() == monadeck_core::config::Backend::Wivrn {
        "oculus_touch"
    } else {
        "knuckles"
    }
}

// --- cover art + scan folders -------------------------------------------------------------------

#[tauri::command]
pub async fn game_cover(app_id: String, game_key: Option<String>) -> CmdResult<String> {
    tauri::async_runtime::spawn_blocking(move || steam::game_cover(&app_id, game_key.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn set_custom_cover(game_key: String, image_path: String) -> CmdResult<()> {
    steam::set_custom_cover(&game_key, &image_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_custom_cover(game_key: String) {
    steam::remove_custom_cover(&game_key);
}

#[tauri::command]
pub fn get_custom_paths() -> Vec<String> {
    steam::get_custom_paths()
}

#[tauri::command]
pub fn set_custom_paths(paths: Vec<String>) -> CmdResult<()> {
    steam::set_custom_paths(&paths).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own_default(ty: &str) -> Value {
        own::default_doc(ty).value().clone()
    }

    #[test]
    fn own_edits_round_trip_and_guard_the_dashboard() {
        let doc = own_default("knuckles");
        let v = bind_view(doc.clone(), own::DASHBOARD_SET.into(), true).unwrap();
        assert!(v.blocker.is_none());
        assert_eq!(v.sources.len(), 1);
        assert!(v.bound.contains(&own::DASHBOARD.to_string()) && v.bound.contains(&own::MOVE.to_string()));
        // Take the dashboard button away: the view says why it can't be saved.
        let r = bind_edit(doc, "knuckles".into(), true, own::DASHBOARD_SET.into(), false, EditOp::Remove { index: 0 }).unwrap();
        let v = bind_view(r.doc.clone(), own::DASHBOARD_SET.into(), true).unwrap();
        assert!(v.blocker.is_some() && v.sources.is_empty());
        // Mirror mode: a right system button, then its action, lands on both hands.
        let r = bind_edit(r.doc, "knuckles".into(), true, own::DASHBOARD_SET.into(), true, EditOp::Add { hand: "right".into(), input: "system".into(), mode: "button".into() }).unwrap();
        let i = r.index.unwrap();
        let r = bind_edit(r.doc, "knuckles".into(), true, own::DASHBOARD_SET.into(), true, EditOp::Bind { index: i, slot: "click".into(), action: Some(own::DASHBOARD.into()) }).unwrap();
        let v = bind_view(r.doc, own::DASHBOARD_SET.into(), true).unwrap();
        assert_eq!(v.sources.iter().filter(|s| s.outputs.len() == 1).count(), 2);
        assert!(v.blocker.is_none());
    }

    #[test]
    fn chords_and_unknown_ops() {
        let doc = own_default(own::GLOVES);
        let v = bind_view(doc.clone(), own::PLAYSPACE.into(), true).unwrap();
        assert_eq!(v.chords.len(), 2);
        let r = bind_edit(doc, own::GLOVES.into(), true, own::PLAYSPACE.into(), false, EditOp::ChordToggle { index: 0, hand: "left".into(), input: "trigger".into() }).unwrap();
        let v = bind_view(r.doc, own::PLAYSPACE.into(), true).unwrap();
        assert_eq!(v.chords[0].inputs.len(), 3);
        assert!(bind_edit(Value::Null, "knuckles".into(), false, "/actions/x".into(), false, EditOp::DropUnreadable).is_err());
        assert!(bind_edit(own_default("knuckles"), "nope".into(), true, "/x".into(), false, EditOp::DropUnreadable).is_err());
    }
}
