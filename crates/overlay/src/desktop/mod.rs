//! WayVR-style desktop viewer for the overlay: mirror the user's monitors into
//! VR as grabbable quad layers and drive a virtual mouse from the laser.
//!
//! Pipeline: xdg-desktop-portal ScreenCast (`portal`) → PipeWire video node
//! (`pw`) → DMA-BUF import + blit into an OpenXR swapchain (`dmabuf`, `screen`)
//! → quad layer. Interaction: laser hit (u, v) → desktop-logical coordinates via
//! the monitor layout (`outputs`) → absolute uinput mouse (`hid`).
//!
//! Screens are independent of the dashboard: once shown they persist while the
//! dashboard is dismissed and while a game runs, exactly like WayVR.
pub mod dmabuf;
pub mod hid;
pub mod outputs;
pub mod portal;
pub mod pw;
pub mod screen;
pub mod selftest;

use std::sync::mpsc::Receiver;
use std::sync::Mutex;

use ash::vk;
use openxr as xr;

use crate::mathx::{front_pose, pose_compose};
use dmabuf::{Caps, Importer};
use hid::UInput;
use screen::ScreenPanel;

const GRAB_START: f32 = 0.40;
const GRAB_RELEASE: f32 = 0.15;
/// Wheel notches per frame at full thumbstick deflection.
const SCROLL_SPEED: f32 = 0.12;
/// Default distance a newly shown screen is placed at, metres.
const PLACE_DIST: f32 = 1.6;

/// Per-hand controller state for one frame (only while the overlay is focused).
pub struct HandInput {
    /// Pose located this frame (inactive hands keep their slot for edge tracking).
    pub active: bool,
    pub aim: xr::Posef,
    #[allow(dead_code)]
    pub path: xr::Path,
    pub select: bool,
    pub secondary: bool,
    pub grip: f32,
    pub scroll: (f32, f32),
}

/// One row of the in-headset "Desktop" page.
#[derive(Clone, Debug)]
pub struct ScreenRow {
    pub name: String,
    pub detail: String,
    pub shown: bool,
    /// A stream exists for it (portal approved this monitor).
    pub available: bool,
    /// Asked to show, waiting on the portal.
    pub pending: bool,
}

enum PortalState {
    Idle,
    Pending(Receiver<Result<portal::Cast, String>>),
    Ready(portal::Cast),
    Failed(String),
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
    /// Output names asked for before the portal answered.
    pending_show: Vec<String>,
    /// Which screen the laser points at this frame (index, hand).
    pointing: Option<(usize, usize)>,
    /// A mouse button held down by (hand, button code) — released on trigger up
    /// even if the laser has left the screen, so nothing gets stuck.
    held: Option<(usize, u16)>,
    hover_prev: Option<(f64, f64)>,
    select_prev: [bool; 2],
    secondary_prev: [bool; 2],
    pending_gpu_teardown: bool,
    width_m: f32,
}

impl DesktopViewer {
    pub fn new(caps: Caps, importer: Option<Importer>, token: Option<String>) -> Self {
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
            pending_show: Vec::new(),
            pointing: None,
            held: None,
            hover_prev: None,
            select_prev: [false; 2],
            secondary_prev: [false; 2],
            pending_gpu_teardown: false,
            width_m: screen::DEFAULT_WIDTH_M,
        }
    }

    /// Physical width for every mirrored screen (live + for new ones).
    pub fn set_width(&mut self, w: f32) {
        self.width_m = w.clamp(0.4, 4.0);
        for s in &mut self.screens {
            s.width_m = self.width_m;
        }
    }

    // --- UI-facing state -------------------------------------------------

    pub fn rows(&self) -> Vec<ScreenRow> {
        let mut rows: Vec<ScreenRow> = Vec::new();
        if self.screens.is_empty() {
            for o in &self.outputs {
                rows.push(ScreenRow {
                    name: o.name.clone(),
                    detail: output_detail(o),
                    shown: false,
                    available: false,
                    pending: self.pending_show.contains(&o.name),
                });
            }
        } else {
            for s in &self.screens {
                rows.push(ScreenRow {
                    name: s.name.clone(),
                    detail: s.detail.clone(),
                    shown: s.shown,
                    available: true,
                    pending: false,
                });
            }
            // Outputs the portal didn't include (user didn't tick them).
            for o in &self.outputs {
                if !self.screens.iter().any(|s| s.name == o.name) {
                    rows.push(ScreenRow {
                        name: o.name.clone(),
                        detail: output_detail(o),
                        shown: false,
                        available: false,
                        pending: false,
                    });
                }
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
                    "Pick a screen to mirror. The first time, approve the share dialog on your desktop.".into()
                }
            }
            PortalState::Pending(_) => "Waiting for the screen-share dialog on your desktop…".into(),
            PortalState::Ready(c) => {
                let mode = if self.caps.dmabuf { "GPU (DMA-BUF)" } else { "CPU (SHM)" };
                format!("{} screen(s) approved · capture: {mode}", c.streams.len())
            }
            PortalState::Failed(e) => format!("Screen share failed: {e}"),
        }
    }

    pub fn shown_count(&self) -> usize {
        self.screens.iter().filter(|s| s.shown).count()
    }

    /// Show/hide the screen behind row `i`. Before the portal has answered this
    /// starts the request and remembers what to show.
    pub fn toggle(&mut self, i: usize) {
        let rows = self.rows();
        let Some(row) = rows.get(i) else { return };
        let name = row.name.clone();
        if let Some(s) = self.screens.iter_mut().find(|s| s.name == name) {
            if s.shown {
                s.hide();
            } else {
                s.show(&self.caps);
            }
            return;
        }
        // Not available yet: (re)start the portal flow.
        match self.pending_show.iter().position(|n| n == &name) {
            Some(p) => {
                self.pending_show.remove(p);
            }
            None => self.pending_show.push(name),
        }
        if matches!(self.portal, PortalState::Idle | PortalState::Failed(_)) {
            self.start_portal();
        } else if matches!(self.portal, PortalState::Ready(_)) {
            // Approved set doesn't include this monitor — ask again.
            self.reselect();
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

    // --- Per-frame ---------------------------------------------------------

    /// Drive the portal, build screens when streams arrive, place newly shown
    /// screens, and upload the newest frames. Call every frame (hidden or not).
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
                    // Apply what was asked for while we waited.
                    let wanted = std::mem::take(&mut self.pending_show);
                    for name in wanted {
                        if let Some(s) = self.screens.iter_mut().find(|s| s.name == name) {
                            s.show(&self.caps);
                        } else {
                            log::warn!("desktop: '{name}' was not among the approved screens");
                        }
                    }
                }
                Ok(Err(e)) => {
                    log::error!("desktop: {e}");
                    self.pending_show.clear();
                    self.portal = PortalState::Failed(e);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.portal = PortalState::Failed("portal thread died".into());
                }
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
            if let Err(e) = s.upload(session, device, allocator, self.importer.as_ref(), &self.caps, cmd, queue, fence) {
                log::error!("desktop: {} upload: {e}", s.name);
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
                    self.outputs.iter().find(|o| {
                        st.position == Some(o.logical_pos) && st.size == Some(o.logical_size)
                    })
                })
                .or_else(|| {
                    // Single-output sessions: the only choice.
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
        // With no Wayland output list, derive the pointer bounds from the streams.
        if self.outputs.is_empty() {
            if let Some(h) = &mut self.hid {
                let rects: Vec<(f64, f64, f64, f64)> = self.screens.iter().map(|s| s.rect).collect();
                let (origin, extent) = bounds_of(&rects);
                h.set_desktop(origin, extent);
            }
        }
    }

    /// Laser interaction with the shown screens. `max_t`: distance of a closer
    /// dashboard hit (screens behind it are ignored). Returns the laser ray
    /// `(aim, t)` when pointing at a screen.
    pub fn update_input(&mut self, hands: &[HandInput], max_t: Option<f32>) -> Option<(xr::Posef, f32)> {
        self.pointing = None;
        // Continue grabs first (the grabbed screen follows the hand).
        for s in &mut self.screens {
            if let Some((hand, offset)) = s.grab {
                match hands.get(hand) {
                    Some(h) if h.active && h.grip >= GRAB_RELEASE => s.pose = pose_compose(&h.aim, &offset),
                    _ => s.grab = None,
                }
            }
        }
        let grabbing = self.screens.iter().any(|s| s.grab.is_some());

        // Closest screen hit across hands.
        let mut best: Option<(usize, usize, f32, f32, f32)> = None; // (screen, hand, u, v, t)
        if !grabbing {
            for (hi, h) in hands.iter().enumerate().filter(|(_, h)| h.active) {
                for (si, s) in self.screens.iter().enumerate() {
                    if let Some((u, v, t)) = s.hit(&h.aim) {
                        if max_t.is_some_and(|m| t >= m) {
                            continue;
                        }
                        if best.map_or(true, |b| t < b.4) {
                            best = Some((si, hi, u, v, t));
                        }
                    }
                }
            }
        }

        // Grip while pointing grabs that screen.
        if let Some((si, hi, _, _, _)) = best {
            if hands[hi].grip > GRAB_START {
                let aim = hands[hi].aim;
                self.screens[si].start_grab(hi, &aim);
                best = None;
            }
        }

        let mut ray = None;
        if let Some((si, hi, u, v, t)) = best {
            self.pointing = Some((si, hi));
            ray = Some((hands[hi].aim, t));
            let (x, y) = self.screens[si].desktop_pos(u, v);
            let h = &hands[hi];
            if let Some(hid) = &mut self.hid {
                // Hover → move (skip sub-pixel jitter).
                let moved = self.hover_prev.map_or(true, |(px, py)| (px - x).abs() >= 0.5 || (py - y).abs() >= 0.5);
                if moved {
                    hid.mouse_move(x, y);
                    self.hover_prev = Some((x, y));
                }
                // Clicks: rising edge of trigger / A on the pointing hand.
                if h.select && !self.select_prev[hi] && self.held.is_none() {
                    hid.mouse_move(x, y);
                    hid.button(hid::BTN_LEFT, true);
                    self.held = Some((hi, hid::BTN_LEFT));
                }
                if h.secondary && !self.secondary_prev[hi] && self.held.is_none() {
                    hid.mouse_move(x, y);
                    hid.button(hid::BTN_RIGHT, true);
                    self.held = Some((hi, hid::BTN_RIGHT));
                }
                // Thumbstick scroll.
                let (sx, sy) = h.scroll;
                if sx != 0.0 || sy != 0.0 {
                    hid.wheel(sx * SCROLL_SPEED, sy * SCROLL_SPEED);
                }
            }
        }
        // Release a held button when that hand lets go, wherever it points now.
        if let Some((hi, code)) = self.held {
            let still = hands.get(hi).is_some_and(|h| match code {
                hid::BTN_LEFT => h.select,
                hid::BTN_RIGHT => h.secondary,
                _ => false,
            });
            if !still {
                if let Some(hid) = &mut self.hid {
                    hid.button(code, false);
                }
                self.held = None;
            }
        }
        for (i, h) in hands.iter().enumerate().take(2) {
            self.select_prev[i] = h.select;
            self.secondary_prev[i] = h.secondary;
        }
        if hands.is_empty() {
            self.select_prev = [false; 2];
            self.secondary_prev = [false; 2];
        }
        ray
    }

    /// Index of the screen the laser is on this frame (for the laser fade).
    pub fn pointing_screen(&self) -> Option<usize> {
        self.pointing.map(|(s, _)| s)
    }

    pub fn pointing(&self) -> bool {
        self.pointing.is_some() || self.screens.iter().any(|s| s.grab.is_some())
    }

    pub fn quad_layers<'a>(&'a self, space: &'a xr::Space) -> Vec<xr::CompositionLayerQuad<'a, xr::Vulkan>> {
        self.screens.iter().filter_map(|s| s.quad(space)).collect()
    }

    /// Frames uploaded per screen (debug/status).
    #[allow(dead_code)]
    pub fn frame_counts(&self) -> Vec<(String, u64, Option<String>)> {
        self.screens.iter().map(|s| (s.name.clone(), s.frames, s.last_error.clone())).collect()
    }
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
