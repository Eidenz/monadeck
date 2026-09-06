//! WayVR-style desktop viewer for the overlay: mirror the user's monitors into
//! VR as grabbable quad layers and drive a virtual mouse/keyboard from the laser.
//!
//! Pipeline: xdg-desktop-portal ScreenCast (`portal`) → PipeWire video node
//! (`pw`) → DMA-BUF import + blit into an OpenXR swapchain (`dmabuf`, `screen`)
//! → quad layer. Interaction: laser hit (u, v) → desktop-logical coordinates via
//! the monitor layout (`outputs`) → absolute uinput mouse (`hid`). The VR
//! keyboard (`keyboard`) types through the same uinput device, labelled from the
//! user's xkb keymap (`keymap`).
//!
//! Screens are independent of the dashboard: once shown they persist while the
//! dashboard is dismissed and while a game runs, exactly like WayVR.
pub mod clipboard;
pub mod dmabuf;
pub mod hid;
pub mod keyboard;
pub mod keymap;
pub mod outputs;
pub mod portal;
pub mod pw;
pub mod screen;
pub mod selftest;

use std::sync::mpsc::Receiver;
use std::sync::Mutex;

use ash::vk;
use openxr as xr;

use crate::mathx::{front_pose, offset_pose, pose_compose, pose_invert, qf, quat_rotate, raycast};
use monadeck_core::desktop_layouts::{DesktopLayout, KeyboardPlacement, ScreenPlacement};
use dmabuf::{Caps, Importer};
use hid::UInput;
use keyboard::{KeyAction, KeyboardState};
use screen::ScreenPanel;

const GRAB_START: f32 = 0.40;
const GRAB_RELEASE: f32 = 0.15;
/// Wheel notches per frame at full thumbstick deflection.
const SCROLL_SPEED: f32 = 0.12;
/// Default distance a newly shown screen is placed at, metres.
const PLACE_DIST: f32 = 1.6;
/// Release a screen within this distance of another screen's side spot to dock.
const SCREEN_DOCK_SNAP: f32 = 0.15;
/// Gap between docked screens, metres.
const SCREEN_DOCK_GAP: f32 = 0.02;
/// Trigger-held cursor motion below this (desktop px) is ignored, so a click
/// on a draggable thing doesn't turn into a drag; beyond it, dragging is live.
const DRAG_THRESHOLD_PX: f64 = 14.0;
/// Resize gesture: width multiplier per metre of hand travel toward/away.
const RESIZE_PER_M: f32 = 3.0;
/// Push/pull speed while gripping (metres per frame at full stick).
const PUSH_SPEED: f32 = 0.006;
/// Curvature change per frame at full stick while gripping with trigger.
const CURVE_SPEED: f32 = 0.012;

/// Per-hand controller state for one frame (only while the overlay is focused).
pub struct HandInput {
    /// Pose located this frame (inactive hands keep their slot for edge tracking).
    pub active: bool,
    pub aim: xr::Posef,
    #[allow(dead_code)]
    pub path: xr::Path,
    pub select: bool,
    pub secondary: bool,
    /// B: a click with the cursor frozen.
    pub precise: bool,
    pub grip: f32,
    pub scroll: (f32, f32),
}

/// One row of the in-headset Desktop settings page.
#[derive(Clone, Debug)]
pub struct ScreenRow {
    pub name: String,
    pub detail: String,
    /// Extra note (e.g. "not approved — re-pick screens to add it").
    pub hint: Option<String>,
    pub shown: bool,
    /// A stream exists for it (portal approved this monitor).
    pub approved: bool,
    pub opacity: f32,
}

pub enum ToggleAll {
    Hidden(usize),
    Shown(usize),
    Nothing,
}

/// Live readout while a screen is gripped (size · distance · curve).
pub struct GestureInfo {
    pub title: String,
    pub body: String,
    /// Just above the screen's top edge.
    pub pose: xr::Posef,
}


/// What the laser did this frame.
#[derive(Default)]
pub struct InputOut {
    /// Laser ray (aim pose, hit distance) when pointing at a screen or the keyboard.
    pub ray: Option<(xr::Posef, f32)>,
    /// Pointer on the keyboard panel: (u, v, trigger down).
    pub keyboard_ptr: Option<(f32, f32, bool)>,
    /// A screen is being gripped/resized: show its numbers.
    pub gesture: Option<GestureInfo>,
}

enum PortalState {
    Idle,
    Pending(Receiver<Result<portal::Cast, String>>),
    Ready(portal::Cast),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Screen(usize),
    Keyboard,
}

pub struct DesktopViewer {
    pub caps: Caps,
    importer: Option<Importer>,
    outputs: Vec<outputs::OutputInfo>,
    hid: Option<UInput>,
    pub hid_error: Option<String>,
    portal: PortalState,
    token: Option<String>,
    token_changed: bool,
    screens: Vec<ScreenPanel>,
    /// Bottom-bar order (output names). Unknown names go last.
    order: Vec<String>,
    pub keyboard: KeyboardState,
    /// What the laser is on this frame.
    pointing: Option<(Target, usize)>,
    /// Screen the laser was last on (keyboard docks there by default).
    last_screen: Option<usize>,
    /// A mouse button held down by (hand, button code) — released on trigger up
    /// even if the laser has left the screen, so nothing gets stuck.
    held: Option<(usize, u16)>,
    /// Where the held button went down + whether motion is already a drag.
    /// `frozen` (B click) never moves the cursor while held.
    held_press: Option<(f64, f64)>,
    dragging: bool,
    frozen: bool,
    hover_prev: Option<(f64, f64)>,
    select_prev: [bool; 2],
    secondary_prev: [bool; 2],
    precise_prev: [bool; 2],
    clipboard: clipboard::ClipboardWatcher,
    pub gaze_pause: bool,
    pending_gpu_teardown: bool,
    width_m: f32,
    keyboard_place: bool,
    /// A layout to apply once the portal has produced the screens.
    pending_layout: Option<DesktopLayout>,
    /// Screens hidden by "toggle all" (double-A), to bring back the same set.
    stash: Vec<usize>,
    stash_keyboard: bool,
    /// Head-relative poses captured at hide time (screen index / keyboard),
    /// used when several undocked screens were up.
    stash_rel: Vec<(usize, xr::Posef)>,
    stash_kb_rel: Option<xr::Posef>,
    /// One screen / one docked group: (root, group centre offset along the
    /// root's right axis, distance from the head) — restored centred in view.
    stash_center: Option<(usize, f32, f32)>,
    stash_kb_attached: Option<usize>,
    /// Restore screens tilted to the headset's pitch (else upright).
    pub restore_tilt: bool,
    /// While a docked group is gripped: the other members' poses relative to
    /// the gripped screen.
    grab_group: Vec<(usize, xr::Posef)>,
    grab_screen: Option<usize>,
    /// A screen shown from the keyboard's pills: dock the (free) keyboard
    /// under it once it's placed.
    keyboard_dock_pending: Option<usize>,
    /// A layout was applied and nothing has been touched since: a double-B
    /// restore puts things back exactly instead of re-centring on the head.
    layout_untouched: bool,
    /// LOCAL space's pose in STAGE space this frame. LOCAL is re-anchored at the
    /// head on every session start, so layouts are stored in STAGE (floor +
    /// tracking origin) and converted through this.
    local_in_stage: Option<xr::Posef>,
}

impl DesktopViewer {
    pub fn new(caps: Caps, importer: Option<Importer>, token: Option<String>, order: Vec<String>) -> Self {
        let outputs = outputs::list();
        for o in &outputs {
            log::info!(
                "desktop: output {} ({}) at {:?} logical {:?} px {:?}",
                o.name,
                o.description,
                o.logical_pos,
                o.logical_size,
                o.pixel_size
            );
        }
        let (hid, hid_error) = match UInput::open() {
            Ok(mut h) => {
                let (origin, extent) = desktop_bounds(&outputs);
                h.set_desktop(origin, extent);
                (Some(h), None)
            }
            Err(e) => {
                log::warn!("desktop: uinput unavailable ({e}); screens will be view-only");
                (None, Some(e.to_string()))
            }
        };
        Self {
            caps,
            importer,
            outputs,
            hid,
            hid_error,
            portal: PortalState::Idle,
            token,
            token_changed: false,
            screens: Vec::new(),
            order,
            keyboard: KeyboardState::new(),
            pointing: None,
            last_screen: None,
            held: None,
            held_press: None,
            dragging: false,
            frozen: false,
            hover_prev: None,
            select_prev: [false; 2],
            secondary_prev: [false; 2],
            precise_prev: [false; 2],
            clipboard: clipboard::ClipboardWatcher::new(),
            gaze_pause: true,
            pending_gpu_teardown: false,
            width_m: screen::DEFAULT_WIDTH_M,
            keyboard_place: false,
            pending_layout: None,
            local_in_stage: None,
            stash: Vec::new(),
            stash_keyboard: false,
            stash_rel: Vec::new(),
            stash_kb_rel: None,
            stash_center: None,
            stash_kb_attached: None,
            restore_tilt: false,
            grab_group: Vec::new(),
            grab_screen: None,
            keyboard_dock_pending: None,
            layout_untouched: false,
        }
    }

    /// Double-A: hide every shown screen (remembering the set), or bring that
    /// set back. Returns what happened for feedback.
    // --- Screen docking ----------------------------------------------------------

    /// Every screen connected to `i` through dock links (including `i`).
    fn group_of(&self, i: usize) -> Vec<usize> {
        let n = self.screens.len();
        let mut members = vec![i];
        let mut changed = true;
        while changed {
            changed = false;
            for j in 0..n {
                if members.contains(&j) {
                    continue;
                }
                let linked = self.screens[j].dock_parent.is_some_and(|(p, _)| members.contains(&p))
                    || members.iter().any(|&m| self.screens[m].dock_parent.is_some_and(|(p, _)| p == j));
                if linked {
                    members.push(j);
                    changed = true;
                }
            }
        }
        members
    }

    /// Where a screen of width `w` sits when docked on `side` of `parent`.
    fn dock_pose_for(parent: &ScreenPanel, side: i8, w: f32) -> xr::Posef {
        let (pw, _) = parent.size_m();
        offset_pose(&parent.pose, side as f32 * (pw / 2.0 + SCREEN_DOCK_GAP + w / 2.0), 0.0, 0.0)
    }

    /// Re-derive docked children's poses from their parents (a few passes so
    /// chains converge). `skip`: screens being driven by a grab right now.
    fn derive_docked_poses(&mut self, skip: &[usize]) {
        for _ in 0..self.screens.len().max(1) {
            for i in 0..self.screens.len() {
                if skip.contains(&i) {
                    continue;
                }
                if let Some((p, side)) = self.screens[i].dock_parent {
                    if p == i || p >= self.screens.len() {
                        self.screens[i].dock_parent = None;
                        continue;
                    }
                    let w = self.screens[i].size_m().0;
                    let pose = Self::dock_pose_for(&self.screens[p], side, w);
                    self.screens[i].pose = pose;
                    self.screens[i].placed = true;
                }
            }
        }
    }

    /// Offset of a screen's centre along `root`'s right axis.
    fn x_along(&self, root: usize, i: usize) -> f32 {
        let r = &self.screens[root].pose;
        let right = quat_rotate(qf(&r.orientation), [1.0, 0.0, 0.0]);
        let p = self.screens[i].pose.position;
        (p.x - r.position.x) * right[0] + (p.y - r.position.y) * right[1] + (p.z - r.position.z) * right[2]
    }

    /// Rebuild a group's dock chain with `root` in the middle: members are
    /// sorted along the root's right axis and chained outwards on each side.
    fn rebuild_chain(&mut self, root: usize, members: &[usize]) {
        let mut xs: Vec<(usize, f32)> = members.iter().filter(|&&m| m != root).map(|&m| (m, self.x_along(root, m))).collect();
        xs.sort_by(|a, b| a.1.total_cmp(&b.1));
        self.screens[root].dock_parent = None;
        let mut prev_right = root;
        for &(m, x) in xs.iter().filter(|(_, x)| *x >= 0.0) {
            let _ = x;
            self.screens[m].dock_parent = Some((prev_right, 1));
            prev_right = m;
        }
        let mut prev_left = root;
        for &(m, x) in xs.iter().rev().filter(|(_, x)| *x < 0.0) {
            let _ = x;
            self.screens[m].dock_parent = Some((prev_left, -1));
            prev_left = m;
        }
        self.derive_docked_poses(&[]);
    }

    /// The group's centre offset along the root's right axis (edges included).
    fn group_center_x(&self, root: usize, members: &[usize]) -> f32 {
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for &m in members {
            let x = self.x_along(root, m);
            let w = self.screens[m].size_m().0;
            lo = lo.min(x - w / 2.0);
            hi = hi.max(x + w / 2.0);
        }
        if lo > hi {
            0.0
        } else {
            (lo + hi) / 2.0
        }
    }

    /// Try to dock free screen `s` next to any other shown screen it was
    /// released close to (outermost on that side).
    fn try_snap_screen(&mut self, s: usize) {
        let own_group = self.group_of(s);
        let w = self.screens[s].size_m().0;
        let sp = self.screens[s].pose.position;
        let mut best: Option<(usize, i8, f32, xr::Posef)> = None;
        for t in 0..self.screens.len() {
            if own_group.contains(&t) || !self.screens[t].shown || !self.screens[t].placed {
                continue;
            }
            for side in [-1i8, 1] {
                // Walk to the outermost screen on that side.
                let mut outer = t;
                loop {
                    match (0..self.screens.len()).find(|&c| self.screens[c].dock_parent == Some((outer, side))) {
                        Some(c) if c != s => outer = c,
                        _ => break,
                    }
                }
                let pose = Self::dock_pose_for(&self.screens[outer], side, w);
                let d = dist2(&sp, &pose.position).sqrt();
                if d < SCREEN_DOCK_SNAP && best.map_or(true, |b| d < b.2) {
                    best = Some((outer, side, d, pose));
                }
            }
        }
        if let Some((outer, side, _, pose)) = best {
            self.screens[s].dock_parent = Some((outer, side));
            self.screens[s].pose = pose;
            log::info!("desktop: {} docked {} of {}", self.screens[s].name, if side < 0 { "left" } else { "right" }, self.screens[outer].name);
            self.derive_docked_poses(&[]);
        }
    }

    /// Detach `s` from its group (its own docked children come along).
    fn undock_screen(&mut self, s: usize) {
        let members = self.group_of(s);
        // s (and its subtree) leave; the rest re-chain around their old root.
        let mut subtree = vec![s];
        let mut changed = true;
        while changed {
            changed = false;
            for j in 0..self.screens.len() {
                if !subtree.contains(&j) && self.screens[j].dock_parent.is_some_and(|(p, _)| subtree.contains(&p)) {
                    subtree.push(j);
                    changed = true;
                }
            }
        }
        self.screens[s].dock_parent = None;
        let rest: Vec<usize> = members.into_iter().filter(|m| !subtree.contains(m)).collect();
        if let Some(&root) = rest.iter().find(|&&m| self.screens[m].dock_parent.is_none()).or(rest.first()) {
            self.rebuild_chain(root, &rest);
        }
        log::info!("desktop: {} undocked", self.screens[s].name);
    }

    /// Double-B: hide every shown screen (remembering the arrangement), or
    /// bring it back. One screen / one docked group comes back centred in view
    /// like the menu; several undocked screens come back where they sat
    /// relative to your head.
    pub fn toggle_all(&mut self, hmd: Option<&xr::Posef>, recenter: bool) -> ToggleAll {
        let shown: Vec<usize> = self.screens.iter().enumerate().filter(|(_, s)| s.shown).map(|(i, _)| i).collect();
        if !shown.is_empty() || self.keyboard.visible {
            self.stash_rel.clear();
            self.stash_kb_rel = None;
            self.stash_center = None;
            self.stash_kb_attached = self.keyboard.attached.filter(|_| self.keyboard.visible);
            if let (Some(h), true) = (hmd, recenter) {
                // One group (or one screen)?
                let first_group = shown.first().map(|&i| self.group_of(i)).unwrap_or_default();
                let one_group = !shown.is_empty() && shown.iter().all(|i| first_group.contains(i));
                if one_group {
                    let root = shown.iter().copied().find(|&i| self.screens[i].dock_parent.is_none()).unwrap_or(shown[0]);
                    let cx = self.group_center_x(root, &shown);
                    let center = offset_pose(&self.screens[root].pose, cx, 0.0, 0.0);
                    let dist = dist2(&center.position, &h.position).sqrt().max(0.3);
                    self.stash_center = Some((root, cx, dist));
                } else {
                    let inv = pose_invert(h);
                    for &i in &shown {
                        self.stash_rel.push((i, pose_compose(&inv, &self.screens[i].pose)));
                    }
                }
                if self.keyboard.visible && self.keyboard.attached.is_none() {
                    self.stash_kb_rel = Some(pose_compose(&pose_invert(h), &self.keyboard.pose));
                }
            }
            for &i in &shown {
                self.screens[i].hide(); // keeps `placed` + docking
            }
            self.stash_keyboard = self.keyboard.visible;
            self.keyboard.visible = false;
            self.keyboard.grab = None;
            self.clipboard.set_active(false);
            self.stash = shown;
            return ToggleAll::Hidden(self.stash.len() + self.stash_keyboard as usize);
        }
        let stash = std::mem::take(&mut self.stash);
        let valid: Vec<usize> = stash.into_iter().filter(|&i| i < self.screens.len()).collect();
        let kb = std::mem::take(&mut self.stash_keyboard);
        if valid.is_empty() && !kb {
            return ToggleAll::Nothing;
        }
        let rel = std::mem::take(&mut self.stash_rel);
        let center = self.stash_center.take();
        let kb_rel = self.stash_kb_rel.take();
        let kb_attached = self.stash_kb_attached.take();
        let recentre_now = recenter && !self.layout_untouched;
        if let (Some(h), true) = (hmd, recentre_now) {
            if let Some((root, cx, dist)) = center {
                // Menu logic: the group's centre lands in front of you.
                let c = front_pose(h, dist, 0.0, 0.0, self.restore_tilt);
                if root < self.screens.len() {
                    self.screens[root].pose = offset_pose(&c, -cx, 0.0, 0.0);
                    self.screens[root].placed = true;
                }
            } else {
                for (i, r) in &rel {
                    if let Some(s) = self.screens.get_mut(*i) {
                        s.pose = pose_compose(h, r);
                    }
                }
            }
            if let Some(r) = kb_rel {
                self.keyboard.pose = pose_compose(h, &r);
            }
        }
        for &i in &valid {
            self.screens[i].show(&self.caps);
        }
        self.derive_docked_poses(&[]);
        if kb {
            self.keyboard.visible = true;
            self.clipboard.set_active(true);
            match kb_attached {
                Some(i) if self.screens.get(i).is_some_and(|s| s.shown) => self.dock_keyboard(Some(i)),
                _ => self.keyboard.attached = None,
            }
        }
        ToggleAll::Shown(valid.len() + kb as usize)
    }

    /// Which hand the laser is on a screen/keyboard with this frame.
    pub fn pointing_hand(&self) -> Option<usize> {
        self.pointing.map(|(_, h)| h)
    }

    /// Per-frame: where LOCAL sits in STAGE (None if the runtime has no STAGE).
    pub fn set_local_in_stage(&mut self, p: Option<xr::Posef>) {
        self.local_in_stage = p;
    }

    fn to_stage(&self, p: &xr::Posef) -> xr::Posef {
        match &self.local_in_stage {
            Some(l) => pose_compose(l, p),
            None => *p,
        }
    }

    fn from_stage(&self, p: &xr::Posef) -> xr::Posef {
        match &self.local_in_stage {
            Some(l) => pose_compose(&pose_invert(l), p),
            None => *p,
        }
    }

    // --- Layouts (named arrangements) ------------------------------------------

    /// The current arrangement: every approved screen (shown or not), placed
    /// ones with their pose/size/curve, plus the keyboard.
    pub fn snapshot(&self, name: String) -> DesktopLayout {
        if self.local_in_stage.is_none() {
            log::warn!("desktop: no STAGE space; layout saved in LOCAL space (may drift between sessions)");
        }
        // Only screens that have a real pose; unplaced ones would save the origin.
        let screens = self
            .screens
            .iter()
            .filter(|s| s.placed)
            .map(|s| ScreenPlacement {
                name: s.name.clone(),
                shown: s.shown,
                pose: pose_to_arr(&self.to_stage(&s.pose)),
                width_m: s.width_m,
                curve: s.curve,
                opacity: s.opacity,
                docked_to: s.dock_parent.and_then(|(p, side)| self.screens.get(p).map(|ps| (ps.name.clone(), side))),
            })
            .collect();
        let kb = &self.keyboard;
        let keyboard = kb.placed.then(|| KeyboardPlacement {
            visible: kb.visible,
            attached: kb.attached.and_then(|i| self.screens.get(i)).map(|s| s.name.clone()),
            pose: pose_to_arr(&self.to_stage(&kb.pose)),
            scale: kb.scale,
        });
        DesktopLayout { name, screens, keyboard, recenter_on_toggle: false }
    }

    /// Apply an arrangement. Screens the layout doesn't mention are hidden.
    /// Before the portal has answered, it's kept and applied when it does.
    pub fn apply(&mut self, layout: &DesktopLayout) {
        if self.screens.is_empty() || self.local_in_stage.is_none() {
            // Wait for the streams and for the STAGE relation (poll applies it).
            self.pending_layout = Some(layout.clone());
            if matches!(self.portal, PortalState::Idle | PortalState::Failed(_)) {
                self.start_portal();
            }
            return;
        }
        let from_stage = |a: &[f32; 7], me: &Self| me.from_stage(&arr_to_pose(a));
        let placements: Vec<(String, xr::Posef)> =
            layout.screens.iter().map(|p| (p.name.clone(), from_stage(&p.pose, self))).collect();
        for s in &mut self.screens {
            match layout.screens.iter().find(|p| p.name == s.name) {
                Some(p) => {
                    s.pose = placements.iter().find(|(n, _)| n == &s.name).map(|(_, q)| *q).unwrap_or(s.pose);
                    s.width_m = p.width_m.clamp(0.3, 4.0);
                    s.curve = p.curve.clamp(0.0, 1.0);
                    s.opacity = p.opacity.clamp(0.2, 1.0);
                    s.custom_size = true;
                    s.placed = true;
                    if p.shown && !s.shown {
                        s.show(&self.caps);
                    } else if !p.shown && s.shown {
                        s.hide();
                    }
                }
                None => {
                    if s.shown {
                        s.hide();
                    }
                }
            }
        }
        // Docking links (by name), then derive the children from their parents.
        for s in &mut self.screens {
            s.dock_parent = None;
        }
        for p in &layout.screens {
            if let Some((pname, side)) = &p.docked_to {
                let child = self.screens.iter().position(|s| s.name == p.name);
                let parent = self.screens.iter().position(|s| &s.name == pname);
                if let (Some(c), Some(pa)) = (child, parent) {
                    if c != pa {
                        self.screens[c].dock_parent = Some((pa, *side));
                    }
                }
            }
        }
        self.derive_docked_poses(&[]);
        match &layout.keyboard {
            Some(k) => {
                self.keyboard.visible = k.visible;
                self.keyboard.grab = None;
                self.keyboard.pose = self.from_stage(&arr_to_pose(&k.pose));
                self.keyboard.scale = k.scale.clamp(0.5, 2.0);
                self.keyboard.placed = true;
                self.keyboard_place = false;
                let target = k.attached.as_ref().and_then(|n| self.screens.iter().position(|s| &s.name == n));
                match target {
                    Some(i) if self.screens[i].shown => self.dock_keyboard(Some(i)),
                    _ => self.keyboard.attached = None,
                }
                self.clipboard.set_active(self.keyboard.visible);
            }
            None => {
                self.keyboard.visible = false;
                self.keyboard.attached = None;
                self.clipboard.set_active(false);
            }
        }
        // A "follows head" layout behaves like an unsaved arrangement.
        self.layout_untouched = !layout.recenter_on_toggle;
    }

    /// Default width: applies to screens that were never sized by hand.
    pub fn set_width(&mut self, w: f32) {
        self.width_m = w.clamp(0.4, 4.0);
        for s in &mut self.screens.iter_mut().filter(|s| !s.custom_size) {
            s.width_m = self.width_m;
        }
    }

    /// Apply capture limits; a changed fps cap restarts live captures.
    pub fn set_capture_limits(&mut self, max_fps: u32, max_height: u32) {
        let fps_changed = self.caps.max_fps != max_fps;
        self.caps.max_fps = max_fps;
        self.caps.max_height = max_height;
        if fps_changed {
            let caps = self.caps.clone();
            for s in &mut self.screens {
                s.restart_capture(&caps);
            }
        }
    }

    /// Per-screen opacity, by Desktop-page row.
    pub fn set_screen_opacity(&mut self, row: usize, opacity: f32) {
        if let Some(&si) = self.ordered_screens().get(row) {
            self.screens[si].opacity = opacity.clamp(0.2, 1.0);
        }
    }

    // --- Ordering ------------------------------------------------------------

    fn order_key(&self, name: &str) -> usize {
        self.order.iter().position(|n| n == name).unwrap_or(usize::MAX)
    }

    /// Indices of approved screens in bottom-bar order.
    fn ordered_screens(&self) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.screens.len()).collect();
        idx.sort_by_key(|&i| (self.order_key(&self.screens[i].name), i));
        idx
    }

    pub fn order(&self) -> Vec<String> {
        self.order.clone()
    }

    /// Move the approved screen at bar position `i` by `delta` (-1 left, +1
    /// right). Returns true when the order changed (persist it).
    pub fn move_order(&mut self, i: usize, delta: i32) -> bool {
        let mut names: Vec<String> = self.ordered_screens().iter().map(|&s| self.screens[s].name.clone()).collect();
        let j = i as i32 + delta;
        if i >= names.len() || j < 0 || j as usize >= names.len() {
            return false;
        }
        names.swap(i, j as usize);
        self.order = names;
        true
    }

    // --- UI-facing state -----------------------------------------------------

    /// Approved screens for the bottom bar: (name, shown), in order.
    pub fn bar_items(&self) -> Vec<(String, bool)> {
        self.ordered_screens().iter().map(|&i| (self.screens[i].name.clone(), self.screens[i].shown)).collect()
    }

    /// Show/hide the screen at bar position `i`.
    pub fn toggle_bar(&mut self, i: usize) {
        self.layout_untouched = false;
        let Some(&si) = self.ordered_screens().get(i) else { return };
        if self.screens[si].shown {
            self.screens[si].hide();
            // Off then on = a fresh spawn in front of you (not the old spot).
            self.screens[si].placed = false;
            if self.keyboard.attached == Some(si) {
                self.keyboard.attached = None;
            }
        } else {
            self.screens[si].show(&self.caps);
        }
    }

    pub fn rows(&self) -> Vec<ScreenRow> {
        let ready = matches!(self.portal, PortalState::Ready(_));
        let mut rows: Vec<ScreenRow> = self
            .ordered_screens()
            .iter()
            .map(|&i| {
                let s = &self.screens[i];
                ScreenRow {
                    name: s.name.clone(),
                    detail: s.detail.clone(),
                    hint: None,
                    shown: s.shown,
                    approved: true,
                    opacity: s.opacity,
                }
            })
            .collect();
        for o in &self.outputs {
            if !self.screens.iter().any(|s| s.name == o.name) {
                rows.push(ScreenRow {
                    name: o.name.clone(),
                    detail: output_detail(o),
                    hint: Some(if ready {
                        "Not approved — re-pick screens to add it".into()
                    } else {
                        "Set up screens to approve it".into()
                    }),
                    shown: false,
                    approved: false,
                    opacity: 1.0,
                });
            }
        }
        rows
    }

    pub fn status(&self) -> String {
        match &self.portal {
            PortalState::Idle => {
                if self.outputs.is_empty() {
                    "No Wayland outputs found — the screen-share dialog will list what's available.".into()
                } else {
                    "Set up screens once: approve the share dialog on your desktop (tick every monitor you want).".into()
                }
            }
            PortalState::Pending(_) => "Waiting for the screen-share dialog on your desktop…".into(),
            PortalState::Ready(c) => {
                let mode = if self.caps.dmabuf { "GPU (DMA-BUF)" } else { "CPU (SHM)" };
                format!("{} screen(s) approved · capture: {mode} · toggle them from the bottom bar", c.streams.len())
            }
            PortalState::Failed(e) => format!("Screen share failed: {e}"),
        }
    }

    pub fn portal_ready(&self) -> bool {
        matches!(self.portal, PortalState::Ready(_))
    }

    pub fn portal_pending(&self) -> bool {
        matches!(self.portal, PortalState::Pending(_))
    }

    pub fn shown_count(&self) -> usize {
        self.screens.iter().filter(|s| s.shown).count()
    }

    /// Ask the portal for screens (no-op while a request is in flight or done).
    pub fn setup_screens(&mut self) {
        if matches!(self.portal, PortalState::Idle | PortalState::Failed(_)) {
            self.start_portal();
        }
    }

    /// Forget the saved approval and ask the user to pick screens again.
    pub fn reselect(&mut self) {
        self.teardown_screens();
        self.token = None;
        self.token_changed = true;
        self.start_portal();
    }

    fn start_portal(&mut self) {
        self.portal = PortalState::Pending(portal::start(self.token.clone()));
    }

    fn teardown_screens(&mut self) {
        // GPU resources are freed via destroy_gpu in poll (needs the device);
        // here we just stop captures and drop the session.
        for s in &mut self.screens {
            s.hide();
            s.capture = None;
        }
        self.keyboard.attached = None;
        self.pending_gpu_teardown = true;
        self.portal = PortalState::Idle;
    }

    /// `Some(token)` once when the persisted restore token should change.
    pub fn take_token_change(&mut self) -> Option<Option<String>> {
        if self.token_changed {
            self.token_changed = false;
            Some(self.token.clone())
        } else {
            None
        }
    }

    // --- Keyboard --------------------------------------------------------------

    pub fn toggle_keyboard(&mut self) {
        self.layout_untouched = false;
        self.keyboard.visible = !self.keyboard.visible;
        if self.keyboard.visible {
            self.keyboard_place = true;
        } else {
            self.keyboard.grab = None;
        }
        self.clipboard.set_active(self.keyboard.visible);
    }

    pub fn keyboard_visible(&self) -> bool {
        self.keyboard.visible
    }

    /// Dock the keyboard under `si` (or the nearest shown screen when None).
    fn dock_keyboard(&mut self, si: Option<usize>) {
        let target = si.filter(|&i| self.screens.get(i).is_some_and(|s| s.shown)).or_else(|| {
            let kp = self.keyboard.pose.position;
            self.screens
                .iter()
                .enumerate()
                .filter(|(_, s)| s.shown)
                .map(|(i, s)| {
                    let d = KeyboardState::dock_pose_scaled(&s.pose, s.size_m(), self.keyboard.scale).position;
                    (i, dist2(&kp, &d))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        });
        if let Some(i) = target {
            self.keyboard.attached = Some(i);
            self.keyboard.pose =
                KeyboardState::dock_pose_scaled(&self.screens[i].pose, self.screens[i].size_m(), self.keyboard.scale);
            self.keyboard.placed = true;
        }
    }

    /// Type queued keys and apply the keyboard's top-bar requests. Call after
    /// the keyboard panel has been rendered for the frame.
    pub fn flush_keyboard(&mut self) {
        let pending = std::mem::take(&mut self.keyboard.pending);
        if let Some(hid) = &mut self.hid {
            for a in pending {
                match a {
                    KeyAction::Tap { code, mods } => {
                        let held: Vec<u16> = [
                            keyboard::MOD_SHIFT,
                            keyboard::MOD_CTRL,
                            keyboard::MOD_ALT,
                            keyboard::MOD_SUPER,
                            keyboard::MOD_ALTGR,
                        ]
                        .into_iter()
                        .filter(|m| mods & m != 0)
                        .map(keyboard::mod_code)
                        .collect();
                        for &m in &held {
                            hid.key(m, true);
                        }
                        hid.key(code, true);
                        hid.key(code, false);
                        for &m in held.iter().rev() {
                            hid.key(m, false);
                        }
                    }
                    KeyAction::Toggle(code) => {
                        hid.key(code, true);
                        hid.key(code, false);
                    }
                }
            }
        }
        if self.keyboard.close_request {
            self.keyboard.close_request = false;
            self.keyboard.visible = false;
            self.keyboard.grab = None;
            self.clipboard.set_active(false);
        }
        if let Some(i) = self.keyboard.screen_toggle_request.take() {
            let was_shown = self.ordered_screens().get(i).is_some_and(|&si| self.screens[si].shown);
            self.toggle_bar(i);
            // Spawned from the keyboard with no docked screen: dock under it.
            if !was_shown && self.keyboard.attached.is_none() {
                self.keyboard_dock_pending = self.ordered_screens().get(i).copied();
            }
        }
        if let Some(i) = self.keyboard.layout_switch_request.take() {
            if i < self.keyboard.labels.layout_names.len() {
                // Relabel regardless; KDE follows when it's the desktop.
                self.keyboard.labels.current = i;
                if !keymap::kde_set_layout(i) {
                    log::warn!("desktop: layout switch only relabelled the VR keyboard");
                }
            }
        }
        self.keyboard.clipboard = self.clipboard.latest();
        if self.keyboard.detach_request {
            self.keyboard.detach_request = false;
            self.keyboard.attached = None;
        }
        if self.keyboard.attach_request {
            self.keyboard.attach_request = false;
            self.dock_keyboard(self.last_screen);
        }
    }

    // --- Per-frame -------------------------------------------------------------

    /// Drive the portal, build screens when streams arrive, place newly shown
    /// screens / the keyboard, and upload the newest frames. Call every frame.
    #[allow(clippy::too_many_arguments)]
    pub fn poll(
        &mut self,
        session: &xr::Session<xr::Vulkan>,
        device: &ash::Device,
        allocator: &Mutex<gpu_allocator::vulkan::Allocator>,
        cmd: vk::CommandBuffer,
        queue: vk::Queue,
        fence: vk::Fence,
        hmd: Option<&xr::Posef>,
    ) {
        if self.pending_gpu_teardown {
            self.pending_gpu_teardown = false;
            for s in &mut self.screens {
                s.destroy_gpu(device, allocator);
            }
            self.screens.clear();
            self.last_screen = None;
        }
        if let PortalState::Pending(rx) = &self.portal {
            match rx.try_recv() {
                Ok(Ok(cast)) => {
                    if cast.restore_token != self.token {
                        self.token = cast.restore_token.clone();
                        self.token_changed = true;
                    }
                    self.build_screens(&cast);
                    self.portal = PortalState::Ready(cast);
                }
                Ok(Err(e)) => {
                    log::error!("desktop: {e}");
                    self.portal = PortalState::Failed(e);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.portal = PortalState::Failed("portal thread died".into());
                }
            }
        }
        // A queued layout applies once the screens exist and STAGE is known.
        if self.pending_layout.is_some() && !self.screens.is_empty() && self.local_in_stage.is_some() {
            if let Some(l) = self.pending_layout.take() {
                log::info!("desktop: applying layout '{}'", l.name);
                self.apply(&l);
            }
        }
        for s in &mut self.screens {
            if !s.shown {
                continue;
            }
            if !s.placed {
                if let Some(h) = hmd {
                    s.pose = front_pose(h, PLACE_DIST, 0.0, 0.0, false);
                    s.placed = true;
                }
            }
            s.gaze_update(hmd, self.gaze_pause);
            if let Err(e) = s.upload(session, device, allocator, self.importer.as_ref(), &self.caps, cmd, queue, fence) {
                log::error!("desktop: {} upload: {e}", s.name);
            }
        }
        if let Some(si) = self.keyboard_dock_pending {
            if self.screens.get(si).is_some_and(|s| s.shown && s.placed) {
                self.keyboard_dock_pending = None;
                self.dock_keyboard(Some(si));
            } else if !self.screens.get(si).is_some_and(|s| s.shown) {
                self.keyboard_dock_pending = None;
            }
        }
        // Docked screens follow their parents (unless a grab is driving the group).
        let driven: Vec<usize> = self.grab_screen.into_iter().chain(self.grab_group.iter().map(|(m, _)| *m)).collect();
        self.derive_docked_poses(&driven);
        self.keyboard.screens = self.bar_items();
        // Keyboard: first placement, then follow its dock.
        if self.keyboard.visible && self.keyboard_place {
            self.keyboard_place = false;
            if self.screens.iter().any(|s| s.shown) {
                self.dock_keyboard(self.last_screen);
            } else if let Some(h) = hmd {
                self.keyboard.pose = front_pose(h, 1.1, 0.0, -0.35, false);
                self.keyboard.attached = None;
                self.keyboard.placed = true;
            }
        }
        if let Some(i) = self.keyboard.attached {
            match self.screens.get(i) {
                Some(s) if s.shown && self.keyboard.grab.is_none() => {
                    self.keyboard.pose = KeyboardState::dock_pose_scaled(&s.pose, s.size_m(), self.keyboard.scale);
                }
                _ => self.keyboard.attached = None,
            }
        }
    }

    fn build_screens(&mut self, cast: &portal::Cast) {
        self.screens.clear();
        for (i, st) in cast.streams.iter().enumerate() {
            // Link the stream to an output by connector name, else by geometry.
            let out = self
                .outputs
                .iter()
                .find(|o| st.mapping_id.as_deref() == Some(o.name.as_str()))
                .or_else(|| {
                    self.outputs.iter().find(|o| st.position == Some(o.logical_pos) && st.size == Some(o.logical_size))
                })
                .or_else(|| {
                    if self.outputs.len() == 1 && cast.streams.len() == 1 {
                        self.outputs.first()
                    } else {
                        None
                    }
                });
            let (name, detail, rect) = match out {
                Some(o) => (
                    o.name.clone(),
                    output_detail(o),
                    (o.logical_pos.0 as f64, o.logical_pos.1 as f64, o.logical_size.0 as f64, o.logical_size.1 as f64),
                ),
                None => {
                    let pos = st.position.unwrap_or((0, 0));
                    let size = st.size.unwrap_or((1920, 1080));
                    (
                        st.mapping_id.clone().unwrap_or_else(|| format!("Screen {}", i + 1)),
                        format!("{}×{}", size.0, size.1),
                        (pos.0 as f64, pos.1 as f64, size.0 as f64, size.1 as f64),
                    )
                }
            };
            log::info!("desktop: stream node {} -> {name} {detail} rect {rect:?}", st.node_id);
            let mut panel = ScreenPanel::new(name, detail, st.node_id, rect);
            panel.width_m = self.width_m;
            self.screens.push(panel);
        }
        // Screens the order list doesn't know yet go to the end, in stream order.
        for s in &self.screens {
            if !self.order.contains(&s.name) {
                self.order.push(s.name.clone());
            }
        }
        // With no Wayland output list, derive the pointer bounds from the streams.
        if self.outputs.is_empty() {
            if let Some(h) = &mut self.hid {
                let rects: Vec<(f64, f64, f64, f64)> = self.screens.iter().map(|s| s.rect).collect();
                let (origin, extent) = bounds_of(&rects);
                h.set_desktop(origin, extent);
            }
        }
    }

    /// Laser interaction with the shown screens + keyboard. `max_t`: distance
    /// of a closer dashboard hit (things behind it are ignored).
    pub fn update_input(&mut self, hands: &[HandInput], max_t: Option<f32>, hmd: Option<&xr::Posef>) -> InputOut {
        let mut out = InputOut::default();
        self.pointing = None;
        let curved_ok = self.caps.curved;

        // Continue grabs first (the grabbed thing follows the hand). While
        // gripping: trigger + push/pull the hand resizes, trigger + stick ◀▶
        // curves, stick ▲▼ pushes the screen away/closer (WayVR's gestures).
        if let Some(si) = self.grab_screen {
            let grab = self.screens[si].grab;
            match (grab, grab.and_then(|(hand, _)| hands.get(hand))) {
                (Some((hand, mut offset)), Some(h)) if h.active && h.grip >= GRAB_RELEASE => {
                    // B while gripping: detach this screen from its docked group.
                    if h.precise && !self.precise_prev[hand] && !self.grab_group.is_empty() {
                        self.undock_screen(si);
                        self.grab_group.clear();
                    }
                    let (sx, sy) = h.scroll;
                    let s = &mut self.screens[si];
                    if h.select {
                        let d = hmd.map_or(0.0, |m| dist2(&h.aim.position, &m.position).sqrt());
                        match s.resize_ref {
                            None => s.resize_ref = Some((d, s.width_m)),
                            Some((d0, w0)) => {
                                s.width_m = (w0 * (1.0 + (d - d0) * RESIZE_PER_M)).clamp(0.3, 4.0);
                                s.custom_size = true;
                            }
                        }
                        if curved_ok && sx != 0.0 {
                            s.curve = (s.curve + sx * CURVE_SPEED).clamp(0.0, 1.0);
                        }
                        // Position stays put while resizing; re-anchor for when
                        // the trigger lets go.
                        offset = pose_compose(&pose_invert(&h.aim), &s.pose);
                    } else {
                        s.resize_ref = None;
                        if sy != 0.0 {
                            // Hand-local -Z is forward along the aim.
                            offset.position.z -= sy * PUSH_SPEED;
                        }
                        s.pose = pose_compose(&h.aim, &offset);
                    }
                    s.grab = Some((hand, offset));
                    // The rest of a docked group rides along rigidly.
                    let root_pose = self.screens[si].pose;
                    for (m, rel) in self.grab_group.clone() {
                        if let Some(ms) = self.screens.get_mut(m) {
                            ms.pose = pose_compose(&root_pose, &rel);
                        }
                    }
                }
                _ => {
                    // Released: a group re-chains around this screen; a lone
                    // screen may snap onto a neighbour's side.
                    self.screens[si].grab = None;
                    self.screens[si].resize_ref = None;
                    let group: Vec<usize> = std::iter::once(si).chain(self.grab_group.iter().map(|(m, _)| *m)).collect();
                    self.grab_group.clear();
                    self.grab_screen = None;
                    if group.len() > 1 {
                        self.rebuild_chain(si, &group);
                    } else {
                        self.try_snap_screen(si);
                    }
                }
            }
        }
        // Live numbers for the gripped screen.
        if let Some(s) = self.screens.iter().find(|s| s.grab.is_some()) {
            let (w, h) = s.size_m();
            let dist = hmd.map(|m| dist2(&s.pose.position, &m.position).sqrt());
            let deg = s.curve * screen::MAX_CURVE_ANGLE.to_degrees();
            let mut body = format!("{w:.2} × {h:.2} m", );
            if let Some(d) = dist {
                body.push_str(&format!("  ·  {d:.2} m away"));
            }
            if s.curve > 0.01 {
                body.push_str(&format!("  ·  curve {deg:.0}°"));
            } else {
                body.push_str("  ·  flat");
            }
            let above = xr::Posef {
                orientation: s.pose.orientation,
                position: crate::mathx::offset_pose(&s.pose, 0.0, h / 2.0 + 0.10, 0.0).position,
            };
            out.gesture = Some(GestureInfo { title: s.name.clone(), body, pose: above });
        }
        if let Some((hand, offset)) = self.keyboard.grab {
            match hands.get(hand) {
                Some(h) if h.active && h.grip >= GRAB_RELEASE => {
                    self.keyboard.pose = pose_compose(&h.aim, &offset);
                }
                _ => {
                    // Released: snap under a screen if close to its dock.
                    self.keyboard.grab = None;
                    let kp = self.keyboard.pose.position;
                    let near = self
                        .screens
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| s.shown)
                        .map(|(i, s)| {
                            (i, dist2(&kp, &KeyboardState::dock_pose_scaled(&s.pose, s.size_m(), self.keyboard.scale).position))
                        })
                        .filter(|(_, d)| *d < keyboard::DOCK_SNAP_M * keyboard::DOCK_SNAP_M)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(i, _)| i);
                    if let Some(i) = near {
                        self.dock_keyboard(Some(i));
                    }
                }
            }
        }
        let grabbing = self.screens.iter().any(|s| s.grab.is_some()) || self.keyboard.grab.is_some();

        // Closest hit across hands, screens and keyboard.
        let mut best: Option<(Target, usize, f32, f32, f32)> = None; // (target, hand, u, v, t)
        if !grabbing {
            for (hi, h) in hands.iter().enumerate().filter(|(_, h)| h.active) {
                let mut consider = |target: Target, hit: Option<(f32, f32, f32)>| {
                    if let Some((u, v, t)) = hit {
                        if max_t.is_some_and(|m| t >= m) {
                            return;
                        }
                        if best.map_or(true, |b| t < b.4) {
                            best = Some((target, hi, u, v, t));
                        }
                    }
                };
                for (si, s) in self.screens.iter().enumerate() {
                    consider(Target::Screen(si), s.hit(&h.aim, curved_ok));
                }
                if self.keyboard.visible && self.keyboard.placed {
                    consider(Target::Keyboard, raycast(&h.aim, &self.keyboard.pose, keyboard::size_m_scaled(self.keyboard.scale)));
                }
            }
        }

        // Grip while pointing grabs the thing (a grabbed keyboard undocks).
        if let Some((target, hi, _, _, _)) = best {
            if hands[hi].grip > GRAB_START {
                self.layout_untouched = false;
                let aim = hands[hi].aim;
                match target {
                    Target::Screen(si) => {
                        self.screens[si].start_grab(hi, &aim);
                        self.grab_screen = Some(si);
                        let root = self.screens[si].pose;
                        self.grab_group = self
                            .group_of(si)
                            .into_iter()
                            .filter(|&m| m != si)
                            .map(|m| (m, pose_compose(&pose_invert(&root), &self.screens[m].pose)))
                            .collect();
                    }
                    Target::Keyboard => {
                        self.keyboard.grab = Some((hi, pose_compose(&pose_invert(&aim), &self.keyboard.pose)));
                        self.keyboard.attached = None;
                    }
                }
                best = None;
            }
        }

        if let Some((target, hi, u, v, t)) = best {
            self.pointing = Some((target, hi));
            out.ray = Some((hands[hi].aim, t));
            let h = &hands[hi];
            match target {
                Target::Keyboard => {
                    out.keyboard_ptr = Some((u, v, h.select));
                }
                Target::Screen(si) => {
                    self.last_screen = Some(si);
                    let (x, y) = self.screens[si].desktop_pos(u, v);
                    if let Some(hid) = &mut self.hid {
                        // Hover → move (skip sub-pixel jitter). While a button is
                        // held: frozen (B) never moves; trigger waits for a real drag.
                        let moved =
                            self.hover_prev.map_or(true, |(px, py)| (px - x).abs() >= 0.5 || (py - y).abs() >= 0.5);
                        let mut allow_move = !self.frozen;
                        if self.held.is_some() && !self.frozen && !self.dragging {
                            if let Some((px, py)) = self.held_press {
                                let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
                                if d >= DRAG_THRESHOLD_PX {
                                    self.dragging = true;
                                } else {
                                    allow_move = false;
                                }
                            }
                        }
                        if moved && allow_move {
                            hid.mouse_move(x, y);
                            self.hover_prev = Some((x, y));
                        }
                        // Clicks: rising edge of trigger / A / B on the pointing hand.
                        if self.held.is_none() {
                            let (code, frozen) = if h.select && !self.select_prev[hi] {
                                (Some(hid::BTN_LEFT), false)
                            } else if h.precise && !self.precise_prev[hi] {
                                (Some(hid::BTN_LEFT), true)
                            } else if h.secondary && !self.secondary_prev[hi] {
                                (Some(hid::BTN_RIGHT), false)
                            } else {
                                (None, false)
                            };
                            if let Some(code) = code {
                                hid.mouse_move(x, y);
                                self.hover_prev = Some((x, y));
                                hid.button(code, true);
                                self.held = Some((hi, code));
                                self.held_press = Some((x, y));
                                self.dragging = false;
                                self.frozen = frozen;
                            }
                        }
                        // Thumbstick scroll.
                        let (sx, sy) = h.scroll;
                        if sx != 0.0 || sy != 0.0 {
                            hid.wheel(sx * SCROLL_SPEED, sy * SCROLL_SPEED);
                        }
                    }
                }
            }
        }
        // Second hand on the keyboard: it types too (trigger edge → key under
        // its ray), without touching the egui pointer the primary hand drives.
        if self.keyboard.visible && self.keyboard.placed {
            let primary = best.map(|(_, hi, _, _, _)| hi);
            let kb_size = keyboard::size_m_scaled(self.keyboard.scale);
            for (hi, h) in hands.iter().enumerate().filter(|(_, h)| h.active) {
                if Some(hi) == primary || self.keyboard.grab.is_some() {
                    continue;
                }
                if h.select && !self.select_prev[hi] {
                    if let Some((u, v, _)) = raycast(&h.aim, &self.keyboard.pose, kb_size) {
                        if let Some(k) = self.keyboard.key_at(u, v) {
                            self.keyboard.press(k);
                        }
                    }
                }
            }
        }
        // Release a held button when that hand lets go, wherever it points now.
        if let Some((hi, code)) = self.held {
            let still = hands.get(hi).is_some_and(|h| match (code, self.frozen) {
                (hid::BTN_LEFT, true) => h.precise,
                (hid::BTN_LEFT, false) => h.select,
                (hid::BTN_RIGHT, _) => h.secondary,
                _ => false,
            });
            if !still {
                if let Some(hid) = &mut self.hid {
                    hid.button(code, false);
                }
                self.held = None;
                self.held_press = None;
                self.dragging = false;
                self.frozen = false;
            }
        }
        for (i, h) in hands.iter().enumerate().take(2) {
            self.select_prev[i] = h.select;
            self.secondary_prev[i] = h.secondary;
            self.precise_prev[i] = h.precise;
        }
        if hands.is_empty() {
            self.select_prev = [false; 2];
            self.secondary_prev = [false; 2];
            self.precise_prev = [false; 2];
        }
        out
    }

    /// Index of the screen the laser is on this frame (for the laser fade).
    pub fn pointing_screen(&self) -> Option<usize> {
        match self.pointing {
            Some((Target::Screen(s), _)) => Some(s),
            _ => None,
        }
    }

    pub fn pointing(&self) -> bool {
        self.pointing.is_some() || self.screens.iter().any(|s| s.grab.is_some()) || self.keyboard.grab.is_some()
    }

    /// Layers for every shown screen: flat quads + curved cylinders.
    #[allow(clippy::type_complexity)]
    pub fn screen_layers<'a>(
        &'a mut self,
        space: &'a xr::Space,
    ) -> (Vec<xr::CompositionLayerQuad<'a, xr::Vulkan>>, Vec<xr::CompositionLayerCylinderKHR<'a, xr::Vulkan>>) {
        let (curved, cs) = (self.caps.curved, self.caps.color_scale);
        let mut quads = Vec::new();
        let mut cyls = Vec::new();
        for s in self.screens.iter_mut() {
            if s.cyl(curved).is_some() {
                if let Some(c) = s.cylinder(space, curved, cs) {
                    cyls.push(c);
                }
            } else if let Some(q) = s.quad(space, curved, cs) {
                quads.push(q);
            }
        }
        (quads, cyls)
    }

    /// Frames uploaded per screen (debug/status).
    #[allow(dead_code)]
    pub fn frame_counts(&self) -> Vec<(String, u64, Option<String>)> {
        self.screens.iter().map(|s| (s.name.clone(), s.frames, s.last_error.clone())).collect()
    }
}

fn pose_to_arr(p: &xr::Posef) -> [f32; 7] {
    [p.position.x, p.position.y, p.position.z, p.orientation.x, p.orientation.y, p.orientation.z, p.orientation.w]
}

fn arr_to_pose(a: &[f32; 7]) -> xr::Posef {
    xr::Posef {
        position: xr::Vector3f { x: a[0], y: a[1], z: a[2] },
        orientation: xr::Quaternionf { x: a[3], y: a[4], z: a[5], w: a[6] },
    }
}

fn dist2(a: &xr::Vector3f, b: &xr::Vector3f) -> f32 {
    let d = [a.x - b.x, a.y - b.y, a.z - b.z];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

fn output_detail(o: &outputs::OutputInfo) -> String {
    let mut d = format!("{}×{}", o.logical_size.0, o.logical_size.1);
    if !o.description.is_empty() {
        d = format!("{} · {d}", o.description);
    }
    d
}

fn desktop_bounds(outputs: &[outputs::OutputInfo]) -> ((f64, f64), (f64, f64)) {
    let rects: Vec<(f64, f64, f64, f64)> = outputs
        .iter()
        .map(|o| (o.logical_pos.0 as f64, o.logical_pos.1 as f64, o.logical_size.0 as f64, o.logical_size.1 as f64))
        .collect();
    bounds_of(&rects)
}

fn bounds_of(rects: &[(f64, f64, f64, f64)]) -> ((f64, f64), (f64, f64)) {
    if rects.is_empty() {
        return ((0.0, 0.0), (1.0, 1.0));
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for r in rects {
        x0 = x0.min(r.0);
        y0 = y0.min(r.1);
        x1 = x1.max(r.0 + r.2);
        y1 = y1.max(r.1 + r.3);
    }
    ((x0, y0), (x1 - x0, y1 - y0))
}
