//! Controller bindings for SteamVR-input games, the way xrizer reads them.
//!
//! A game ships `actions.json` (the actions it understands, grouped in action
//! sets, with display names) and one binding file per controller type mapping
//! physical inputs to those actions. xrizer loads `<game dir>/xrizer/<type>.json`
//! instead of the shipped file when that exists: a personal binding the game's
//! updates never touch. This module reads both, edits a binding without losing
//! anything it doesn't model (chords, options, parameters, unknown keys), and
//! knows what xrizer accepts: the modes it parses (one it doesn't know makes it
//! drop the WHOLE file) and the inputs each controller has.
//!
//! Action paths are case-insensitive (manifests say `/actions/Global/in/Jump`,
//! binding files `/actions/global/in/jump`), so every lookup lowercases.

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

// --- actions.json ----------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionKind {
    Boolean,
    Vector1,
    Vector2,
    Vector3,
    Vibration,
    Pose,
    Skeleton,
    Other,
}

impl ActionKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Vector1 => "vector1",
            Self::Vector2 => "vector2",
            Self::Vector3 => "vector3",
            Self::Vibration => "vibration",
            Self::Pose => "pose",
            Self::Skeleton => "skeleton",
            Self::Other => "other",
        }
    }

    fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "boolean" => Self::Boolean,
            "vector1" => Self::Vector1,
            "vector2" => Self::Vector2,
            "vector3" => Self::Vector3,
            "vibration" => Self::Vibration,
            "pose" => Self::Pose,
            "skeleton" => Self::Skeleton,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Action {
    /// The path lowercased: what binding files use and what lookups key on.
    pub key: String,
    pub kind: ActionKind,
    pub mandatory: bool,
    /// The game's own name for it, else one made from the path.
    pub name: String,
    /// Its action set's key.
    pub set: String,
}

#[derive(Clone, Debug)]
pub struct ActionSet {
    pub key: String,
    pub name: String,
    /// `usage: hidden` — not meant to be rebound.
    pub hidden: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Manifest {
    pub sets: Vec<ActionSet>,
    pub actions: Vec<Action>,
    /// (controller type, binding file relative to actions.json), in file order.
    pub default_bindings: Vec<(String, String)>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(strip_bom(text)).map_err(|e| format!("actions.json: {e}"))?;
        let loc = pick_localization(&v);
        let name_for = |key: &str| loc.iter().find(|(k, _)| k == key).map(|(_, n)| n.clone());

        let mut sets: Vec<ActionSet> = Vec::new();
        for s in v.get("action_sets").and_then(Value::as_array).into_iter().flatten() {
            let Some(path) = s.get("name").and_then(Value::as_str) else { continue };
            let key = path.to_ascii_lowercase();
            if sets.iter().any(|x| x.key == key) {
                continue;
            }
            let hidden = s.get("usage").and_then(Value::as_str).is_some_and(|u| u.eq_ignore_ascii_case("hidden"));
            sets.push(ActionSet { name: name_for(&key).unwrap_or_else(|| pretty_name(path)), key, hidden });
        }
        let mut actions = Vec::new();
        for a in v.get("actions").and_then(Value::as_array).into_iter().flatten() {
            let Some(path) = a.get("name").and_then(Value::as_str) else { continue };
            let key = path.to_ascii_lowercase();
            let set = set_of(&key);
            // A manifest that forgot to list a set still gets it.
            if !sets.iter().any(|s| s.key == set) {
                sets.push(ActionSet { name: pretty_name(&set), key: set.clone(), hidden: false });
            }
            actions.push(Action {
                kind: ActionKind::parse(a.get("type").and_then(Value::as_str).unwrap_or("")),
                mandatory: a.get("requirement").and_then(Value::as_str).is_some_and(|r| r.eq_ignore_ascii_case("mandatory")),
                name: name_for(&key).unwrap_or_else(|| pretty_name(path)),
                set,
                key,
            });
        }
        let default_bindings = v
            .get("default_bindings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|d| Some((d.get("controller_type")?.as_str()?.to_string(), d.get("binding_url")?.as_str()?.to_string())))
            .collect();
        Ok(Self { sets, actions, default_bindings })
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text)
    }

    pub fn action(&self, path: &str) -> Option<&Action> {
        let key = path.to_ascii_lowercase();
        self.actions.iter().find(|a| a.key == key)
    }

    /// What to call an action: the game's name for it, or one made from the path
    /// (a binding may name an action the manifest no longer has).
    pub fn action_name(&self, path: &str) -> String {
        self.action(path).map_or_else(|| pretty_name(path), |a| a.name.clone())
    }

    pub fn set(&self, key: &str) -> Option<&ActionSet> {
        let key = key.to_ascii_lowercase();
        self.sets.iter().find(|s| s.key == key)
    }

    /// The first binding file shipped for a controller type (xrizer skips the rest).
    pub fn default_binding(&self, controller_type: &str) -> Option<&str> {
        self.default_bindings.iter().find(|(t, _)| t == controller_type).map(|(_, u)| u.as_str())
    }
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

/// The localization table to show: US English, any English, else the first.
/// Keys lowercased.
fn pick_localization(v: &Value) -> Vec<(String, String)> {
    let tables: Vec<&Map<String, Value>> = v.get("localization").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_object).collect();
    let tag = |m: &&Map<String, Value>| m.get("language_tag").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
    let chosen = tables
        .iter()
        .find(|m| tag(m) == "en_us")
        .or_else(|| tables.iter().find(|m| tag(m).starts_with("en")))
        .or(tables.first());
    chosen
        .map(|m| {
            m.iter()
                .filter(|(k, _)| k.as_str() != "language_tag")
                .filter_map(|(k, v)| Some((k.to_ascii_lowercase(), v.as_str()?.trim().to_string())))
                .filter(|(_, v)| !v.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// `/actions/global/in/jump` → `/actions/global`.
pub fn set_of(action: &str) -> String {
    action.to_ascii_lowercase().splitn(4, '/').take(3).collect::<Vec<_>>().join("/")
}

/// A readable name from a path's last segment: `Stick_Click` → "Stick click",
/// `SkeletonLeftHand` → "Skeleton left hand"; all-caps words (HUD) stay.
pub fn pretty_name(path: &str) -> String {
    let last = path.rsplit('/').next().unwrap_or(path);
    let mut spaced = String::new();
    let mut prev: Option<char> = None;
    for c in last.chars() {
        if c == '_' || c == '-' {
            spaced.push(' ');
        } else {
            if c.is_uppercase() && prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit()) {
                spaced.push(' ');
            }
            spaced.push(c);
        }
        prev = Some(c);
    }
    let words: Vec<String> = spaced
        .split_whitespace()
        .enumerate()
        .map(|(i, w)| {
            let caps = w.len() > 1 && w.chars().all(|c| !c.is_lowercase());
            if caps {
                return w.to_string();
            }
            let lower = w.to_lowercase();
            if i == 0 {
                let mut cs = lower.chars();
                cs.next().map(|f| f.to_uppercase().chain(cs).collect()).unwrap_or_default()
            } else {
                lower
            }
        })
        .collect();
    if words.is_empty() {
        last.to_string()
    } else {
        words.join(" ")
    }
}

// --- controllers + modes, as xrizer knows them ------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Hand {
    Left,
    Right,
}

impl Hand {
    pub fn id(self) -> &'static str {
        match self {
            Hand::Left => "left",
            Hand::Right => "right",
        }
    }
    pub fn other(self) -> Hand {
        match self {
            Hand::Left => Hand::Right,
            Hand::Right => Hand::Left,
        }
    }
}

/// How an input is read. xrizer parses exactly these; any other mode in a file
/// (`radial`, `complex_button`, `dpad_touch`…) fails its parse of the whole file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Button,
    ToggleButton,
    Trigger,
    Joystick,
    Trackpad,
    Dpad,
    Scroll,
    ForceSensor,
    Grab,
    ScalarConstant,
    None,
}

/// One thing a mode reports, that an action can be bound to.
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub key: &'static str,
    pub label: &'static str,
    /// The action type this slot drives.
    pub kind: ActionKind,
}

const fn slot(key: &'static str, label: &'static str, kind: ActionKind) -> Slot {
    Slot { key, label, kind }
}

impl Mode {
    pub const ALL: [Mode; 11] = [
        Mode::Button,
        Mode::ToggleButton,
        Mode::Trigger,
        Mode::Joystick,
        Mode::Trackpad,
        Mode::Dpad,
        Mode::Scroll,
        Mode::ForceSensor,
        Mode::Grab,
        Mode::ScalarConstant,
        Mode::None,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Mode::Button => "button",
            Mode::ToggleButton => "toggle_button",
            Mode::Trigger => "trigger",
            Mode::Joystick => "joystick",
            Mode::Trackpad => "trackpad",
            Mode::Dpad => "dpad",
            Mode::Scroll => "scroll",
            Mode::ForceSensor => "force_sensor",
            Mode::Grab => "grab",
            Mode::ScalarConstant => "scalar_constant",
            Mode::None => "none",
        }
    }

    /// A mode xrizer reads (case-sensitive, like its parser).
    pub fn from_id(s: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.id() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Button => "Button",
            Mode::ToggleButton => "Toggle button",
            Mode::Trigger => "Trigger",
            Mode::Joystick => "Joystick",
            Mode::Trackpad => "Trackpad",
            Mode::Dpad => "D-pad",
            Mode::Scroll => "Scroll",
            Mode::ForceSensor => "Force sensor",
            Mode::Grab => "Grab",
            Mode::ScalarConstant => "Constant value",
            Mode::None => "Nothing",
        }
    }

    /// One line on what picking it does.
    pub fn blurb(self) -> &'static str {
        match self {
            Mode::Button => "Press, touch or double press",
            Mode::ToggleButton => "Each press switches on or off",
            Mode::Trigger => "How far it's pulled, plus click and touch",
            Mode::Joystick => "Push in any direction, plus click and touch",
            Mode::Trackpad => "Where your thumb is, plus click and touch",
            Mode::Dpad => "Four directions and the centre, as buttons",
            Mode::Scroll => "Swipe or push to scroll",
            Mode::ForceSensor => "How hard you squeeze",
            Mode::Grab => "Squeeze to grab, relax to let go",
            Mode::ScalarConstant => "Always reports a fixed value",
            Mode::None => "Does nothing",
        }
    }

    pub fn slots(self) -> &'static [Slot] {
        match self {
            Mode::Button => BUTTON_SLOTS,
            Mode::ToggleButton => TOGGLE_SLOTS,
            Mode::Trigger => TRIGGER_SLOTS,
            Mode::Joystick | Mode::Trackpad => STICK_SLOTS,
            Mode::Dpad => DPAD_SLOTS,
            Mode::Scroll => SCROLL_SLOTS,
            Mode::ForceSensor => FORCE_SLOTS,
            Mode::Grab => GRAB_SLOTS,
            Mode::ScalarConstant => VALUE_SLOTS,
            Mode::None => &[],
        }
    }
}

use ActionKind::{Boolean, Vector1, Vector2};
const CLICK: Slot = slot("click", "Click", Boolean);
const TOUCH: Slot = slot("touch", "Touch", Boolean);
const BUTTON_SLOTS: &[Slot] = &[CLICK, TOUCH, slot("double", "Double press", Boolean)];
const TOGGLE_SLOTS: &[Slot] = &[CLICK, TOUCH];
const TRIGGER_SLOTS: &[Slot] = &[slot("pull", "Pull", Vector1), CLICK, TOUCH];
const STICK_SLOTS: &[Slot] = &[slot("position", "Position", Vector2), CLICK, TOUCH];
const DPAD_SLOTS: &[Slot] = &[
    slot("north", "Up", Boolean),
    slot("south", "Down", Boolean),
    slot("west", "Left", Boolean),
    slot("east", "Right", Boolean),
    slot("center", "Centre", Boolean),
];
const SCROLL_SLOTS: &[Slot] = &[slot("scroll", "Scroll", Vector2)];
const FORCE_SLOTS: &[Slot] = &[slot("force", "Force", Vector1)];
const GRAB_SLOTS: &[Slot] = &[slot("grab", "Grab", Boolean)];
const VALUE_SLOTS: &[Slot] = &[slot("value", "Value", Vector1)];

/// Which hand(s) have an input.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Both,
    Left,
    Right,
}

#[derive(Debug)]
pub struct InputDef {
    /// The path segment after `/input/` (`trigger`, `a`, `application_menu`).
    pub id: &'static str,
    pub label: &'static str,
    pub side: Side,
    /// Modes offered when adding, most useful first. Empty = can't be bound
    /// (reserved by the runtime, or xrizer doesn't read it); see `note`.
    pub modes: &'static [Mode],
    /// It senses touch (so the Touch slots mean something).
    pub touch: bool,
    /// Why it can't be bound, when `modes` is empty.
    pub note: &'static str,
}

impl InputDef {
    pub fn on(&self, hand: Hand) -> bool {
        match self.side {
            Side::Both => true,
            Side::Left => hand == Hand::Left,
            Side::Right => hand == Hand::Right,
        }
    }

    pub fn bindable(&self) -> bool {
        !self.modes.is_empty()
    }

    /// Whether `slot` of a mode means anything on this input.
    pub fn has_slot(&self, slot: &Slot) -> bool {
        slot.key != "touch" || self.touch
    }
}

#[derive(Debug)]
pub struct Controller {
    /// The `controller_type` in actions.json / binding files.
    pub ty: &'static str,
    pub name: &'static str,
    /// xrizer's personal-binding file name (its enum's Debug name, lowercased:
    /// `OculusTouch` → `oculustouch.json`, not `oculus_touch.json`).
    pub xrizer_file: &'static str,
    pub inputs: &'static [InputDef],
    /// (left id, right id) pairs mirror mode swaps; other inputs keep their id.
    pub mirror: &'static [(&'static str, &'static str)],
}

use Mode as M;

const RESERVED: &str = "Reserved for the dashboard";

const INDEX_INPUTS: &[InputDef] = &[
    InputDef { id: "trigger", label: "Trigger", side: Side::Both, modes: &[M::Trigger, M::Button, M::ToggleButton], touch: true, note: "" },
    InputDef { id: "trackpad", label: "Trackpad", side: Side::Both, modes: &[M::Trackpad, M::Dpad, M::Button, M::ToggleButton, M::Scroll], touch: true, note: "" },
    InputDef { id: "thumbstick", label: "Thumbstick", side: Side::Both, modes: &[M::Joystick, M::Dpad, M::Button, M::ToggleButton, M::Scroll], touch: true, note: "" },
    InputDef { id: "a", label: "A button", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: true, note: "" },
    InputDef { id: "b", label: "B button", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: true, note: "" },
    InputDef { id: "grip", label: "Grip", side: Side::Both, modes: &[M::Grab, M::ForceSensor, M::Trigger, M::Button, M::ToggleButton], touch: true, note: "" },
    InputDef { id: "system", label: "System button", side: Side::Both, modes: &[], touch: false, note: RESERVED },
];

const TOUCH_INPUTS: &[InputDef] = &[
    InputDef { id: "trigger", label: "Trigger", side: Side::Both, modes: &[M::Trigger, M::Button, M::ToggleButton], touch: true, note: "" },
    InputDef { id: "joystick", label: "Thumbstick", side: Side::Both, modes: &[M::Joystick, M::Dpad, M::Button, M::ToggleButton, M::Scroll], touch: true, note: "" },
    InputDef { id: "x", label: "X button", side: Side::Left, modes: &[M::Button, M::ToggleButton], touch: true, note: "" },
    InputDef { id: "y", label: "Y button", side: Side::Left, modes: &[M::Button, M::ToggleButton], touch: true, note: "" },
    InputDef { id: "a", label: "A button", side: Side::Right, modes: &[M::Button, M::ToggleButton], touch: true, note: "" },
    InputDef { id: "b", label: "B button", side: Side::Right, modes: &[M::Button, M::ToggleButton], touch: true, note: "" },
    InputDef { id: "grip", label: "Grip", side: Side::Both, modes: &[M::Trigger, M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "thumbrest", label: "Thumb rest", side: Side::Both, modes: &[M::Button], touch: true, note: "" },
    InputDef { id: "application_menu", label: "Menu button", side: Side::Left, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "system", label: "System button", side: Side::Right, modes: &[], touch: false, note: RESERVED },
];

const VIVE_INPUTS: &[InputDef] = &[
    InputDef { id: "trigger", label: "Trigger", side: Side::Both, modes: &[M::Trigger, M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "trackpad", label: "Trackpad", side: Side::Both, modes: &[M::Trackpad, M::Dpad, M::Button, M::ToggleButton, M::Scroll], touch: true, note: "" },
    InputDef { id: "grip", label: "Grip", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "application_menu", label: "Menu button", side: Side::Both, modes: &[M::Button, M::ToggleButton], touch: false, note: "" },
    InputDef { id: "system", label: "System button", side: Side::Both, modes: &[], touch: false, note: RESERVED },
];

pub const CONTROLLERS: &[Controller] = &[
    Controller { ty: "knuckles", name: "Index controllers", xrizer_file: "knuckles.json", inputs: INDEX_INPUTS, mirror: &[] },
    Controller { ty: "oculus_touch", name: "Touch controllers", xrizer_file: "oculustouch.json", inputs: TOUCH_INPUTS, mirror: &[("x", "a"), ("y", "b")] },
    Controller { ty: "vive_controller", name: "Vive wands", xrizer_file: "vivecontroller.json", inputs: VIVE_INPUTS, mirror: &[] },
];

pub fn controller(ty: &str) -> Option<&'static Controller> {
    CONTROLLERS.iter().find(|c| c.ty == ty)
}

impl Controller {
    pub fn input(&self, id: &str) -> Option<&'static InputDef> {
        self.inputs.iter().find(|i| i.id == id)
    }

    /// The input on the other hand that mirrors `id` on `hand` (X ↔ A on Touch).
    pub fn mirror_input<'a>(&self, hand: Hand, id: &'a str) -> &'a str {
        for &(l, r) in self.mirror {
            if hand == Hand::Left && id == l {
                return r;
            }
            if hand == Hand::Right && id == r {
                return l;
            }
        }
        id
    }
}

/// The drawing a controller type is shown with (`index`, `touch`, `vive`).
pub fn art(ty: &str) -> Option<&'static str> {
    match ty {
        "knuckles" | own::GLOVES => Some("index"),
        "oculus_touch" => Some("touch"),
        "vive_controller" => Some("vive"),
        _ => None,
    }
}

/// Where each input sits on the drawing of a RIGHT controller: (input, x, y,
/// radius), as fractions of the drawing (x and radius of its width, y of its
/// height). Left-hand drawings are the mirror.
pub fn spots(ty: &str) -> &'static [(&'static str, f32, f32, f32)] {
    match art(ty) {
        Some("index") => &[
            ("thumbstick", 0.311, 0.178, 0.055),
            ("trackpad", 0.253, 0.244, 0.05),
            ("b", 0.170, 0.262, 0.035),
            ("a", 0.262, 0.297, 0.035),
            ("system", 0.338, 0.317, 0.025),
            ("trigger", 0.09, 0.40, 0.055),
            ("grip", 0.47, 0.58, 0.07),
        ],
        Some("touch") => &[
            ("joystick", 0.34, 0.23, 0.07),
            ("trigger", 0.19, 0.55, 0.06),
            ("grip", 0.65, 0.57, 0.07),
            ("a", 0.49, 0.335, 0.045),
            ("b", 0.38, 0.37, 0.04),
            ("x", 0.49, 0.335, 0.045),
            ("y", 0.38, 0.37, 0.04),
            ("system", 0.525, 0.26, 0.03),
            ("application_menu", 0.525, 0.26, 0.03),
            ("thumbrest", 0.51, 0.43, 0.05),
        ],
        Some("vive") => &[
            ("trackpad", 0.86, 0.38, 0.1),
            ("trigger", 0.52, 0.42, 0.07),
            ("grip", 0.66, 0.575, 0.06),
            ("application_menu", 0.82, 0.235, 0.045),
            ("system", 0.87, 0.54, 0.04),
        ],
        _ => &[],
    }
}

/// xrizer's personal-binding file name for a controller type, `None` for types
/// it doesn't read personal bindings for.
pub fn xrizer_file(ty: &str) -> Option<&'static str> {
    match ty {
        "vive_focus3_controller" => Some("vivefocus3.json"),
        _ => controller(ty).map(|c| c.xrizer_file),
    }
}

/// The controller type a file in `<game>/xrizer/` is for: xrizer's own names,
/// plus the `bindings_<type>.json` / `<type>.json` spellings older Monadeck wrote.
pub fn type_from_xrizer_file(file_name: &str) -> String {
    let stem = file_name.strip_suffix(".json").unwrap_or(file_name);
    let stem = stem.strip_prefix("bindings_").or_else(|| stem.strip_prefix("binding_")).unwrap_or(stem);
    if let Some(c) = CONTROLLERS.iter().find(|c| c.xrizer_file.strip_suffix(".json") == Some(stem)) {
        return c.ty.to_string();
    }
    if stem == "vivefocus3" {
        return "vive_focus3_controller".into();
    }
    stem.to_string()
}

// --- a binding file ---------------------------------------------------------------------------

/// One `sources` entry: an input used in a mode, with actions on its slots.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    /// Its position in the set's `sources`.
    pub index: usize,
    pub hand: Option<Hand>,
    /// The segment after `/input/`, or the whole path when it isn't a hand input.
    pub input: String,
    pub mode: String,
    /// (slot key, action path) as written.
    pub outputs: Vec<(String, String)>,
}

impl Source {
    pub fn output(&self, slot: &str) -> Option<&str> {
        self.outputs.iter().find(|(k, _)| k == slot).map(|(_, a)| a.as_str())
    }
}

/// One input of a chord.
#[derive(Clone, Debug, PartialEq)]
pub struct ChordInput {
    pub hand: Option<Hand>,
    pub input: String,
    pub slot: String,
}

/// A `chords` entry: `action` while every input is held.
#[derive(Clone, Debug, PartialEq)]
pub struct Chord {
    pub index: usize,
    pub action: String,
    pub inputs: Vec<ChordInput>,
}

/// A pose action bound to a hand (`/user/hand/left/pose/raw`).
#[derive(Clone, Debug, PartialEq)]
pub struct PoseBinding {
    pub action: String,
    pub hand: Hand,
    /// raw / tip / base / handgrip / gdc2015
    pub point: String,
}

/// The pose points a pose action can follow; xrizer reads raw, tip and gdc2015
/// (anything else as raw).
pub const POSE_POINTS: &[(&str, &str)] = &[("raw", "Raw"), ("tip", "Tip"), ("gdc2015", "GDC 2015")];

/// A binding file, kept as its JSON so saving never drops what isn't modelled.
#[derive(Clone, Debug, PartialEq)]
pub struct BindingDoc {
    root: Value,
}

fn hand_input_path(hand: Hand, input: &str) -> String {
    format!("/user/hand/{}/input/{input}", hand.id())
}

fn parse_source_path(path: &str) -> (Option<Hand>, String) {
    let lower = path.to_ascii_lowercase();
    for hand in [Hand::Left, Hand::Right] {
        let prefix = format!("/user/hand/{}/input/", hand.id());
        if let Some(rest) = lower.strip_prefix(&prefix) {
            return (Some(hand), rest.to_string());
        }
    }
    (None, path.to_string())
}

impl BindingDoc {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut root: Value = serde_json::from_str(strip_bom(text)).map_err(|e| format!("binding file: {e}"))?;
        let obj = root.as_object_mut().ok_or("binding file: not a JSON object")?;
        if !obj.get("bindings").is_some_and(Value::is_object) {
            obj.insert("bindings".into(), json!({}));
        }
        Ok(Self { root })
    }

    /// From JSON already parsed (the desktop editor sends the document back
    /// and forth).
    pub fn from_value(mut root: Value) -> Result<Self, String> {
        let obj = root.as_object_mut().ok_or("binding file: not a JSON object")?;
        if !obj.get("bindings").is_some_and(Value::is_object) {
            obj.insert("bindings".into(), json!({}));
        }
        Ok(Self { root })
    }

    pub fn value(&self) -> &Value {
        &self.root
    }

    /// A blank binding for a controller type (the game shipped none for it).
    pub fn empty(controller_type: &str) -> Self {
        Self { root: json!({ "bindings": {}, "controller_type": controller_type, "description": "", "name": "" }) }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.root).unwrap_or_default()
    }

    pub fn controller_type(&self) -> Option<&str> {
        self.root.get("controller_type").and_then(Value::as_str)
    }

    pub fn name(&self) -> Option<&str> {
        self.root.get("name").and_then(Value::as_str).filter(|s| !s.trim().is_empty())
    }

    fn bindings(&self) -> &Map<String, Value> {
        self.root["bindings"].as_object().expect("bindings is an object (parse ensures it)")
    }

    fn bindings_mut(&mut self) -> &mut Map<String, Value> {
        self.root["bindings"].as_object_mut().expect("bindings is an object (parse ensures it)")
    }

    /// The set keys present in the file, lowercased.
    pub fn set_keys(&self) -> Vec<String> {
        self.bindings().keys().map(|k| k.to_ascii_lowercase()).collect()
    }

    fn set_obj(&self, set: &str) -> Option<&Map<String, Value>> {
        self.bindings().iter().find(|(k, _)| k.eq_ignore_ascii_case(set)).and_then(|(_, v)| v.as_object())
    }

    /// The set's object, created (under the lowercased key) when missing.
    fn set_obj_mut(&mut self, set: &str) -> &mut Map<String, Value> {
        let b = self.bindings_mut();
        let key = b.keys().find(|k| k.eq_ignore_ascii_case(set)).cloned().unwrap_or_else(|| set.to_ascii_lowercase());
        let entry = b.entry(key).or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        entry.as_object_mut().unwrap()
    }

    fn list_mut<'a>(obj: &'a mut Map<String, Value>, name: &str) -> &'a mut Vec<Value> {
        let entry = obj.entry(name.to_string()).or_insert_with(|| json!([]));
        if !entry.is_array() {
            *entry = json!([]);
        }
        entry.as_array_mut().unwrap()
    }

    fn sources_mut(&mut self, set: &str) -> &mut Vec<Value> {
        Self::list_mut(self.set_obj_mut(set), "sources")
    }

    pub fn sources(&self, set: &str) -> Vec<Source> {
        let Some(list) = self.set_obj(set).and_then(|s| s.get("sources")).and_then(Value::as_array) else {
            return Vec::new();
        };
        list.iter()
            .enumerate()
            .filter_map(|(index, s)| {
                let path = s.get("path")?.as_str()?;
                let (hand, input) = parse_source_path(path);
                let mode = s.get("mode").and_then(Value::as_str).unwrap_or("").to_string();
                let outputs = s
                    .get("inputs")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                    .filter_map(|(k, v)| Some((k.clone(), v.get("output")?.as_str()?.to_string())))
                    .filter(|(_, a)| !a.is_empty())
                    .collect();
                Some(Source { index, hand, input, mode, outputs })
            })
            .collect()
    }

    /// How many inputs (and chords) of a set do something (a tab's badge).
    pub fn bound_count(&self, set: &str) -> usize {
        self.sources(set).iter().filter(|s| !s.outputs.is_empty()).count() + self.chords(set).iter().filter(|c| !c.inputs.is_empty()).count()
    }

    /// The source on `hand`'s `input` in `mode`, other than `skip`.
    fn find(&self, set: &str, hand: Hand, input: &str, mode: &str, skip: Option<usize>) -> Option<usize> {
        self.sources(set).into_iter().find(|s| s.hand == Some(hand) && s.input == input && s.mode == mode && Some(s.index) != skip).map(|s| s.index)
    }

    /// Where mirror mode lands for a source: the same mode on the other hand's
    /// mirrored input, `None` when that hand lacks the input.
    fn mirror_target(ctrl: &Controller, hand: Hand, input: &str) -> Option<(Hand, String)> {
        let other = hand.other();
        let id = ctrl.mirror_input(hand, input);
        ctrl.input(id).filter(|d| d.on(other)).map(|_| (other, id.to_string()))
    }

    /// Use `input` on `hand` in `mode` (a new, empty card). With `mirror`, the
    /// other hand gets the same card unless it has one in that mode already.
    /// Returns the new source's index.
    pub fn add_source(&mut self, set: &str, hand: Hand, input: &str, mode: Mode, mirror: Option<&Controller>) -> usize {
        let list = self.sources_mut(set);
        list.push(json!({ "path": hand_input_path(hand, input), "mode": mode.id(), "inputs": {} }));
        let index = list.len() - 1;
        if let Some((mh, mi)) = mirror.and_then(|c| Self::mirror_target(c, hand, input)) {
            if self.find(set, mh, &mi, mode.id(), None).is_none() {
                self.sources_mut(set).push(json!({ "path": hand_input_path(mh, &mi), "mode": mode.id(), "inputs": {} }));
            }
        }
        index
    }

    /// The mirrored source for `index`, created (empty) when missing.
    fn mirror_index(&mut self, set: &str, index: usize, ctrl: &Controller) -> Option<usize> {
        let src = self.sources(set).into_iter().find(|s| s.index == index)?;
        let (mh, mi) = Self::mirror_target(ctrl, src.hand?, &src.input)?;
        if let Some(i) = self.find(set, mh, &mi, &src.mode, Some(index)) {
            return Some(i);
        }
        let list = self.sources_mut(set);
        list.push(json!({ "path": hand_input_path(mh, &mi), "mode": src.mode, "inputs": {} }));
        Some(list.len() - 1)
    }

    fn put_output(&mut self, set: &str, index: usize, slot: &str, action: Option<&str>) {
        let Some(src) = self.sources_mut(set).get_mut(index).and_then(Value::as_object_mut) else { return };
        let inputs = src.entry("inputs").or_insert_with(|| json!({}));
        if !inputs.is_object() {
            *inputs = json!({});
        }
        let inputs = inputs.as_object_mut().unwrap();
        match action {
            Some(a) => {
                inputs.insert(slot.to_string(), json!({ "output": a.to_ascii_lowercase() }));
            }
            None => {
                inputs.remove(slot);
            }
        }
    }

    /// A source's parameter (`sub_mode`, `click_activate_threshold`…), as text.
    pub fn parameter(&self, set: &str, index: usize, key: &str) -> Option<String> {
        let p = self.set_obj(set)?.get("sources")?.as_array()?.get(index)?.get("parameters")?.get(key)?;
        p.as_str().map(str::to_string).or_else(|| (!p.is_null()).then(|| p.to_string()))
    }

    /// Bind (or with `None` unbind) one slot of a source, and its mirror.
    pub fn set_output(&mut self, set: &str, index: usize, slot: &str, action: Option<&str>, mirror: Option<&Controller>) {
        if let Some(mi) = mirror.and_then(|c| self.mirror_index(set, index, c)) {
            self.put_output(set, mi, slot, action);
        }
        self.put_output(set, index, slot, action);
    }

    /// Read the source another way: slots the new mode also has keep their
    /// action, the rest (and the old mode's parameters) go. Mirrored too.
    pub fn change_mode(&mut self, set: &str, index: usize, mode: Mode, mirror: Option<&Controller>) {
        let mi = mirror.and_then(|c| self.mirror_index(set, index, c));
        for i in [Some(index), mi].into_iter().flatten() {
            let Some(src) = self.sources_mut(set).get_mut(i).and_then(Value::as_object_mut) else { continue };
            src.insert("mode".into(), json!(mode.id()));
            src.remove("parameters");
            if let Some(inputs) = src.get_mut("inputs").and_then(Value::as_object_mut) {
                inputs.retain(|k, _| mode.slots().iter().any(|s| s.key == k));
            }
        }
    }

    /// Drop a source (and its mirror, when it has one).
    pub fn remove_source(&mut self, set: &str, index: usize, mirror: Option<&Controller>) {
        let src = self.sources(set).into_iter().find(|s| s.index == index);
        let mi = match (mirror, &src) {
            (Some(c), Some(s)) => s.hand.and_then(|h| Self::mirror_target(c, h, &s.input)).and_then(|(mh, mi)| self.find(set, mh, &mi, &s.mode, Some(index))),
            _ => None,
        };
        let list = self.sources_mut(set);
        let mut gone: Vec<usize> = [Some(index), mi].into_iter().flatten().filter(|&i| i < list.len()).collect();
        gone.sort_unstable_by(|a, b| b.cmp(a));
        for i in gone {
            list.remove(i);
        }
    }

    /// The vibration action each hand plays, per set.
    pub fn haptic(&self, set: &str, hand: Hand) -> Option<String> {
        let path = format!("/user/hand/{}/output/haptic", hand.id());
        self.set_obj(set)?
            .get("haptics")?
            .as_array()?
            .iter()
            .find(|h| h.get("path").and_then(Value::as_str).is_some_and(|p| p.eq_ignore_ascii_case(&path)))
            .and_then(|h| h.get("output")?.as_str().map(str::to_string))
    }

    pub fn set_haptic(&mut self, set: &str, hand: Hand, action: Option<&str>, mirror: bool) {
        let hands: &[Hand] = if mirror { &[Hand::Left, Hand::Right] } else { std::slice::from_ref(&hand) };
        for &h in hands {
            let path = format!("/user/hand/{}/output/haptic", h.id());
            let list = Self::list_mut(self.set_obj_mut(set), "haptics");
            list.retain(|e| !e.get("path").and_then(Value::as_str).is_some_and(|p| p.eq_ignore_ascii_case(&path)));
            if let Some(a) = action {
                list.push(json!({ "output": a.to_ascii_lowercase(), "path": path }));
            }
        }
    }

    pub fn poses(&self, set: &str) -> Vec<PoseBinding> {
        let Some(list) = self.set_obj(set).and_then(|s| s.get("poses")).and_then(Value::as_array) else {
            return Vec::new();
        };
        list.iter()
            .filter_map(|p| {
                let action = p.get("output")?.as_str()?.to_string();
                let path = p.get("path")?.as_str()?.to_ascii_lowercase();
                let (h, point) = path.strip_prefix("/user/hand/")?.split_once("/pose/")?;
                let hand = match h {
                    "left" => Hand::Left,
                    "right" => Hand::Right,
                    _ => return None,
                };
                Some(PoseBinding { action, hand, point: point.to_string() })
            })
            .collect()
    }

    /// Point a pose action at a hand's pose (`None` unbinds it from that hand).
    pub fn set_pose(&mut self, set: &str, action: &str, hand: Hand, point: Option<&str>, mirror: bool) {
        let hands: &[Hand] = if mirror { &[Hand::Left, Hand::Right] } else { std::slice::from_ref(&hand) };
        for &h in hands {
            let prefix = format!("/user/hand/{}/pose/", h.id());
            let list = Self::list_mut(self.set_obj_mut(set), "poses");
            list.retain(|e| {
                let same_action = e.get("output").and_then(Value::as_str).is_some_and(|o| o.eq_ignore_ascii_case(action));
                let same_hand = e.get("path").and_then(Value::as_str).is_some_and(|p| p.to_ascii_lowercase().starts_with(&prefix));
                !(same_action && same_hand)
            });
            if let Some(pt) = point {
                list.push(json!({ "output": action.to_ascii_lowercase(), "path": format!("{prefix}{pt}") }));
            }
        }
    }

    /// Replace this binding's `set` with `from`'s (dropped if `from` lacks it).
    pub fn copy_set(&mut self, from: &BindingDoc, set: &str) {
        let key = set.to_ascii_lowercase();
        let b = self.bindings_mut();
        b.retain(|k, _| !k.eq_ignore_ascii_case(&key));
        if let Some(v) = from.set_obj(set) {
            b.insert(key, Value::Object(v.clone()));
        }
    }

    /// The set's chords: an action driven by several inputs held together.
    /// (xrizer ignores them; Monadeck's own bindings use them.)
    pub fn chords(&self, set: &str) -> Vec<Chord> {
        let Some(list) = self.set_obj(set).and_then(|s| s.get("chords")).and_then(Value::as_array) else {
            return Vec::new();
        };
        list.iter()
            .enumerate()
            .filter_map(|(index, c)| {
                let action = c.get("output")?.as_str()?.to_string();
                let inputs = c
                    .get("inputs")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|pair| {
                        let pair = pair.as_array()?;
                        let (hand, input) = parse_source_path(pair.first()?.as_str()?);
                        Some(ChordInput { hand, input, slot: pair.get(1)?.as_str()?.to_string() })
                    })
                    .collect();
                Some(Chord { index, action, inputs })
            })
            .collect()
    }

    /// A new chord for `action`, with no inputs yet. Returns its index.
    pub fn add_chord(&mut self, set: &str, action: &str) -> usize {
        let list = Self::list_mut(self.set_obj_mut(set), "chords");
        list.push(json!({ "output": action.to_ascii_lowercase(), "inputs": [] }));
        list.len() - 1
    }

    pub fn set_chord_action(&mut self, set: &str, index: usize, action: &str) {
        if let Some(c) = Self::list_mut(self.set_obj_mut(set), "chords").get_mut(index).and_then(Value::as_object_mut) {
            c.insert("output".into(), json!(action.to_ascii_lowercase()));
        }
    }

    /// Add `hand`'s `input` (its `slot`) to a chord, or take it out if it's in.
    pub fn toggle_chord_input(&mut self, set: &str, index: usize, hand: Hand, input: &str, slot: &str) {
        let path = hand_input_path(hand, input);
        let Some(c) = Self::list_mut(self.set_obj_mut(set), "chords").get_mut(index).and_then(Value::as_object_mut) else { return };
        let inputs = c.entry("inputs").or_insert_with(|| json!([]));
        if !inputs.is_array() {
            *inputs = json!([]);
        }
        let inputs = inputs.as_array_mut().unwrap();
        let before = inputs.len();
        inputs.retain(|p| !p.get(0).and_then(Value::as_str).is_some_and(|x| x.eq_ignore_ascii_case(&path)));
        if inputs.len() == before {
            inputs.push(json!([path, slot]));
        }
    }

    pub fn remove_chord(&mut self, set: &str, index: usize) {
        let list = Self::list_mut(self.set_obj_mut(set), "chords");
        if index < list.len() {
            list.remove(index);
        }
    }

    /// Modes in the file xrizer can't parse — each one makes it ignore the
    /// whole file. (set key, source index, mode)
    pub fn unreadable_modes(&self) -> Vec<(String, usize, String)> {
        let mut out = Vec::new();
        for set in self.set_keys() {
            for s in self.sources(&set) {
                if Mode::from_id(&s.mode).is_none() {
                    out.push((set.clone(), s.index, s.mode.clone()));
                }
            }
        }
        out
    }

    /// Remove every source xrizer can't parse. Returns how many went.
    pub fn drop_unreadable_modes(&mut self) -> usize {
        let mut gone = self.unreadable_modes();
        gone.sort_by(|a, b| b.1.cmp(&a.1));
        let n = gone.len();
        for (set, index, _) in gone {
            let list = self.sources_mut(&set);
            if index < list.len() {
                list.remove(index);
            }
        }
        n
    }
}

pub mod own;

// --- files: shipped default vs personal binding -----------------------------------------------

/// `<game dir>/xrizer/<xrizer file>`: where xrizer looks for a personal binding
/// (it resolves `xrizer/` against the game's working directory, which Steam
/// sets to the install folder).
pub fn personal_path(game_dir: &Path, ty: &str) -> Option<PathBuf> {
    xrizer_file(ty).map(|f| game_dir.join("xrizer").join(f))
}

/// A binding opened for editing.
#[derive(Clone, Debug)]
pub struct Opened {
    pub manifest: Manifest,
    pub doc: BindingDoc,
    /// The doc came from the personal binding (else the game's default).
    pub personal: bool,
    pub default_path: Option<PathBuf>,
    pub personal_path: Option<PathBuf>,
}

/// Open a controller's binding: the personal one if it exists (in the first of
/// the game's folders that has it), else the one the game ships, else a blank
/// one (the manifest lists the type but the file's gone).
pub fn open(actions_path: &Path, dirs: &[PathBuf], ty: &str) -> Result<Opened, String> {
    let manifest = Manifest::load(actions_path)?;
    let default_path = manifest.default_binding(ty).and_then(|url| Some(actions_path.parent()?.join(url)));
    let personal_path = dirs.iter().filter_map(|d| personal_path(d, ty)).find(|p| p.is_file()).or_else(|| dirs.first().and_then(|d| personal_path(d, ty)));
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display())).and_then(|t| BindingDoc::parse(&t));
    let (doc, personal) = match personal_path.as_deref().filter(|p| p.is_file()) {
        Some(p) => (read(p)?, true),
        None => match default_path.as_deref().filter(|p| p.is_file()) {
            Some(p) => (read(p)?, false),
            None => (BindingDoc::empty(ty), false),
        },
    };
    Ok(Opened { manifest, doc, personal, default_path, personal_path })
}

/// Write the personal binding into each of the game's folders (the first must
/// take it; the others are best effort). Returns where the first went.
pub fn save_personal(dirs: &[PathBuf], ty: &str, doc: &BindingDoc) -> Result<PathBuf, String> {
    let write = |dir: &Path| -> Result<PathBuf, String> {
        let path = personal_path(dir, ty).ok_or_else(|| format!("xrizer has no personal bindings for {ty}"))?;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::write(&path, doc.to_json()).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    };
    let (first, rest) = dirs.split_first().ok_or("no folder to save in")?;
    let path = write(first)?;
    for d in rest {
        if let Err(e) = write(d) {
            log::warn!("bindings: extra copy not written: {e}");
        }
    }
    Ok(path)
}

/// Go back to the game's own binding: each personal file is kept aside as
/// `<file>.bak` (xrizer never reads that). Returns false if there was none.
pub fn reset_personal(dirs: &[PathBuf], ty: &str) -> Result<bool, String> {
    let mut any = false;
    for path in dirs.iter().filter_map(|d| personal_path(d, ty)).filter(|p| p.is_file()) {
        let mut bak = path.clone().into_os_string();
        bak.push(".bak");
        std::fs::rename(&path, &bak).map_err(|e| format!("{}: {e}", path.display()))?;
        any = true;
    }
    Ok(any)
}

// --- the games that have bindings -------------------------------------------------------------

/// Text only an xrizer that reloads personal bindings while a game runs has in
/// its library (its log lines about it). Monadeck's fork does; upstream reads
/// them when a game starts. A build without it just reads as "next start".
const LIVE_RELOAD_MARKER: &[u8] = b"Personal bindings changed";

/// Whether the xrizer games run on picks up a saved binding right away.
/// Reads its library (a few MB) once per file version: off the UI thread.
pub fn xrizer_reloads_live() -> bool {
    use crate::config::{MonadeckConfig, OvrRuntime};
    let cfg = MonadeckConfig::load();
    if cfg.ovr_runtime != OvrRuntime::Xrizer {
        return false;
    }
    cfg.xrizer_path
        .or_else(crate::launch_options::detect_xrizer_path)
        .is_some_and(|dir| library_has(&dir.join("bin/linux64/vrclient.so"), LIVE_RELOAD_MARKER))
}

/// Whether `lib` contains `marker`, remembered per file and modification time.
fn library_has(lib: &Path, marker: &[u8]) -> bool {
    static SEEN: std::sync::Mutex<Option<(PathBuf, std::time::SystemTime, bool)>> = std::sync::Mutex::new(None);
    let Ok(modified) = std::fs::metadata(lib).and_then(|m| m.modified()) else {
        return false;
    };
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((p, t, has)) = seen.as_ref() {
        if p == lib && *t == modified {
            return *has;
        }
    }
    let has = std::fs::read(lib).is_ok_and(|data| data.windows(marker.len()).any(|w| w == marker));
    *seen = Some((lib.to_path_buf(), modified, has));
    has
}

/// A game whose controls can be edited: it ships an actions.json with bindings
/// for at least one controller xrizer reads.
#[derive(Clone, Debug)]
pub struct BindableGame {
    pub name: String,
    pub app_id: Option<String>,
    pub shortcut_id: Option<String>,
    /// Where personal bindings go: the executable's folder, then the install
    /// folder when that differs (xrizer reads the one the game runs from).
    pub dirs: Vec<PathBuf>,
    pub actions_path: PathBuf,
    /// Editable controller types it ships bindings for, in [`CONTROLLERS`] order.
    pub controllers: Vec<&'static str>,
    /// Of those, the ones with a personal binding.
    pub personal: Vec<&'static str>,
    pub last_played: Option<u64>,
}

/// Every game with editable bindings (walks the Steam libraries: run it off the
/// UI thread).
pub fn bindable_games() -> Vec<BindableGame> {
    let mut out: Vec<BindableGame> = Vec::new();
    for g in crate::steam::scan_games() {
        let actions_path = PathBuf::from(&g.actions_path);
        let dirs: Vec<PathBuf> = std::iter::once(PathBuf::from(&g.game_path)).chain(g.root_dir.as_ref().map(PathBuf::from)).collect();
        if out.iter().any(|o| o.actions_path == actions_path) {
            continue;
        }
        let Ok(manifest) = Manifest::load(&actions_path) else { continue };
        let controllers: Vec<&'static str> = CONTROLLERS.iter().map(|c| c.ty).filter(|t| manifest.default_binding(t).is_some()).collect();
        if controllers.is_empty() {
            continue;
        }
        let personal = controllers.iter().copied().filter(|t| dirs.iter().any(|d| personal_path(d, t).is_some_and(|p| p.is_file()))).collect();
        out.push(BindableGame {
            name: g.name,
            app_id: g.app_id,
            shortcut_id: g.shortcut_id,
            dirs,
            actions_path,
            controllers,
            personal,
            last_played: g.last_played,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "\u{feff}{
      \"action_sets\": [ { \"name\": \"/actions/Global\", \"usage\": \"leftright\" }, { \"name\": \"/actions/Debug\", \"usage\": \"hidden\" } ],
      \"actions\": [
        { \"name\": \"/actions/Global/in/Jump\", \"type\": \"boolean\", \"requirement\": \"mandatory\" },
        { \"name\": \"/actions/Global/in/Move\", \"type\": \"vector2\" },
        { \"name\": \"/actions/Global/in/Grip_Axis\", \"type\": \"vector1\" },
        { \"name\": \"/actions/Global/out/Haptic\", \"type\": \"vibration\" },
        { \"name\": \"/actions/Global/in/SkeletonLeftHand\", \"type\": \"skeleton\", \"skeleton\": \"/skeleton/hand/left\" },
        { \"name\": \"/actions/Drone/in/Fly\", \"type\": \"vector2\" }
      ],
      \"default_bindings\": [
        { \"controller_type\": \"knuckles\", \"binding_url\": \"bindings_knuckles.json\" },
        { \"controller_type\": \"knuckles\", \"binding_url\": \"other.json\" },
        { \"controller_type\": \"oculus_touch\", \"binding_url\": \"bindings_oculus_touch.json\" }
      ],
      \"localization\": [
        { \"language_tag\": \"fr\", \"/actions/Global/in/Jump\": \"Sauter\" },
        { \"language_tag\": \"en_US\", \"/actions/Global/in/Jump\": \"Jump up\", \"/actions/Global\": \"Everywhere\" }
      ]
    }";

    #[test]
    fn manifest_names_and_sets() {
        let m = Manifest::parse(MANIFEST).unwrap();
        assert_eq!(m.action_name("/actions/global/in/jump"), "Jump up");
        assert_eq!(m.action_name("/actions/global/in/grip_axis"), "Grip axis");
        // Found case-insensitively, named from the manifest's own spelling.
        assert_eq!(m.action_name("/actions/global/in/skeletonlefthand"), "Skeleton left hand");
        // Not in the manifest: named from the (lowercased) path.
        assert_eq!(m.action_name("/actions/global/in/old_thing"), "Old thing");
        assert!(m.action("/ACTIONS/GLOBAL/IN/JUMP").unwrap().mandatory);
        assert_eq!(m.set("/actions/global").unwrap().name, "Everywhere");
        assert!(m.set("/actions/debug").unwrap().hidden);
        // A set only its actions mention.
        assert_eq!(m.set("/actions/drone").unwrap().name, "Drone");
        assert_eq!(m.default_binding("knuckles"), Some("bindings_knuckles.json"));
    }

    #[test]
    fn pretty_names() {
        assert_eq!(pretty_name("/actions/a/in/Stick_Click"), "Stick click");
        assert_eq!(pretty_name("/actions/a/in/grabPinch"), "Grab pinch");
        assert_eq!(pretty_name("/actions/a/in/Toggle_HUD"), "Toggle HUD");
        assert_eq!(pretty_name("/actions/One_Hand"), "One hand");
    }

    fn doc() -> BindingDoc {
        BindingDoc::parse(
            r#"{ "controller_type": "knuckles", "chords": [1], "bindings": { "/actions/global": {
                "sources": [
                  { "path": "/user/hand/left/input/thumbstick", "mode": "joystick", "inputs": { "position": { "output": "/actions/global/in/move" } }, "parameters": { "deadzone_pct": "20" } },
                  { "path": "/user/hand/right/input/a", "mode": "radial", "inputs": {} }
                ],
                "chords": [ { "output": "/actions/global/in/jump", "inputs": [] } ]
            } } }"#,
        )
        .unwrap()
    }

    #[test]
    fn sources_and_edits_keep_unknown_keys() {
        let mut d = doc();
        let s = d.sources("/actions/Global");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].hand, Some(Hand::Left));
        assert_eq!(s[0].input, "thumbstick");
        assert_eq!(s[0].output("position"), Some("/actions/global/in/move"));
        let i = d.add_source("/actions/Global", Hand::Right, "trigger", Mode::Trigger, None);
        d.set_output("/actions/Global", i, "pull", Some("/actions/Global/in/Grip_Axis"), None);
        assert_eq!(d.sources("/actions/global")[i].output("pull"), Some("/actions/global/in/grip_axis"));
        let text = d.to_json();
        assert!(text.contains("\"chords\""));
        assert!(text.contains("deadzone_pct"));
        // A set the file doesn't have yet is created lowercased.
        d.add_source("/actions/Drone", Hand::Left, "a", Mode::Button, None);
        assert!(d.set_keys().contains(&"/actions/drone".to_string()));
    }

    #[test]
    fn mirror_mode() {
        let idx = controller("knuckles").unwrap();
        let mut d = doc();
        // Editing the left stick creates and fills the right one.
        d.set_output("/actions/global", 0, "click", Some("/actions/global/in/jump"), Some(idx));
        let right: Vec<Source> = d.sources("/actions/global").into_iter().filter(|s| s.hand == Some(Hand::Right) && s.input == "thumbstick").collect();
        assert_eq!(right.len(), 1);
        assert_eq!(right[0].output("click"), Some("/actions/global/in/jump"));
        assert_eq!(right[0].output("position"), None);
        // Removing one removes both.
        d.remove_source("/actions/global", 0, Some(idx));
        assert!(d.sources("/actions/global").iter().all(|s| s.input != "thumbstick"));

        // Touch: X on the left mirrors to A on the right.
        let touch = controller("oculus_touch").unwrap();
        let mut t = BindingDoc::empty("oculus_touch");
        let i = t.add_source("/actions/global", Hand::Left, "x", Mode::Button, Some(touch));
        t.set_output("/actions/global", i, "click", Some("/actions/global/in/jump"), Some(touch));
        let s = t.sources("/actions/global");
        assert_eq!(s.len(), 2);
        assert_eq!((s[1].hand, s[1].input.as_str(), s[1].output("click")), (Some(Hand::Right), "a", Some("/actions/global/in/jump")));
        // The Touch menu button is left-only: nothing to mirror to.
        t.add_source("/actions/global", Hand::Left, "application_menu", Mode::Button, Some(touch));
        assert_eq!(t.sources("/actions/global").len(), 3);
    }

    #[test]
    fn change_mode_keeps_shared_slots() {
        let mut d = doc();
        d.set_output("/actions/global", 0, "click", Some("/actions/global/in/jump"), None);
        d.change_mode("/actions/global", 0, Mode::Button, None);
        let s = &d.sources("/actions/global")[0];
        assert_eq!(s.mode, "button");
        assert_eq!(s.output("click"), Some("/actions/global/in/jump"));
        assert_eq!(s.output("position"), None);
        assert!(!d.to_json().contains("deadzone_pct"));
    }

    #[test]
    fn unreadable_modes_break_xrizer() {
        let mut d = doc();
        assert_eq!(d.unreadable_modes(), vec![("/actions/global".to_string(), 1, "radial".to_string())]);
        assert_eq!(d.drop_unreadable_modes(), 1);
        assert!(d.unreadable_modes().is_empty());
        assert_eq!(d.sources("/actions/global").len(), 1);
    }

    #[test]
    fn haptics_and_poses() {
        let mut d = BindingDoc::empty("knuckles");
        d.set_haptic("/actions/global", Hand::Left, Some("/actions/global/out/haptic"), true);
        assert_eq!(d.haptic("/actions/global", Hand::Right).as_deref(), Some("/actions/global/out/haptic"));
        d.set_haptic("/actions/global", Hand::Right, None, false);
        assert_eq!(d.haptic("/actions/global", Hand::Right), None);
        assert!(d.haptic("/actions/global", Hand::Left).is_some());
        d.set_pose("/actions/global", "/actions/global/in/pose", Hand::Left, Some("raw"), true);
        d.set_pose("/actions/global", "/actions/global/in/pose", Hand::Right, Some("tip"), false);
        let p = d.poses("/actions/global");
        assert_eq!(p.len(), 2);
        assert!(p.contains(&PoseBinding { action: "/actions/global/in/pose".into(), hand: Hand::Right, point: "tip".into() }));
    }

    #[test]
    fn chords() {
        let mut d = BindingDoc::empty("knuckles");
        let i = d.add_chord("/actions/Global", "/actions/Global/in/Jump");
        d.toggle_chord_input("/actions/global", i, Hand::Left, "a", "click");
        d.toggle_chord_input("/actions/global", i, Hand::Left, "b", "click");
        let c = &d.chords("/actions/global")[0];
        assert_eq!(c.action, "/actions/global/in/jump");
        assert_eq!(c.inputs.len(), 2);
        assert_eq!((c.inputs[1].hand, c.inputs[1].input.as_str(), c.inputs[1].slot.as_str()), (Some(Hand::Left), "b", "click"));
        assert_eq!(d.bound_count("/actions/global"), 1);
        // Toggling again takes it out.
        d.toggle_chord_input("/actions/global", i, Hand::Left, "a", "click");
        assert_eq!(d.chords("/actions/global")[0].inputs.len(), 1);
        d.set_chord_action("/actions/global", i, "/actions/global/in/crouch");
        assert_eq!(d.chords("/actions/global")[0].action, "/actions/global/in/crouch");
        d.remove_chord("/actions/global", i);
        assert!(d.chords("/actions/global").is_empty());
    }

    #[test]
    fn xrizer_file_names() {
        assert_eq!(xrizer_file("knuckles"), Some("knuckles.json"));
        assert_eq!(xrizer_file("oculus_touch"), Some("oculustouch.json"));
        assert_eq!(xrizer_file("vive_controller"), Some("vivecontroller.json"));
        assert_eq!(xrizer_file("holographic_controller"), None);
        assert_eq!(type_from_xrizer_file("oculustouch.json"), "oculus_touch");
        assert_eq!(type_from_xrizer_file("bindings_knuckles.json"), "knuckles");
        assert_eq!(type_from_xrizer_file("oculus_touch.json"), "oculus_touch");
    }

    #[test]
    fn spots_an_xrizer_that_reloads_live() {
        let dir = std::env::temp_dir().join(format!("monadeck-xrizer-lib-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lib = dir.join("vrclient.so");
        std::fs::write(&lib, b"\x7fELF...upstream build...").unwrap();
        assert!(!library_has(&lib, LIVE_RELOAD_MARKER));
        // Replaced by the fork's (a newer file): read again.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&lib, b"\x7fELF...Personal bindings changed in {}...").unwrap();
        assert!(library_has(&lib, LIVE_RELOAD_MARKER));
        assert!(!library_has(&dir.join("missing.so"), LIVE_RELOAD_MARKER));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_save_reset() {
        let dir = std::env::temp_dir().join(format!("monadeck-bindings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let data = dir.join("Game_Data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("actions.json"), MANIFEST).unwrap();
        std::fs::write(data.join("bindings_knuckles.json"), doc().to_json()).unwrap();
        let actions = data.join("actions.json");

        let dirs = [dir.clone()];
        let o = open(&actions, &dirs, "knuckles").unwrap();
        assert!(!o.personal);
        assert_eq!(o.doc.sources("/actions/global").len(), 2);
        // Touch is listed but its file is missing: a blank binding.
        assert!(open(&actions, &dirs, "oculus_touch").unwrap().doc.sources("/actions/global").is_empty());

        let mut d = o.doc.clone();
        d.remove_source("/actions/global", 1, None);
        let p = save_personal(&dirs, "knuckles", &d).unwrap();
        assert_eq!(p, dir.join("xrizer/knuckles.json"));
        let o = open(&actions, &dirs, "knuckles").unwrap();
        assert!(o.personal);
        assert_eq!(o.doc.sources("/actions/global").len(), 1);

        assert!(reset_personal(&dirs, "knuckles").unwrap());
        assert!(dir.join("xrizer/knuckles.json.bak").is_file());
        assert!(!open(&actions, &dirs, "knuckles").unwrap().personal);
        assert!(!reset_personal(&dirs, "knuckles").unwrap());
        // A game in a subfolder: the copy goes in both folders, and resets from both.
        let two = [data.clone(), dir.clone()];
        save_personal(&two, "knuckles", &d).unwrap();
        assert!(data.join("xrizer/knuckles.json").is_file() && dir.join("xrizer/knuckles.json").is_file());
        assert!(open(&actions, &two, "knuckles").unwrap().personal);
        assert!(reset_personal(&two, "knuckles").unwrap());
        assert!(!data.join("xrizer/knuckles.json").exists() && !dir.join("xrizer/knuckles.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
