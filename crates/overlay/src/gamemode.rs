//! Gaming mode: the mirrored desktop becomes a games console. Controller
//! inputs feed a virtual Xbox pad (or keys / mouse through a remap profile),
//! the lasers stay off until the watch's Mouse button asks for one, the VR
//! app behind is blocked from seeing the controllers at all, and the screen
//! can be pinned to the world, trail the head, or be held between the hands
//! like a handheld.
//!
//! Routing per hand, every frame:
//! - on the watch (or its mini pill): the watch gets it, nothing else;
//! - pointer on (from the watch): the desktop viewer gets it (mouse);
//! - otherwise: the profile turns it into pad / key / mouse events, and the
//!   viewer never sees it.
//!
//! With the dashboard summoned, routing pauses and every hand is a pointer.
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use monadeck_core::gamepad_profiles::{Axis, Btn};

use crate::gamepad::{PadMouse, Rumble, VirtualPad};
pub use crate::desktop::DockMode;

/// A pointer hand that has been off every screen for this long goes back to
/// the pad (a wobble across an edge doesn't drop the mouse).
const LEAVE_GRACE: Duration = Duration::from_millis(500);
/// A pointer turned on from the watch that never reaches a screen gives up.
const ARM_TIMEOUT: Duration = Duration::from_secs(4);

/// One controller's inputs this frame, before any routing.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawHand {
    pub active: bool,
    pub trigger: f32,
    pub grip: f32,
    /// Thumbstick, right / up positive.
    pub stick: (f32, f32),
    pub stick_click: bool,
    pub a: bool,
    pub b: bool,
    /// Trackpad touch position, right / up positive.
    pub pad: (f32, f32),
    pub pad_force: f32,
    pub pad_touch: bool,
}

pub use monadeck_core::gamepad_profiles::{Hand, Input, Profile, Target};

/// An input's value this frame in its own range (0..1 or -1..1); digital
/// inputs read as 0 / 1. `press`: the rule's threshold (trackpad force,
/// stick deflection).
fn input_value(input: Input, h: &RawHand, press: f32) -> f32 {
    let pressed = h.pad_force >= press;
    let quadrant = |dx: f32, dy: f32| -> bool {
        // Split along the diagonals so each direction owns a quarter.
        let (x, y) = h.pad;
        pressed && (x * dx + y * dy) > 0.0 && (x * dx + y * dy).abs() >= (x * dy - y * dx).abs()
    };
    match input {
        Input::Trigger => h.trigger,
        Input::Grip => h.grip,
        Input::StickX => h.stick.0,
        Input::StickY => h.stick.1,
        Input::PadX => h.pad.0,
        Input::PadY => h.pad.1,
        Input::StickClick => h.stick_click as u8 as f32,
        Input::A => h.a as u8 as f32,
        Input::B => h.b as u8 as f32,
        Input::PadTouch => h.pad_touch as u8 as f32,
        Input::PadPress => pressed as u8 as f32,
        Input::PadUp => quadrant(0.0, 1.0) as u8 as f32,
        Input::PadDown => quadrant(0.0, -1.0) as u8 as f32,
        Input::PadLeft => quadrant(-1.0, 0.0) as u8 as f32,
        Input::PadRight => quadrant(1.0, 0.0) as u8 as f32,
        Input::StickUp => (h.stick.1 >= press) as u8 as f32,
        Input::StickDown => (h.stick.1 <= -press) as u8 as f32,
        Input::StickLeft => (h.stick.0 <= -press) as u8 as f32,
        Input::StickRight => (h.stick.0 >= press) as u8 as f32,
    }
}

/// Where a hand's input goes this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Driving the pad / keys / mouse; invisible to the desktop viewer.
    Pad,
    /// Driving the desktop viewer's laser.
    Pointer,
    /// On the watch: the watch alone gets it.
    Watch,
}

pub struct GameMode {
    pub enabled: bool,
    pad: Option<VirtualPad>,
    mouse: Option<PadMouse>,
    /// Why the pad could not be created (shown in settings).
    pub error: Option<String>,
    pub dock: DockMode,
    pub rumble_enabled: bool,
    pub profiles: Vec<Profile>,
    pub profile: usize,
    /// Per hand: the laser is on (mouse mode).
    pub pointer: [bool; 2],
    off_screen_since: [Option<Instant>; 2],
    /// Pointer armed from the watch: waiting for the hand to reach a screen.
    armed_at: [Option<Instant>; 2],
    /// Digital targets pressed last frame (for release edges).
    digital_prev: HashSet<Target>,
    keys_down: HashSet<u16>,
    last_frame: Option<Instant>,
    pub rumble: Rumble,
    /// The watch asked for the Guide button.
    pub guide_request: bool,
}

impl GameMode {
    pub fn new(dock: DockMode, rumble_enabled: bool, profile_name: Option<&str>) -> Self {
        let profiles = monadeck_core::gamepad_profiles::load_all();
        let profile = profile_name.and_then(|n| profiles.iter().position(|p| p.name == n)).unwrap_or(0);
        Self {
            enabled: false,
            pad: None,
            mouse: None,
            error: None,
            dock,
            rumble_enabled,
            profiles,
            profile,
            pointer: [false; 2],
            off_screen_since: [None; 2],
            armed_at: [None; 2],
            digital_prev: HashSet::new(),
            keys_down: HashSet::new(),
            last_frame: None,
            rumble: Rumble::default(),
            guide_request: false,
        }
    }

    pub fn profile_name(&self) -> &str {
        self.profiles.get(self.profile).map_or("Xbox", |p| p.name.as_str())
    }

    pub fn profile_names(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.name.clone()).collect()
    }

    pub fn reload_profiles(&mut self) {
        let name = self.profile_name().to_string();
        self.profiles = monadeck_core::gamepad_profiles::load_all();
        self.profile = self.profiles.iter().position(|p| p.name == name).unwrap_or(0);
    }

    pub fn select_profile(&mut self, i: usize) {
        if i < self.profiles.len() {
            self.profile = i;
            self.release_targets();
        }
    }

    #[allow(dead_code)]
    pub fn cycle_profile(&mut self) {
        if !self.profiles.is_empty() {
            self.select_profile((self.profile + 1) % self.profiles.len());
        }
    }

    /// A game was launched: pick the profile made for it, if any.
    pub fn auto_select(&mut self, game_name: &str) -> Option<String> {
        let lower = game_name.to_lowercase();
        let hit = self.profiles.iter().position(|p| p.game.as_deref().is_some_and(|g| !g.trim().is_empty() && lower.contains(&g.trim().to_lowercase())))?;
        if hit != self.profile {
            self.select_profile(hit);
        }
        Some(self.profiles[hit].name.clone())
    }

    /// Turn gaming mode on (creating the pad) or off (everything released,
    /// the pad unplugged).
    pub fn set_enabled(&mut self, on: bool) {
        if on == self.enabled {
            return;
        }
        if on {
            match VirtualPad::open() {
                Ok(p) => {
                    log::info!("gaming: virtual pad at {}", p.node().unwrap_or_else(|| "?".into()));
                    self.pad = Some(p);
                    self.error = None;
                }
                Err(e) => {
                    log::warn!("gaming: no virtual pad: {e}");
                    self.error = Some(e.to_string());
                }
            }
            match PadMouse::open() {
                Ok(m) => self.mouse = Some(m),
                Err(e) => log::warn!("gaming: no pad mouse: {e}"),
            }
        } else {
            self.release_targets();
            self.pad = None;
            self.mouse = None;
        }
        self.enabled = on;
        self.pointer = [false; 2];
        self.armed_at = [None; 2];
        self.rumble = Rumble::default();
    }

    pub fn pad_ok(&self) -> bool {
        self.pad.is_some()
    }

    /// Turn a hand's pointer on from the watch (it turns itself off once the
    /// hand has been on a screen and leaves it), or off.
    pub fn toggle_pointer(&mut self, hand: usize) {
        if hand > 1 {
            return;
        }
        if self.pointer[hand] {
            self.pointer[hand] = false;
            self.armed_at[hand] = None;
        } else {
            self.pointer[hand] = true;
            self.armed_at[hand] = Some(Instant::now());
            self.off_screen_since[hand] = None;
        }
    }

    /// Let go of every key / button / axis this mode is holding.
    fn release_targets(&mut self) {
        if let Some(p) = &mut self.pad {
            p.release_all();
        }
        if let Some(m) = &mut self.mouse {
            m.release_all();
        }
        self.digital_prev.clear();
        // Keys are released through the viewer's keyboard in `route` (we don't
        // own it); remember them so the next call lets go.
    }

    /// Keys still held that must be released through the viewer's keyboard.
    pub fn take_stale_keys(&mut self) -> Vec<u16> {
        if self.enabled && !self.digital_prev.is_empty() {
            return Vec::new();
        }
        self.keys_down.drain().collect()
    }

    /// Route one frame. `on_screen[h]`: the hand's laser would hit a shown
    /// screen. `on_watch[h]`: it is on the watch. `paused`: the dashboard is
    /// up (everything is a pointer). `key`: press/release an evdev key through
    /// the viewer's keyboard. Returns each hand's role.
    pub fn route(&mut self, raw: &[RawHand; 2], on_screen: [bool; 2], on_watch: [bool; 2], paused: bool, mut key: impl FnMut(u16, bool)) -> [Role; 2] {
        let now = Instant::now();
        let dt = self.last_frame.map_or(0.0, |t| now.duration_since(t).as_secs_f32()).min(0.1);
        self.last_frame = Some(now);
        if let Some(p) = &mut self.pad {
            self.rumble = p.poll();
            if !self.rumble_enabled {
                self.rumble = Rumble::default();
            }
        }
        if !self.enabled {
            return [Role::Pointer; 2];
        }
        if paused {
            self.pointer = [false; 2];
            self.armed_at = [None; 2];
            self.release_targets();
            for k in self.keys_down.drain() {
                key(k, false);
            }
            return [Role::Pointer; 2];
        }
        let mut roles = [Role::Pad; 2];
        for h in 0..2 {
            // --- pointer off again: armed but never reached a screen, or it
            // has been off every screen for a moment.
            if self.pointer[h] {
                if on_screen[h] {
                    self.armed_at[h] = None;
                    self.off_screen_since[h] = None;
                } else if let Some(t0) = self.armed_at[h] {
                    if now.duration_since(t0) > ARM_TIMEOUT {
                        self.pointer[h] = false;
                        self.armed_at[h] = None;
                    }
                } else {
                    let since = *self.off_screen_since[h].get_or_insert(now);
                    if now.duration_since(since) > LEAVE_GRACE {
                        self.pointer[h] = false;
                    }
                }
            }
            roles[h] = if on_watch[h] {
                Role::Watch
            } else if self.pointer[h] {
                Role::Pointer
            } else {
                Role::Pad
            };
        }

        // --- profile → targets ------------------------------------------------
        let mut digital: HashSet<Target> = HashSet::new();
        let mut axes: HashMap<Axis, f32> = HashMap::new();
        let mut mouse = (0.0f32, 0.0f32);
        let mut wheel = (0.0f32, 0.0f32);
        if let Some(profile) = self.profiles.get(self.profile) {
            for rule in &profile.rules {
                let h = match rule.hand {
                    Hand::Left => 0,
                    Hand::Right => 1,
                };
                if roles[h] != Role::Pad || !raw[h].active {
                    continue;
                }
                let mut v = input_value(rule.input, &raw[h], rule.threshold);
                if rule.invert && rule.input.is_analog() {
                    v = -v;
                }
                let pressed = if rule.input.is_analog() { v.abs() >= rule.threshold } else { v > 0.5 };
                match rule.target {
                    Target::Button(_) | Target::Key(_) | Target::MouseButton(_) => {
                        if pressed {
                            digital.insert(rule.target);
                        }
                    }
                    Target::Axis(a) => {
                        let v = if rule.input.is_analog() { v } else if pressed { 1.0 } else { 0.0 };
                        let e = axes.entry(a).or_insert(0.0);
                        if v.abs() > e.abs() {
                            *e = v;
                        }
                    }
                    Target::MouseX => mouse.0 += v * rule.speed * dt,
                    Target::MouseY => mouse.1 -= v * rule.speed * dt, // screen Y grows downward
                    Target::WheelY => wheel.1 += v * rule.speed * dt,
                    Target::WheelX => wheel.0 += v * rule.speed * dt,
                }
            }
        }
        // Edges: press what's new, release what's gone.
        let released: Vec<Target> = self.digital_prev.difference(&digital).copied().collect();
        let pressed: Vec<Target> = digital.difference(&self.digital_prev).copied().collect();
        for (t, down) in released.into_iter().map(|t| (t, false)).chain(pressed.into_iter().map(|t| (t, true))) {
            match t {
                Target::Button(b) => {
                    if let Some(p) = &mut self.pad {
                        p.button(b, down);
                    }
                }
                Target::Key(k) => {
                    key(k, down);
                    if down {
                        self.keys_down.insert(k);
                    } else {
                        self.keys_down.remove(&k);
                    }
                }
                Target::MouseButton(b) => {
                    if let Some(m) = &mut self.mouse {
                        m.button(b, down);
                    }
                }
                _ => {}
            }
        }
        self.digital_prev = digital;
        if let Some(p) = &mut self.pad {
            for a in Axis::ALL {
                p.axis(a, axes.get(&a).copied().unwrap_or(0.0));
            }
            if self.guide_request {
                self.guide_request = false;
                p.tap(Btn::Guide);
            }
            p.flush();
        }
        if let Some(m) = &mut self.mouse {
            if mouse != (0.0, 0.0) {
                m.motion(mouse.0, mouse.1);
            }
            if wheel != (0.0, 0.0) {
                m.wheel(wheel.0, wheel.1);
            }
        }
        roles
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn hand(trigger: f32, stick: (f32, f32)) -> RawHand {
        RawHand { active: true, trigger, stick, ..Default::default() }
    }

    #[test]
    fn quadrants_split_the_trackpad() {
        let mut h = RawHand { active: true, pad_force: 1.0, ..Default::default() };
        h.pad = (0.1, 0.9);
        assert_eq!(input_value(Input::PadUp, &h, 0.5), 1.0);
        assert_eq!(input_value(Input::PadRight, &h, 0.5), 0.0);
        h.pad = (-0.8, -0.2);
        assert_eq!(input_value(Input::PadLeft, &h, 0.5), 1.0);
        assert_eq!(input_value(Input::PadDown, &h, 0.5), 0.0);
        h.pad_force = 0.1;
        assert_eq!(input_value(Input::PadLeft, &h, 0.5), 0.0);
    }

    #[test]
    fn watch_arms_the_pointer_and_leaving_the_screen_drops_it() {
        let mut g = GameMode::new(DockMode::World, false, None);
        g.enabled = true; // no real pad in tests
        let idle = [hand(0.0, (0.0, 0.0)); 2];
        let on = [true, false];
        let none = [false, false];
        // Trigger alone never makes a pointer.
        let tap = [hand(1.0, (0.0, 0.0)), hand(0.0, (0.0, 0.0))];
        assert_eq!(g.route(&tap, on, none, false, |_, _| {}), [Role::Pad, Role::Pad]);
        // The watch arms it; it survives until the hand has reached a screen
        // and left it again.
        g.toggle_pointer(0);
        assert_eq!(g.route(&idle, none, none, false, |_, _| {})[0], Role::Pointer);
        assert_eq!(g.route(&idle, on, none, false, |_, _| {})[0], Role::Pointer);
        assert_eq!(g.route(&idle, none, none, false, |_, _| {})[0], Role::Pointer);
        g.off_screen_since[0] = Some(Instant::now() - Duration::from_secs(2));
        assert_eq!(g.route(&idle, none, none, false, |_, _| {})[0], Role::Pad);
        // Armed but never reaching a screen: gives up after the timeout.
        g.toggle_pointer(1);
        g.armed_at[1] = Some(Instant::now() - Duration::from_secs(10));
        assert_eq!(g.route(&idle, none, none, false, |_, _| {})[1], Role::Pad);
    }

    #[test]
    fn keys_press_and_release_on_edges() {
        let mut g = GameMode::new(DockMode::World, false, None);
        g.enabled = true;
        g.profiles = vec![Profile::keyboard_mouse_example()];
        g.profile = 0;
        let mut log = Vec::new();
        let fwd = [hand(0.0, (0.0, 1.0)), hand(0.0, (0.0, 0.0))];
        g.route(&fwd, [false; 2], [false; 2], false, |k, d| log.push((k, d)));
        assert_eq!(log, vec![(17, true)]);
        let idle = [hand(0.0, (0.0, 0.0)); 2];
        g.route(&idle, [false; 2], [false; 2], false, |k, d| log.push((k, d)));
        assert_eq!(log, vec![(17, true), (17, false)]);
    }
}
