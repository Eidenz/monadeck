//! Gaming-mode remap profiles: how VR controller inputs become gamepad, key
//! and mouse events while the overlay's gaming mode is on. Shared by the
//! overlay (which applies them every frame) and the desktop app's editor.
//!
//! Profiles live as JSON in `~/.config/monadeck/gamepad_profiles/*.json`:
//! ```json
//! { "name": "Xbox", "game": null, "rules": [
//!   { "hand": "left",  "input": "stick_x", "target": { "axis": "LX" } },
//!   { "hand": "right", "input": "a",       "target": { "button": "A" } },
//!   { "hand": "right", "input": "stick_x", "target": "mouse_x", "speed": 1100 },
//!   { "hand": "left",  "input": "stick_up","target": { "key": 17 } }
//! ] }
//! ```
use std::path::PathBuf;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::paths::monadeck_config_dir;

/// Xbox 360 buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Btn {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    Back,
    Start,
    Guide,
    L3,
    R3,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
}

impl Btn {
    pub const ALL: [Btn; 15] = [
        Btn::A, Btn::B, Btn::X, Btn::Y, Btn::LB, Btn::RB, Btn::Back, Btn::Start, Btn::Guide, Btn::L3, Btn::R3,
        Btn::DpadUp, Btn::DpadDown, Btn::DpadLeft, Btn::DpadRight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Btn::A => "A",
            Btn::B => "B",
            Btn::X => "X",
            Btn::Y => "Y",
            Btn::LB => "LB",
            Btn::RB => "RB",
            Btn::Back => "Back",
            Btn::Start => "Start",
            Btn::Guide => "Guide",
            Btn::L3 => "L3",
            Btn::R3 => "R3",
            Btn::DpadUp => "D-pad ▲",
            Btn::DpadDown => "D-pad ▼",
            Btn::DpadLeft => "D-pad ◀",
            Btn::DpadRight => "D-pad ▶",
        }
    }
}

/// Xbox 360 analog axes: sticks -1..1 (right / up positive), triggers 0..1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Axis {
    LX,
    LY,
    RX,
    RY,
    LT,
    RT,
}

impl Axis {
    pub const ALL: [Axis; 6] = [Axis::LX, Axis::LY, Axis::RX, Axis::RY, Axis::LT, Axis::RT];

    pub fn label(self) -> &'static str {
        match self {
            Axis::LX => "Left stick ◀▶",
            Axis::LY => "Left stick ▲▼",
            Axis::RX => "Right stick ◀▶",
            Axis::RY => "Right stick ▲▼",
            Axis::LT => "LT",
            Axis::RT => "RT",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hand {
    Left,
    Right,
}

/// A controller input a rule can read. Analog ones (`trigger`, `grip`,
/// `stick_x/y`, `pad_x/y`) drive axes or the mouse, and become a button past
/// the rule's `threshold`; the rest are already on/off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    Trigger,
    Grip,
    StickX,
    StickY,
    StickClick,
    A,
    B,
    PadX,
    PadY,
    PadTouch,
    /// Trackpad pressed anywhere.
    PadPress,
    /// Trackpad pressed in that quadrant.
    PadUp,
    PadDown,
    PadLeft,
    PadRight,
    /// Stick pushed past the threshold that way.
    StickUp,
    StickDown,
    StickLeft,
    StickRight,
}

impl Input {
    pub fn is_analog(self) -> bool {
        matches!(self, Input::Trigger | Input::Grip | Input::StickX | Input::StickY | Input::PadX | Input::PadY)
    }
}

/// What a rule drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Button(Btn),
    Axis(Axis),
    /// evdev `KEY_*` code (30 = A, 17 = W, 57 = space…).
    Key(u16),
    /// evdev `BTN_*` code (272 left, 273 right, 274 middle).
    MouseButton(u16),
    /// Relative mouse motion from an analog input (`speed` px/s at full tilt).
    MouseX,
    MouseY,
    /// Scroll from an analog input (`speed` notches/s at full tilt).
    WheelY,
    WheelX,
}

fn default_threshold() -> f32 {
    0.5
}

fn default_speed() -> f32 {
    900.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub hand: Hand,
    pub input: Input,
    pub target: Target,
    /// Analog input → button: pressed past this. Also the trackpad press force
    /// and the stick's digital deflection.
    #[serde(default = "default_threshold")]
    pub threshold: f32,
    /// Flip an analog input's sign (mouse Y, inverted look…).
    #[serde(default)]
    pub invert: bool,
    /// Mouse / wheel speed at full deflection.
    #[serde(default = "default_speed")]
    pub speed: f32,
}

impl Rule {
    pub fn new(hand: Hand, input: Input, target: Target) -> Self {
        Self { hand, input, target, threshold: 0.5, invert: false, speed: 900.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    /// Picked automatically when a launched game's name contains this
    /// (case-insensitive).
    #[serde(default)]
    pub game: Option<String>,
    pub rules: Vec<Rule>,
}

pub const STOCK_NAME: &str = "Xbox";

impl Profile {
    /// The stock layout: a full Xbox pad on Index-style controllers.
    /// Right A/B → A/B, left A/B → X/Y, sticks + clicks, triggers analog, grip
    /// → bumpers, left trackpad → d-pad, right trackpad ▲ Start / ▼ Back.
    pub fn xbox() -> Self {
        use Hand::*;
        use Input::*;
        let rules = vec![
            Rule::new(Left, StickX, Target::Axis(Axis::LX)),
            Rule::new(Left, StickY, Target::Axis(Axis::LY)),
            Rule::new(Right, StickX, Target::Axis(Axis::RX)),
            Rule::new(Right, StickY, Target::Axis(Axis::RY)),
            Rule::new(Left, Trigger, Target::Axis(Axis::LT)),
            Rule::new(Right, Trigger, Target::Axis(Axis::RT)),
            Rule::new(Left, Grip, Target::Button(Btn::LB)),
            Rule::new(Right, Grip, Target::Button(Btn::RB)),
            Rule::new(Right, A, Target::Button(Btn::A)),
            Rule::new(Right, B, Target::Button(Btn::B)),
            Rule::new(Left, A, Target::Button(Btn::X)),
            Rule::new(Left, B, Target::Button(Btn::Y)),
            Rule::new(Left, StickClick, Target::Button(Btn::L3)),
            Rule::new(Right, StickClick, Target::Button(Btn::R3)),
            Rule::new(Left, PadUp, Target::Button(Btn::DpadUp)),
            Rule::new(Left, PadDown, Target::Button(Btn::DpadDown)),
            Rule::new(Left, PadLeft, Target::Button(Btn::DpadLeft)),
            Rule::new(Left, PadRight, Target::Button(Btn::DpadRight)),
            Rule::new(Right, PadUp, Target::Button(Btn::Start)),
            Rule::new(Right, PadDown, Target::Button(Btn::Back)),
        ];
        Self { name: STOCK_NAME.into(), game: None, rules }
    }

    /// A keyboard + mouse example (WASD on the left stick, mouse-look on the
    /// right, trigger = left click, grip = right click): a template to copy.
    pub fn keyboard_mouse_example() -> Self {
        use Hand::*;
        use Input::*;
        let mut look_x = Rule::new(Right, StickX, Target::MouseX);
        look_x.speed = 1100.0;
        let mut look_y = Rule::new(Right, StickY, Target::MouseY);
        look_y.speed = 1100.0;
        look_y.invert = true;
        let mut scroll = Rule::new(Left, PadY, Target::WheelY);
        scroll.speed = 6.0;
        let rules = vec![
            Rule::new(Left, StickUp, Target::Key(17)),    // W
            Rule::new(Left, StickDown, Target::Key(31)),  // S
            Rule::new(Left, StickLeft, Target::Key(30)),  // A
            Rule::new(Left, StickRight, Target::Key(32)), // D
            Rule::new(Left, StickClick, Target::Key(42)), // left shift
            Rule::new(Right, A, Target::Key(57)),         // space
            Rule::new(Right, B, Target::Key(46)),         // C
            Rule::new(Left, A, Target::Key(18)),          // E
            Rule::new(Left, B, Target::Key(16)),          // Q
            Rule::new(Right, Trigger, Target::MouseButton(272)),
            Rule::new(Right, Grip, Target::MouseButton(273)),
            Rule::new(Left, Trigger, Target::Key(29)),    // left ctrl
            Rule::new(Left, Grip, Target::Key(15)),       // tab
            Rule::new(Right, PadUp, Target::Key(1)),      // escape
            Rule::new(Right, PadDown, Target::Key(50)),   // M
            look_x,
            look_y,
            scroll,
        ];
        Self { name: "Keyboard + mouse (example)".into(), game: None, rules }
    }

    /// The untouched stock layout (the editor shows it read-only).
    pub fn is_stock(&self) -> bool {
        self.name == STOCK_NAME && self.rules == Self::xbox().rules
    }
}

pub fn dir() -> PathBuf {
    monadeck_config_dir().join("gamepad_profiles")
}

/// One profile file for the editor.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileFile {
    /// File name inside the profiles dir ("" for the built-in stock entry
    /// when no file holds it).
    pub file: String,
    pub profile: Profile,
    pub stock: bool,
}

/// Every `.json` in the profiles dir, by file name. An empty dir gets the
/// stock files written out as editable templates.
fn read_files() -> Vec<(String, Profile)> {
    let dir = dir();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect())
        .unwrap_or_default();
    if paths.is_empty() {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::warn!("gamepad profiles: could not create {}: {e}", dir.display());
        } else {
            for (file, p) in [("xbox.json", Profile::xbox()), ("keyboard-mouse-example.json", Profile::keyboard_mouse_example())] {
                if let Ok(json) = serde_json::to_string_pretty(&p) {
                    let path = dir.join(file);
                    if std::fs::write(&path, json).is_ok() {
                        paths.push(path);
                    }
                }
            }
        }
    }
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| serde_json::from_str::<Profile>(&s).map_err(|e| e.to_string())) {
            Ok(p) => out.push((file, p)),
            Err(e) => log::warn!("gamepad profiles: skipping {}: {e}", path.display()),
        }
    }
    out
}

/// The overlay's list: the stock Xbox layout first, then every file (an
/// unchanged stock file isn't repeated).
pub fn load_all() -> Vec<Profile> {
    let mut out = vec![Profile::xbox()];
    out.extend(read_files().into_iter().map(|(_, p)| p).filter(|p| !p.is_stock()));
    out
}

/// The editor's list: every file, with the stock layout flagged (and added
/// as a file-less entry when no file holds it, so it can still be copied).
pub fn list_files() -> Vec<ProfileFile> {
    let mut out: Vec<ProfileFile> = read_files()
        .into_iter()
        .map(|(file, profile)| ProfileFile { stock: profile.is_stock(), file, profile })
        .collect();
    if !out.iter().any(|f| f.stock) {
        out.insert(0, ProfileFile { file: String::new(), profile: Profile::xbox(), stock: true });
    } else {
        out.sort_by_key(|f| !f.stock);
    }
    out
}

/// A file name for a profile name: `my-game.json` (ASCII letters, digits and
/// dashes only), made unique against the files already there.
pub fn file_name_for(name: &str) -> String {
    let mut slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        slug = "profile".into();
    }
    let dir = dir();
    let mut candidate = format!("{slug}.json");
    let mut n = 2;
    while dir.join(&candidate).exists() {
        candidate = format!("{slug}-{n}.json");
        n += 1;
    }
    candidate
}

fn check_file_name(file: &str) -> anyhow::Result<()> {
    if file.is_empty() || file.contains('/') || file.contains("..") || !file.ends_with(".json") {
        anyhow::bail!("bad profile file name {file:?}");
    }
    Ok(())
}

/// Write a profile; an empty `file` picks a fresh name from the profile's
/// name. Returns the file name used.
pub fn save(file: &str, profile: &Profile) -> anyhow::Result<String> {
    let file = if file.is_empty() { file_name_for(&profile.name) } else { file.to_string() };
    check_file_name(&file)?;
    let dir = dir();
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(profile)?;
    std::fs::write(dir.join(&file), json)?;
    Ok(file)
}

pub fn delete(file: &str) -> anyhow::Result<()> {
    check_file_name(file)?;
    std::fs::remove_file(dir().join(file))?;
    Ok(())
}

/// The newest change under the profiles dir (the overlay polls this to pick
/// up edits without a manual reload).
pub fn dir_mtime() -> Option<SystemTime> {
    let dir = dir();
    let mut newest = std::fs::metadata(&dir).and_then(|m| m.modified()).ok()?;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                newest = newest.max(t);
            }
        }
    }
    Some(newest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_profile_round_trips_through_json() {
        let p = Profile::xbox();
        let json = serde_json::to_string(&p).unwrap();
        let back: Profile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
        assert!(json.contains("\"dpad_up\"") || json.contains("DpadUp"));
        assert!(back.is_stock());
    }

    #[test]
    fn targets_have_the_documented_shapes() {
        let mut r = Rule::new(Hand::Right, Input::StickX, Target::MouseX);
        r.speed = 1100.0;
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"target\":\"mouse_x\""));
        assert!(json.contains("\"hand\":\"right\""));
        assert!(json.contains("\"input\":\"stick_x\""));
        let k: Rule = serde_json::from_str(r#"{"hand":"left","input":"a","target":{"key":30}}"#).unwrap();
        assert_eq!(k.target, Target::Key(30));
        assert_eq!(k.threshold, 0.5);
        assert_eq!(k.speed, 900.0);
    }

    #[test]
    fn file_names_are_slugs() {
        assert!(file_name_for("Elden Ring (PC)").starts_with("elden-ring-pc"));
        assert_eq!(&file_name_for("")[..7], "profile");
        assert!(check_file_name("../x.json").is_err());
        assert!(check_file_name("ok.json").is_ok());
    }
}
