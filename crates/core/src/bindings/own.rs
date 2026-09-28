//! Monadeck's own controls, bound like a game's: the same action-set manifest,
//! the same binding files (one per controller, in `~/.config/monadeck/bindings/`),
//! the same editor. The overlay reads its controllers raw and runs these
//! bindings itself ([`Evaluator`]), so unlike a game's they apply at once and
//! can use chords (several inputs held together), which xrizer ignores.
//!
//! Four sets: the dashboard (open / close: required, or there'd be no way back
//! in), the screens (hide them all / bring them back), the mouse on a screen
//! (the clicks and the wheel, only while a laser is on one, and there they come
//! first: a button they use does nothing else on that hand), and the playspace
//! drag (*Move the playspace* while held, a double press snaps it back, plus an
//! optional *Snap it back*).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::{BindingDoc, Controller, Hand, InputDef, Manifest, Mode, Opened, Side};
use crate::paths::monadeck_config_dir;

pub const DASHBOARD_SET: &str = "/actions/dashboard";
pub const DASHBOARD: &str = "/actions/dashboard/in/toggle";
pub const SCREENS_SET: &str = "/actions/screens";
pub const SCREENS: &str = "/actions/screens/in/toggle";
pub const MOUSE_SET: &str = "/actions/mouse";
pub const CLICK: &str = "/actions/mouse/in/click";
pub const RIGHT_CLICK: &str = "/actions/mouse/in/right_click";
pub const MIDDLE_CLICK: &str = "/actions/mouse/in/middle_click";
/// A left click that holds the cursor where it was pressed.
pub const STILL_CLICK: &str = "/actions/mouse/in/still_click";
pub const SCROLL: &str = "/actions/mouse/in/scroll";
pub const PLAYSPACE: &str = "/actions/playspace";
pub const MOVE: &str = "/actions/playspace/in/move";
pub const SNAP: &str = "/actions/playspace/in/snap_back";
/// UdCap gloves: Index-style inputs, no trackpad.
pub const GLOVES: &str = "udcap_gloves";

const MANIFEST: &str = r#"{
  "action_sets": [
    { "name": "/actions/dashboard", "usage": "leftright" },
    { "name": "/actions/screens", "usage": "leftright" },
    { "name": "/actions/mouse", "usage": "leftright" },
    { "name": "/actions/playspace", "usage": "leftright" }
  ],
  "actions": [
    { "name": "/actions/dashboard/in/toggle", "type": "boolean", "requirement": "mandatory" },
    { "name": "/actions/screens/in/toggle", "type": "boolean" },
    { "name": "/actions/mouse/in/click", "type": "boolean" },
    { "name": "/actions/mouse/in/right_click", "type": "boolean" },
    { "name": "/actions/mouse/in/middle_click", "type": "boolean" },
    { "name": "/actions/mouse/in/still_click", "type": "boolean" },
    { "name": "/actions/mouse/in/scroll", "type": "vector2" },
    { "name": "/actions/playspace/in/move", "type": "boolean" },
    { "name": "/actions/playspace/in/snap_back", "type": "boolean" }
  ],
  "localization": [ {
    "language_tag": "en_US",
    "/actions/dashboard": "Dashboard",
    "/actions/dashboard/in/toggle": "Open or close the dashboard",
    "/actions/screens": "Screens",
    "/actions/screens/in/toggle": "Hide or bring back your screens",
    "/actions/mouse": "Mouse",
    "/actions/mouse/in/click": "Left click",
    "/actions/mouse/in/right_click": "Right click",
    "/actions/mouse/in/middle_click": "Middle click",
    "/actions/mouse/in/still_click": "Click without moving the cursor",
    "/actions/mouse/in/scroll": "Scroll wheel",
    "/actions/playspace": "Playspace",
    "/actions/playspace/in/move": "Move the playspace",
    "/actions/playspace/in/snap_back": "Snap it back"
  } ]
}"#;

pub fn manifest() -> Manifest {
    Manifest::parse(MANIFEST).expect("Monadeck's own manifest")
}

use Mode as M;

// The inputs the overlay reads (it has no touch sensing but the trackpad's).
const INDEX: &[InputDef] = &[
    InputDef { id: "trigger", label: "Trigger", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "trackpad", label: "Trackpad", side: Side::Both, modes: &[M::Button, M::ToggleButton, M::Dpad, M::Scroll], touch: true, note: "" },
    InputDef { id: "thumbstick", label: "Thumbstick", side: Side::Both, modes: &[M::Button, M::ToggleButton, M::Dpad, M::Scroll], touch: false, note: "" },
    InputDef { id: "a", label: "A button", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "b", label: "B button", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "grip", label: "Grip", side: Side::Both, modes: &[M::Grab, M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "system", label: "System button", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
];

const TOUCH: &[InputDef] = &[
    InputDef { id: "trigger", label: "Trigger", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "joystick", label: "Thumbstick", side: Side::Both, modes: &[M::Button, M::ToggleButton, M::Dpad, M::Scroll], touch: false, note: "" },
    InputDef { id: "x", label: "X button", side: Side::Left, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "y", label: "Y button", side: Side::Left, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "a", label: "A button", side: Side::Right, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "b", label: "B button", side: Side::Right, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "grip", label: "Grip", side: Side::Both, modes: &[M::Grab, M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "application_menu", label: "Menu button", side: Side::Left, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "system", label: "System button", side: Side::Right, modes: &[], touch: false, note: "Reserved for the headset" },
];

const GLOVE: &[InputDef] = &[
    InputDef { id: "trigger", label: "Trigger", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "thumbstick", label: "Thumbstick", side: Side::Both, modes: &[M::Button, M::ToggleButton, M::Dpad, M::Scroll], touch: false, note: "" },
    InputDef { id: "a", label: "A button", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "b", label: "B button", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "grip", label: "Grip", side: Side::Both, modes: &[M::Grab, M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "system", label: "System button", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
];

/// The controllers Monadeck's own bindings exist for (`xrizer_file` is the
/// binding's file name here).
pub const CONTROLLERS: &[Controller] = &[
    Controller { ty: "knuckles", name: "Index controllers", xrizer_file: "knuckles.json", inputs: INDEX, mirror: &[] },
    Controller { ty: "oculus_touch", name: "Touch controllers", xrizer_file: "oculus_touch.json", inputs: TOUCH, mirror: &[("x", "a"), ("y", "b")] },
    Controller { ty: GLOVES, name: "UdCap gloves", xrizer_file: "udcap_gloves.json", inputs: GLOVE, mirror: &[] },
];

pub fn controller(ty: &str) -> Option<&'static Controller> {
    CONTROLLERS.iter().find(|c| c.ty == ty)
}

/// The shipped binding: the left system button (menu on Touch) opens the
/// dashboard, a double press of the left B (Y) hides / brings back the screens,
/// on a screen the trigger clicks, A (X) right-clicks and the stick scrolls (B
/// is left free, so the double press works there too), and the playspace
/// moves with the trackpad on Index controllers, with the two face buttons
/// together on Touch controllers and gloves (no trackpad there).
pub fn default_doc(ty: &str) -> BindingDoc {
    let mut d = BindingDoc::empty(ty);
    let stick = if ty == "oculus_touch" { "joystick" } else { "thumbstick" };
    for hand in [Hand::Left, Hand::Right] {
        let a = if ty == "oculus_touch" && hand == Hand::Left { "x" } else { "a" };
        let i = d.add_source(MOUSE_SET, hand, "trigger", Mode::Button, None);
        d.set_output(MOUSE_SET, i, "click", Some(CLICK), None);
        let i = d.add_source(MOUSE_SET, hand, a, Mode::Button, None);
        d.set_output(MOUSE_SET, i, "click", Some(RIGHT_CLICK), None);
        let i = d.add_source(MOUSE_SET, hand, stick, Mode::Scroll, None);
        d.set_output(MOUSE_SET, i, "scroll", Some(SCROLL), None);
    }
    if ty == "knuckles" {
        for hand in [Hand::Left, Hand::Right] {
            let i = d.add_source(PLAYSPACE, hand, "trackpad", Mode::Button, None);
            d.set_output(PLAYSPACE, i, "click", Some(MOVE), None);
        }
    } else {
        face_chords(&mut d, ty);
    }
    let (menu, b) = if ty == "oculus_touch" { ("application_menu", "y") } else { ("system", "b") };
    let i = d.add_source(DASHBOARD_SET, Hand::Left, menu, Mode::Button, None);
    d.set_output(DASHBOARD_SET, i, "click", Some(DASHBOARD), None);
    let i = d.add_source(SCREENS_SET, Hand::Left, b, Mode::Button, None);
    d.set_output(SCREENS_SET, i, "double", Some(SCREENS), None);
    d
}

/// Both face buttons held together move the playspace, on each hand.
fn face_chords(d: &mut BindingDoc, ty: &str) {
    for (hand, first, second) in [(Hand::Left, "x", "y"), (Hand::Right, "a", "b")] {
        let (first, second) = if ty == "oculus_touch" { (first, second) } else { ("a", "b") };
        let c = d.add_chord(PLAYSPACE, MOVE);
        d.toggle_chord_input(PLAYSPACE, c, hand, first, "click");
        d.toggle_chord_input(PLAYSPACE, c, hand, second, "click");
    }
}

/// Whether anything in the binding drives `action`.
pub fn drives(doc: &BindingDoc, action: &str) -> bool {
    doc.set_keys().iter().any(|set| {
        doc.sources(set).iter().any(|s| s.outputs.iter().any(|(_, a)| a.eq_ignore_ascii_case(action)))
            || doc.chords(set).iter().any(|c| !c.inputs.is_empty() && c.action.eq_ignore_ascii_case(action))
    })
}

/// Sets a binding saved before they existed get their defaults (an empty set
/// the user cleared stays empty).
fn fill_missing_sets(doc: &mut BindingDoc, ty: &str) {
    let def = default_doc(ty);
    let have = doc.set_keys();
    for set in def.set_keys() {
        if !have.contains(&set) {
            doc.copy_set(&def, &set);
        }
    }
}

/// Where Monadeck's own bindings live (one file per controller).
pub fn dir() -> PathBuf {
    monadeck_config_dir().join("bindings")
}

pub fn path(ty: &str) -> Option<PathBuf> {
    controller(ty).map(|c| dir().join(c.xrizer_file))
}

fn personal(ty: &str) -> Option<BindingDoc> {
    let text = std::fs::read_to_string(path(ty)?).ok()?;
    let mut doc = BindingDoc::parse(&text).map_err(|e| log::warn!("bindings: {ty}: {e}")).ok()?;
    fill_missing_sets(&mut doc, ty);
    Some(doc)
}

/// When the bindings folder last changed (the overlay reloads on a change,
/// e.g. a save from the desktop editor).
pub fn dir_mtime() -> Option<std::time::SystemTime> {
    std::fs::read_dir(dir())
        .ok()?
        .flatten()
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .chain(std::fs::metadata(dir()).ok().and_then(|m| m.modified().ok()))
        .max()
}

pub fn has_personal(ty: &str) -> bool {
    path(ty).is_some_and(|p| p.is_file())
}

/// The binding in use for a controller: yours, else the default. Never one
/// without a way to open the dashboard (the editor won't save that, but a
/// hand-edited file could): the default's comes back.
pub fn load(ty: &str) -> BindingDoc {
    let mut doc = personal(ty).unwrap_or_else(|| default_doc(ty));
    if !drives(&doc, DASHBOARD) {
        log::warn!("bindings: {ty}: nothing opens the dashboard, using the default button for it");
        doc.copy_set(&default_doc(ty), DASHBOARD_SET);
    }
    doc
}

pub fn open(ty: &str) -> Result<Opened, String> {
    let p = path(ty).ok_or_else(|| format!("Monadeck has no bindings for {ty}"))?;
    let mine = personal(ty);
    Ok(Opened { manifest: manifest(), personal: mine.is_some(), doc: mine.unwrap_or_else(|| default_doc(ty)), default_path: None, personal_path: Some(p) })
}

pub fn save(ty: &str, doc: &BindingDoc) -> Result<PathBuf, String> {
    let p = path(ty).ok_or_else(|| format!("Monadeck has no bindings for {ty}"))?;
    std::fs::create_dir_all(dir()).map_err(|e| format!("{}: {e}", dir().display()))?;
    std::fs::write(&p, doc.to_json()).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(p)
}

/// Back to the default (yours is kept as `<file>.bak`).
pub fn reset(ty: &str) -> Result<bool, String> {
    let Some(p) = path(ty).filter(|p| p.is_file()) else {
        return Ok(false);
    };
    let mut bak = p.clone().into_os_string();
    bak.push(".bak");
    std::fs::rename(&p, &bak).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(true)
}

/// Carry the old Playspace settings (hands: both / left / right, button:
/// auto / pad / ab) over into bindings, once, when they weren't the defaults
/// and no binding of yours exists yet. Returns whether anything was written.
pub fn migrate_legacy(hands: &str, button: &str) -> bool {
    let keep_hand = |h: Hand| match hands {
        "left" => h == Hand::Left,
        "right" => h == Hand::Right,
        _ => true,
    };
    if (button != "ab" && hands != "left" && hands != "right") || CONTROLLERS.iter().any(|c| has_personal(c.ty)) {
        return false;
    }
    let mut wrote = false;
    for c in CONTROLLERS {
        let mut d = default_doc(c.ty);
        if button == "ab" && c.ty == "knuckles" {
            for s in d.sources(PLAYSPACE).into_iter().rev() {
                d.remove_source(PLAYSPACE, s.index, None);
            }
            face_chords(&mut d, c.ty);
        }
        for s in d.sources(PLAYSPACE).into_iter().rev() {
            if s.hand.is_some_and(|h| !keep_hand(h)) {
                d.remove_source(PLAYSPACE, s.index, None);
            }
        }
        for ch in d.chords(PLAYSPACE).into_iter().rev() {
            if ch.inputs.first().and_then(|i| i.hand).is_some_and(|h| !keep_hand(h)) {
                d.remove_chord(PLAYSPACE, ch.index);
            }
        }
        if d != default_doc(c.ty) && save(c.ty, &d).is_ok() {
            wrote = true;
        }
    }
    wrote
}

/// Carry the old "B is a middle click on a screen" setting (before 1.8) into
/// the mouse set, on every controller (Y on a left Touch controller).
/// Returns whether anything was written.
pub fn migrate_b_middle() -> bool {
    let mut wrote = false;
    for c in CONTROLLERS {
        let mut d = load(c.ty);
        for hand in [Hand::Left, Hand::Right] {
            let b = if c.ty == "oculus_touch" && hand == Hand::Left { "y" } else { "b" };
            if d.sources(MOUSE_SET).iter().any(|s| s.hand == Some(hand) && s.input == b) {
                continue;
            }
            let i = d.add_source(MOUSE_SET, hand, b, Mode::Button, None);
            d.set_output(MOUSE_SET, i, "click", Some(MIDDLE_CLICK), None);
        }
        wrote |= save(c.ty, &d).is_ok();
    }
    wrote
}

/// What drives `action`, per hand, in words ("Trackpad (double press)",
/// "A button + B button").
fn inputs_for(doc: &BindingDoc, ctrl: &Controller, action: &str) -> [Vec<String>; 2] {
    let label = |id: &str| ctrl.input(id).map_or_else(|| super::pretty_name(id), |d| d.label.to_string());
    let set = action.split("/in/").next().unwrap_or(action);
    let mut per_hand: [Vec<String>; 2] = Default::default();
    for s in doc.sources(set) {
        let Some(hand) = s.hand else { continue };
        for (slot, a) in &s.outputs {
            if !a.eq_ignore_ascii_case(action) {
                continue;
            }
            let how = match (s.mode.as_str(), slot.as_str()) {
                ("toggle_button", _) => " (toggles)",
                (_, "touch") => " (touch)",
                (_, "double") => " (double press)",
                ("dpad", "north") => " up",
                ("dpad", "south") => " down",
                ("dpad", "west") => " left",
                ("dpad", "east") => " right",
                ("dpad", "center") => " centre",
                _ => "",
            };
            per_hand[hand as usize].push(format!("{}{how}", label(&s.input)));
        }
    }
    for c in doc.chords(set) {
        let Some(hand) = c.inputs.first().and_then(|i| i.hand) else { continue };
        if c.action.eq_ignore_ascii_case(action) && c.inputs.len() > 1 {
            per_hand[hand as usize].push(c.inputs.iter().map(|i| label(&i.input)).collect::<Vec<_>>().join(" + "));
        }
    }
    per_hand
}

/// In words, what drives `action` (for the Playspace page): "Trackpad",
/// "A button + B button", per hand when the hands differ.
pub fn summary(doc: &BindingDoc, ctrl: &Controller, action: &str) -> String {
    let per_hand = inputs_for(doc, ctrl, action);
    let join = |v: &[String]| v.join(" or ");
    match (per_hand[0].is_empty(), per_hand[1].is_empty()) {
        (true, true) => "Nothing".into(),
        _ if per_hand[0] == per_hand[1] => format!("{} on either hand", join(&per_hand[0])),
        (false, true) => format!("{} on the left hand", join(&per_hand[0])),
        (true, false) => format!("{} on the right hand", join(&per_hand[1])),
        _ => format!("Left: {} · right: {}", join(&per_hand[0]), join(&per_hand[1])),
    }
}

/// In words, what the mouse does on a screen (for the Desktop page):
/// "Trigger: left click · A button: right click · Thumbstick: scroll wheel".
pub fn mouse_summary(doc: &BindingDoc, ctrl: &Controller) -> String {
    let m = manifest();
    let parts: Vec<String> = [CLICK, RIGHT_CLICK, MIDDLE_CLICK, STILL_CLICK, SCROLL]
        .into_iter()
        .filter_map(|a| {
            let [l, r] = inputs_for(doc, ctrl, a);
            let mut both: Vec<&String> = l.iter().chain(&r).collect();
            both.dedup();
            let who = match (l.is_empty(), r.is_empty()) {
                (true, true) => return None,
                (false, true) => format!("{} (left)", l.join(" or ")),
                (true, false) => format!("{} (right)", r.join(" or ")),
                _ if l == r => l.join(" or "),
                _ => both.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" or "),
            };
            let what = m.action_name(a);
            let mut c = what.chars();
            let what = c.next().map_or(String::new(), |f| f.to_lowercase().chain(c).collect());
            Some(format!("{who}: {what}"))
        })
        .collect();
    if parts.is_empty() { "Nothing: every button is free on a screen".into() } else { parts.join(" · ") }
}

// --- running them ------------------------------------------------------------------------------

/// One hand's controls as the overlay reads them.
#[derive(Clone, Copy, Debug, Default)]
pub struct HandInput {
    pub active: bool,
    pub trigger: f32,
    pub grip: f32,
    /// Right / up positive.
    pub stick: (f32, f32),
    pub stick_click: bool,
    /// A (X on a left Touch controller).
    pub a: bool,
    /// B (Y on a left Touch controller).
    pub b: bool,
    pub pad: (f32, f32),
    pub pad_force: f32,
    pub pad_touch: bool,
    /// The system button (the menu button on a left Touch controller).
    pub system: bool,
}

/// What an input reads: pressed (analog ones past a threshold), touched (if
/// it can tell), and where (sticks and pads).
#[derive(Clone, Copy, Default)]
struct Reading {
    click: bool,
    touch: bool,
    pos: Option<(f32, f32)>,
}

/// Actions driven this frame, per hand.
#[derive(Clone, Debug, Default)]
pub struct Active {
    pub hands: [Vec<String>; 2],
    /// Two-axis actions (the mouse wheel) and their value, per hand.
    pub vectors: [Vec<(String, (f32, f32))>; 2],
    /// Buttons held as part of a chord: they don't count on their own until
    /// let go (A+B dragging mustn't also right-click, nor count as a B press).
    pub chorded: [Vec<String>; 2],
}

impl Active {
    pub fn has(&self, hand: Hand, action: &str) -> bool {
        self.hands[hand as usize].iter().any(|a| a.eq_ignore_ascii_case(action))
    }

    /// Driven by either hand.
    pub fn any(&self, action: &str) -> bool {
        self.has(Hand::Left, action) || self.has(Hand::Right, action)
    }

    /// A two-axis action's value on `hand` (summed when several inputs drive it).
    pub fn vector(&self, hand: Hand, action: &str) -> (f32, f32) {
        self.vectors[hand as usize].iter().filter(|(a, _)| a.eq_ignore_ascii_case(action)).fold((0.0, 0.0), |acc, (_, v)| (acc.0 + v.0, acc.1 + v.1))
    }
}

const DOUBLE: Duration = Duration::from_millis(450);
/// Stick travel ignored around the centre when it scrolls.
const STICK_DEADZONE: f32 = 0.2;
/// A swipe on a trackpad scrolls this many stick-frames' worth per unit of
/// finger travel (the pad is 2 units across).
const SWIPE: f32 = 25.0;

/// Runs bindings against raw input, frame by frame (it keeps the state
/// toggles, double presses and analog thresholds need).
#[derive(Default)]
pub struct Evaluator {
    /// Trigger / grip "pressed" latches, per (hand, input).
    analog: HashMap<(usize, String), bool>,
    /// Per (hand, set, source index, slot): last value (edges) and toggle state.
    prev: HashMap<(usize, String, usize, String), bool>,
    toggled: HashMap<(usize, String, usize, String), bool>,
    last_press: HashMap<(usize, String, usize), Instant>,
    /// (hand, input) held as part of a chord, until released.
    in_chord: Vec<(usize, String)>,
    /// Where a finger last touched a scrolling trackpad, per (hand, input).
    swipe: HashMap<(usize, String), (f32, f32)>,
}

impl Evaluator {
    /// Forget edges and toggles (bindings changed: indices mean other things).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn read(&mut self, hand: usize, inp: &HandInput, input: &str) -> Reading {
        let mut latch = |name: &str, v: f32, on: f32, off: f32| {
            let st = self.analog.entry((hand, name.to_string())).or_default();
            *st = if *st { v > off } else { v > on };
            *st
        };
        match input {
            "trigger" => Reading { click: latch("trigger", inp.trigger, 0.5, 0.4), ..Default::default() },
            "grip" => Reading { click: latch("grip", inp.grip, 0.7, 0.45), ..Default::default() },
            "trackpad" => Reading { click: inp.pad_force > 0.5, touch: inp.pad_touch, pos: Some(inp.pad) },
            "thumbstick" | "joystick" => Reading { click: inp.stick_click, touch: false, pos: Some(inp.stick) },
            "a" | "x" => Reading { click: inp.a, ..Default::default() },
            "b" | "y" => Reading { click: inp.b, ..Default::default() },
            "system" | "application_menu" => Reading { click: inp.system, ..Default::default() },
            _ => Reading::default(),
        }
    }

    /// The wheel from a stick (pushed past the dead zone) or a trackpad
    /// (swiped: the page follows the finger).
    fn scroll(&mut self, hand: usize, input: &str, r: &Reading) -> (f32, f32) {
        let Some((x, y)) = r.pos else { return (0.0, 0.0) };
        if input != "trackpad" {
            let mag = x.hypot(y);
            if mag < STICK_DEADZONE {
                return (0.0, 0.0);
            }
            let k = (mag - STICK_DEADZONE) / (1.0 - STICK_DEADZONE) / mag;
            return (x * k, y * k);
        }
        let key = (hand, input.to_string());
        if !r.touch {
            self.swipe.remove(&key);
            return (0.0, 0.0);
        }
        match self.swipe.insert(key, (x, y)) {
            Some((px, py)) => (-(x - px) * SWIPE, -(y - py) * SWIPE),
            None => (0.0, 0.0),
        }
    }

    /// Evaluate every set of each hand's binding (the two can differ: a glove
    /// on one hand, a controller on the other). `on_screen[h]`: that hand's
    /// laser is on a screen (or holds one of its buttons down), so the mouse
    /// set applies there, and the inputs it uses do nothing else on that hand.
    pub fn eval(&mut self, docs: [&BindingDoc; 2], input: [HandInput; 2], on_screen: [bool; 2], now: Instant) -> Active {
        let mut out = Active::default();
        let hands = [Hand::Left, Hand::Right];
        let in_play = |h: usize, set: &str| on_screen[h] || !set.eq_ignore_ascii_case(MOUSE_SET);
        // A swipe starts over each time a laser comes back onto a screen.
        self.swipe.retain(|(h, _), _| on_screen[*h]);
        // The chords in play: each hand's binding's own, led by that hand.
        let mut chords = Vec::new();
        for (h, hand) in hands.into_iter().enumerate() {
            for set in docs[h].set_keys().into_iter().filter(|s| in_play(h, s)) {
                chords.extend(docs[h].chords(&set).into_iter().filter(|c| c.inputs.first().and_then(|i| i.hand) == Some(hand)).map(|c| (h, c)));
            }
        }
        // On a screen, the mouse's inputs are the screen's.
        let taken: [Vec<String>; 2] = [0, 1].map(|h| {
            if !on_screen[h] {
                return Vec::new();
            }
            docs[h].sources(MOUSE_SET).into_iter().filter(|s| s.hand == Some(hands[h]) && !s.outputs.is_empty()).map(|s| s.input).collect()
        });
        // A button held while another of its chord is: a chord in the making,
        // not a press of its own, until it's let go.
        let held = |ev: &mut Self, hand: Option<Hand>, name: &str| -> bool { hand.is_some_and(|hd| ev.read(hd as usize, &input[hd as usize], name).click) };
        for (_, c) in &chords {
            for (k, i) in c.inputs.iter().enumerate() {
                let Some(ih) = i.hand else { continue };
                if held(self, Some(ih), &i.input) && c.inputs.iter().enumerate().any(|(j, o)| j != k && held(self, o.hand, &o.input)) {
                    let key = (ih as usize, i.input.clone());
                    if !self.in_chord.contains(&key) {
                        self.in_chord.push(key);
                    }
                }
            }
        }
        let still: Vec<bool> = self.in_chord.clone().iter().map(|(h, inp)| self.read(*h, &input[*h], inp).click).collect();
        let mut keep = still.into_iter();
        self.in_chord.retain(|_| keep.next().unwrap_or(false));
        for (h, inp) in &self.in_chord {
            out.chorded[*h].push(inp.clone());
        }

        for (h, hand) in hands.into_iter().enumerate() {
            if !input[h].active {
                continue;
            }
            for set in docs[h].set_keys().into_iter().filter(|s| in_play(h, s)) {
                let mouse = set.eq_ignore_ascii_case(MOUSE_SET);
                for s in docs[h].sources(&set).into_iter().filter(|s| s.hand == Some(hand)) {
                    if !mouse && taken[h].contains(&s.input) {
                        continue;
                    }
                    let Some(mode) = Mode::from_id(&s.mode) else { continue };
                    let mut r = self.read(h, &input[h], &s.input);
                    if self.in_chord.contains(&(h, s.input.clone())) {
                        r.click = false;
                        r.touch = false;
                    }
                    for (slot, action) in &s.outputs {
                        let key = (h, set.clone(), s.index, slot.clone());
                        let on = match (mode, slot.as_str()) {
                            (Mode::Button | Mode::Trigger | Mode::Joystick | Mode::Trackpad, "click") => r.click,
                            (Mode::Button | Mode::Trigger | Mode::Joystick | Mode::Trackpad, "touch") => r.touch,
                            (Mode::Button, "double") => {
                                let was = self.prev.insert(key, r.click).unwrap_or(false);
                                let press = (h, set.clone(), s.index);
                                let mut fired = false;
                                if r.click && !was {
                                    match self.last_press.get(&press) {
                                        Some(t) if now.duration_since(*t) < DOUBLE => {
                                            self.last_press.remove(&press);
                                            fired = true;
                                        }
                                        _ => {
                                            self.last_press.insert(press, now);
                                        }
                                    }
                                }
                                fired
                            }
                            (Mode::ToggleButton, "click" | "touch") => {
                                let v = if slot == "click" { r.click } else { r.touch };
                                let was = self.prev.insert(key.clone(), v).unwrap_or(false);
                                let t = self.toggled.entry(key).or_default();
                                if v && !was {
                                    *t = !*t;
                                }
                                *t
                            }
                            (Mode::Grab, "grab") => r.click,
                            (Mode::Scroll, "scroll") => {
                                let v = self.scroll(h, &s.input, &r);
                                out.vectors[h].push((action.clone(), v));
                                false
                            }
                            (Mode::Dpad, dir) => dpad(&r, dir, docs[h].parameter(&set, s.index, "sub_mode").as_deref() == Some("click")),
                            _ => false,
                        };
                        if on {
                            out.hands[h].push(action.clone());
                        }
                    }
                }
            }
        }
        for (h, c) in &chords {
            if !input[*h].active || c.inputs.is_empty() {
                continue;
            }
            let all = c.inputs.iter().all(|i| match i.hand {
                Some(ih) => {
                    let r = self.read(ih as usize, &input[ih as usize], &i.input);
                    if i.slot == "touch" { r.touch } else { r.click }
                }
                None => false,
            });
            if all {
                out.hands[*h].push(c.action.clone());
            }
        }
        out
    }
}

/// A d-pad direction from a stick / pad: engaged by a click (`sub_mode: click`)
/// or else by touching / pushing it; the centre is the inner half.
fn dpad(r: &Reading, dir: &str, needs_click: bool) -> bool {
    let Some((x, y)) = r.pos else { return false };
    let mag = (x * x + y * y).sqrt();
    let engaged = if needs_click { r.click } else { r.touch || r.click || mag > 0.5 };
    if !engaged {
        return false;
    }
    let got = if mag < 0.5 {
        "center"
    } else if x.abs() > y.abs() {
        if x > 0.0 { "east" } else { "west" }
    } else if y > 0.0 {
        "north"
    } else {
        "south"
    };
    got == dir
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ev: &mut Evaluator, doc: &BindingDoc, left: HandInput, t: Instant) -> Active {
        ev.eval([doc, doc], [left, HandInput { active: true, ..Default::default() }], [false; 2], t)
    }

    const IDLE: HandInput = HandInput {
        active: true,
        trigger: 0.0,
        grip: 0.0,
        stick: (0.0, 0.0),
        stick_click: false,
        a: false,
        b: false,
        pad: (0.0, 0.0),
        pad_force: 0.0,
        pad_touch: false,
        system: false,
    };

    #[test]
    fn defaults_move_like_before() {
        let idx = default_doc("knuckles");
        let mut ev = Evaluator::default();
        let t = Instant::now();
        assert!(frame(&mut ev, &idx, HandInput { pad_force: 0.9, ..IDLE }, t).has(Hand::Left, MOVE));
        assert!(!frame(&mut ev, &idx, IDLE, t).has(Hand::Left, MOVE));

        // Gloves: A alone does nothing, A + B moves and marks both as chorded.
        let glove = default_doc(GLOVES);
        assert!(!frame(&mut ev, &glove, HandInput { a: true, ..IDLE }, t).has(Hand::Left, MOVE));
        let out = frame(&mut ev, &glove, HandInput { a: true, b: true, ..IDLE }, t);
        assert!(out.has(Hand::Left, MOVE));
        assert_eq!(out.chorded[0], vec!["a".to_string(), "b".to_string()]);

        // Touch: X + Y on the left.
        let touch = default_doc("oculus_touch");
        assert_eq!(touch.chords(PLAYSPACE)[0].inputs.iter().map(|i| i.input.as_str()).collect::<Vec<_>>(), ["x", "y"]);
        assert!(frame(&mut ev, &touch, HandInput { a: true, b: true, ..IDLE }, t).has(Hand::Left, MOVE));
    }

    #[test]
    fn dashboard_and_screens() {
        let t = Instant::now();
        for ty in ["knuckles", "oculus_touch", GLOVES] {
            let d = default_doc(ty);
            assert!(drives(&d, DASHBOARD) && drives(&d, SCREENS), "{ty}");
            let mut ev = Evaluator::default();
            // The left system (menu) button opens the dashboard; the right doesn't.
            assert!(frame(&mut ev, &d, HandInput { system: true, ..IDLE }, t).has(Hand::Left, DASHBOARD));
            assert!(!ev.eval([&d, &d], [IDLE, HandInput { system: true, ..IDLE }], [false; 2], t).any(DASHBOARD));
            // Double B (Y) brings the screens back.
            frame(&mut ev, &d, IDLE, t);
            assert!(!frame(&mut ev, &d, HandInput { b: true, ..IDLE }, t).any(SCREENS));
            frame(&mut ev, &d, IDLE, t);
            assert!(frame(&mut ev, &d, HandInput { b: true, ..IDLE }, t + Duration::from_millis(200)).any(SCREENS), "{ty}");
        }
    }

    #[test]
    fn the_mouse_comes_first_on_a_screen() {
        let t = Instant::now();
        let on_left = |ev: &mut Evaluator, d: &BindingDoc, left: HandInput, at: Instant| ev.eval([d, d], [left, IDLE], [true, false], at);
        for ty in ["knuckles", "oculus_touch", GLOVES] {
            let d = default_doc(ty);
            let mut ev = Evaluator::default();
            // Off a screen the mouse does nothing.
            assert!(!frame(&mut ev, &d, HandInput { trigger: 1.0, a: true, ..IDLE }, t).any(CLICK));
            // On one: trigger clicks, A (X) right-clicks, the stick scrolls.
            let out = on_left(&mut ev, &d, HandInput { trigger: 0.6, a: true, stick: (0.0, 1.0), ..IDLE }, t);
            assert!(out.has(Hand::Left, CLICK) && out.has(Hand::Left, RIGHT_CLICK) && !out.has(Hand::Right, CLICK), "{ty}");
            assert_eq!(out.vector(Hand::Left, SCROLL), (0.0, 1.0));
            assert_eq!(on_left(&mut ev, &d, HandInput { stick: (0.1, 0.1), ..IDLE }, t).vector(Hand::Left, SCROLL), (0.0, 0.0));
            // B is free by default: a double press still hides the screens there.
            on_left(&mut ev, &d, HandInput { b: true, ..IDLE }, t);
            on_left(&mut ev, &d, IDLE, t);
            assert!(on_left(&mut ev, &d, HandInput { b: true, ..IDLE }, t + Duration::from_millis(200)).any(SCREENS), "{ty}");
        }
        // Give B a click: on a screen, its double press is the screen's now.
        let mut d = default_doc("knuckles");
        let i = d.add_source(MOUSE_SET, Hand::Left, "b", Mode::Button, None);
        d.set_output(MOUSE_SET, i, "click", Some(STILL_CLICK), None);
        let mut ev = Evaluator::default();
        assert!(on_left(&mut ev, &d, HandInput { b: true, ..IDLE }, t).has(Hand::Left, STILL_CLICK));
        on_left(&mut ev, &d, IDLE, t);
        assert!(!on_left(&mut ev, &d, HandInput { b: true, ..IDLE }, t + Duration::from_millis(200)).any(SCREENS));
        // Away from the screens it's the double press again.
        frame(&mut ev, &d, HandInput { b: true, ..IDLE }, t);
        frame(&mut ev, &d, IDLE, t);
        assert!(frame(&mut ev, &d, HandInput { b: true, ..IDLE }, t + Duration::from_millis(300)).any(SCREENS));
    }

    #[test]
    fn trackpad_swipes_scroll() {
        let mut d = BindingDoc::empty("knuckles");
        let i = d.add_source(MOUSE_SET, Hand::Left, "trackpad", Mode::Scroll, None);
        d.set_output(MOUSE_SET, i, "scroll", Some(SCROLL), None);
        let mut ev = Evaluator::default();
        let t = Instant::now();
        let pad = |y: f32| HandInput { pad: (0.0, y), pad_touch: true, ..IDLE };
        let wheel = |ev: &mut Evaluator, h: HandInput| ev.eval([&d, &d], [h, IDLE], [true, false], t).vector(Hand::Left, SCROLL).1;
        assert_eq!(wheel(&mut ev, pad(-0.5)), 0.0, "touching down doesn't scroll");
        // The page follows the finger: swiping up scrolls down.
        assert!(wheel(&mut ev, pad(-0.3)) < -4.9);
        assert_eq!(wheel(&mut ev, IDLE), 0.0);
        assert_eq!(wheel(&mut ev, pad(0.4)), 0.0, "a new touch starts over");
    }

    #[test]
    fn chord_buttons_dont_count_alone() {
        // Gloves: A + B drags twice in a row; B mustn't read as a double press.
        let d = default_doc(GLOVES);
        let mut ev = Evaluator::default();
        let t = Instant::now();
        for k in 0..2 {
            let at = t + Duration::from_millis(100 * k);
            frame(&mut ev, &d, HandInput { a: true, ..IDLE }, at);
            let out = frame(&mut ev, &d, HandInput { a: true, b: true, ..IDLE }, at);
            assert!(out.has(Hand::Left, MOVE) && !out.any(SCREENS));
            // A let go first: B still held, still part of the chord.
            assert!(!frame(&mut ev, &d, HandInput { b: true, ..IDLE }, at).any(SCREENS));
            frame(&mut ev, &d, IDLE, at);
        }
    }

    #[test]
    fn missing_sets_and_dashboard_come_back() {
        // A binding saved before the dashboard / screens sets existed.
        let mut old = BindingDoc::empty("knuckles");
        let i = old.add_source(PLAYSPACE, Hand::Left, "grip", Mode::Grab, None);
        old.set_output(PLAYSPACE, i, "grab", Some(MOVE), None);
        fill_missing_sets(&mut old, "knuckles");
        assert!(drives(&old, DASHBOARD) && drives(&old, SCREENS));
        assert_eq!(old.sources(PLAYSPACE).len(), 1);
        // An emptied dashboard set stays empty (the editor shows it, and won't save it).
        let mut cleared = default_doc("knuckles");
        cleared.remove_source(DASHBOARD_SET, 0, None);
        fill_missing_sets(&mut cleared, "knuckles");
        assert!(!drives(&cleared, DASHBOARD));
        cleared.copy_set(&default_doc("knuckles"), DASHBOARD_SET);
        assert!(drives(&cleared, DASHBOARD));
    }

    #[test]
    fn summaries() {
        let idx = controller("knuckles").unwrap();
        assert_eq!(summary(&default_doc("knuckles"), idx, MOVE), "Trackpad on either hand");
        let g = controller(GLOVES).unwrap();
        assert_eq!(summary(&default_doc(GLOVES), g, MOVE), "A button + B button on either hand");
        let t = controller("oculus_touch").unwrap();
        assert_eq!(summary(&default_doc("oculus_touch"), t, MOVE), "Left: X button + Y button · right: A button + B button");
        assert_eq!(summary(&default_doc("knuckles"), idx, SNAP), "Nothing");
        assert_eq!(summary(&default_doc("knuckles"), idx, SCREENS), "B button (double press) on the left hand");
        assert_eq!(mouse_summary(&default_doc("knuckles"), idx), "Trigger: left click · A button: right click · Thumbstick: scroll wheel");
        assert_eq!(mouse_summary(&default_doc("oculus_touch"), t), "Trigger: left click · X button or A button: right click · Thumbstick: scroll wheel");
        let mut d = default_doc("knuckles");
        let i = d.add_source(MOUSE_SET, Hand::Right, "b", Mode::Button, None);
        d.set_output(MOUSE_SET, i, "click", Some(STILL_CLICK), None);
        assert!(mouse_summary(&d, idx).ends_with(" · B button (right): click without moving the cursor · Thumbstick: scroll wheel"));
    }

    #[test]
    fn toggle_double_grab_dpad() {
        let mut d = BindingDoc::empty("knuckles");
        let i = d.add_source(PLAYSPACE, Hand::Left, "a", Mode::ToggleButton, None);
        d.set_output(PLAYSPACE, i, "click", Some(MOVE), None);
        let j = d.add_source(PLAYSPACE, Hand::Left, "b", Mode::Button, None);
        d.set_output(PLAYSPACE, j, "double", Some(SNAP), None);
        let k = d.add_source(PLAYSPACE, Hand::Left, "thumbstick", Mode::Dpad, None);
        d.set_output(PLAYSPACE, k, "south", Some(SNAP), None);
        let mut ev = Evaluator::default();
        let t0 = Instant::now();
        // Toggle: on after a press, stays on, off after the next.
        assert!(frame(&mut ev, &d, HandInput { a: true, ..IDLE }, t0).has(Hand::Left, MOVE));
        assert!(frame(&mut ev, &d, IDLE, t0).has(Hand::Left, MOVE));
        frame(&mut ev, &d, HandInput { a: true, ..IDLE }, t0);
        assert!(!frame(&mut ev, &d, IDLE, t0).has(Hand::Left, MOVE));
        // Double press within 450 ms fires once; a slow one doesn't.
        assert!(!frame(&mut ev, &d, HandInput { b: true, ..IDLE }, t0).has(Hand::Left, SNAP));
        frame(&mut ev, &d, IDLE, t0);
        assert!(frame(&mut ev, &d, HandInput { b: true, ..IDLE }, t0 + Duration::from_millis(200)).has(Hand::Left, SNAP));
        frame(&mut ev, &d, IDLE, t0);
        frame(&mut ev, &d, HandInput { b: true, ..IDLE }, t0 + Duration::from_secs(2));
        frame(&mut ev, &d, IDLE, t0);
        assert!(!frame(&mut ev, &d, HandInput { b: true, ..IDLE }, t0 + Duration::from_secs(4)).has(Hand::Left, SNAP));
        // Stick pulled down.
        assert!(frame(&mut ev, &d, HandInput { stick: (0.1, -0.9), ..IDLE }, t0).has(Hand::Left, SNAP));
        assert!(!frame(&mut ev, &d, HandInput { stick: (0.9, 0.1), ..IDLE }, t0).has(Hand::Left, SNAP));
        // Grab uses the grip with hysteresis.
        let mut g = BindingDoc::empty("knuckles");
        let i = g.add_source(PLAYSPACE, Hand::Left, "grip", Mode::Grab, None);
        g.set_output(PLAYSPACE, i, "grab", Some(MOVE), None);
        assert!(!frame(&mut ev, &g, HandInput { grip: 0.6, ..IDLE }, t0).has(Hand::Left, MOVE));
        assert!(frame(&mut ev, &g, HandInput { grip: 0.8, ..IDLE }, t0).has(Hand::Left, MOVE));
        assert!(frame(&mut ev, &g, HandInput { grip: 0.6, ..IDLE }, t0).has(Hand::Left, MOVE));
        assert!(!frame(&mut ev, &g, HandInput { grip: 0.3, ..IDLE }, t0).has(Hand::Left, MOVE));
    }
}
