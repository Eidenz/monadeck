//! Background link to the running Monado service via libmonado. Runs on its own
//! thread (the `Monado` handle isn't `Send`), polling for the running game and
//! handling one-shot commands: recenter the playspace, and arbitrate input
//! (block the game's controller input while the dashboard is in use — the same
//! pattern monado-frame uses, so summoning over a game doesn't double-input).
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use libmonado::{BlockFlags, ClientLogic, ClientState, DeviceLogic, DeviceRole, Monado};
use monadeck_core::devices::service_connected;

enum Cmd {
    SetBlock(bool),
    Recenter,
    /// Set the playspace tracking-origin offset: translation (m) + yaw (rad).
    SetOrigin { x: f32, y: f32, z: f32, yaw: f32 },
    /// Freeze (or unfreeze) a specific client's hand-controller poses.
    SetFreeze { client_id: u32, freeze: bool },
    /// Make a specific client the active/displayed app (Monado "primary").
    SetPrimary { client_id: u32 },
    /// Powered-off controllers hold their last pose (true) or go untracked.
    SetHoldPose(bool),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BatteryKind {
    Controller,
    Glove,
    Tracker,
    Other,
}

/// Whether a device is live, on but not tracking, or switched off. Needs the
/// fork's libmonado (API 1.9); elsewhere everything reads as `Live`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DevState {
    #[default]
    Live,
    /// On, but its pose has been untracked for over a second.
    Lost,
    /// Switched off: the charge is its last reading, not a live one.
    Off,
}

#[derive(Clone)]
pub struct BatteryInfo {
    pub kind: BatteryKind,
    pub charge: f32, // 0..1
    pub charging: bool,
    pub state: DevState,
}

/// A running app client (non-overlay session) shown on the Monado page.
#[derive(Clone)]
pub struct ClientInfo {
    pub id: u32,
    pub name: String,
    /// Visible / primary app (the running game), vs a backgrounded session.
    pub is_app: bool,
    /// The currently active/displayed app (Monado "primary").
    pub is_primary: bool,
    /// Whether we currently hold this client's controllers frozen.
    pub frozen: bool,
}

#[derive(Default)]
struct Status {
    /// Name of the primary (non-overlay) app, i.e. the running game.
    running_app: Option<String>,
    /// Per-device battery levels (controllers/gloves/trackers).
    batteries: Vec<BatteryInfo>,
    /// Freezable app clients (non-overlay sessions) and their freeze state.
    clients: Vec<ClientInfo>,
    /// Whether the loaded libmonado.so supports controller freezing (our fork).
    freeze_supported: bool,
    /// The left / right hand role is a UdCap glove (no trackpad: A+B drags).
    gloves: (bool, bool),
    /// The service's hold-pose-when-off mode; `None` when unsupported (not our
    /// fork, or not connected).
    hold_pose: Option<bool>,
}

pub struct MonadoLink {
    cmd_tx: Sender<Cmd>,
    status: Arc<Mutex<Status>>,
}

impl MonadoLink {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let st = Arc::clone(&status);
        thread::spawn(move || worker(cmd_rx, st));
        Self { cmd_tx, status }
    }

    /// The running game's name, if any (polled ~2×/s).
    pub fn running_app(&self) -> Option<String> {
        self.status.lock().unwrap().running_app.clone()
    }

    /// Current device battery levels.
    pub fn batteries(&self) -> Vec<BatteryInfo> {
        self.status.lock().unwrap().batteries.clone()
    }

    /// Freezable app clients (non-overlay sessions) with their freeze state,
    /// refreshed ~2×/s. Cheap (reads a cached snapshot).
    pub fn clients(&self) -> Vec<ClientInfo> {
        self.status.lock().unwrap().clients.clone()
    }

    /// Whether the active runtime's libmonado supports controller freezing — i.e.
    /// our Monado fork. False on stock Monado, so the freeze UI can hide itself.
    pub fn freeze_supported(&self) -> bool {
        self.status.lock().unwrap().freeze_supported
    }

    /// (left, right): that hand role is currently a UdCap glove.
    pub fn gloves(&self) -> (bool, bool) {
        self.status.lock().unwrap().gloves
    }

    /// Whether switched-off controllers hold their last pose (`Some(true)`) or
    /// report untracked; `None` when the runtime doesn't have the switch.
    pub fn hold_pose(&self) -> Option<bool> {
        self.status.lock().unwrap().hold_pose
    }

    /// Switch what apps see when a controller turns off. The service forgets it
    /// on restart (it starts out holding), so the link keeps the choice and
    /// re-applies it on every (re)connect.
    pub fn set_hold_pose(&self, hold: bool) {
        // Reflect at once; the next poll confirms.
        if let Some(h) = self.status.lock().unwrap().hold_pose.as_mut() {
            *h = hold;
        }
        let _ = self.cmd_tx.send(Cmd::SetHoldPose(hold));
    }

    /// Freeze (hold in place) or unfreeze a client's hand-controller poses.
    pub fn set_freeze(&self, client_id: u32, freeze: bool) {
        let _ = self.cmd_tx.send(Cmd::SetFreeze { client_id, freeze });
    }

    /// Make a client the active/displayed app (Monado "primary").
    pub fn set_primary(&self, client_id: u32) {
        let _ = self.cmd_tx.send(Cmd::SetPrimary { client_id });
    }

    /// Block (or unblock) the running game's controller input. Edge-triggered by
    /// the caller — only send on change.
    pub fn set_block(&self, block: bool) {
        let _ = self.cmd_tx.send(Cmd::SetBlock(block));
    }

    /// Recenter the VR playspace (`recenter_local_spaces`).
    pub fn recenter(&self) {
        let _ = self.cmd_tx.send(Cmd::Recenter);
    }

    /// Set the playspace offset (OVRAS-style floor/position adjust): translation
    /// in metres + yaw in radians. Persisted by the caller; re-applied on
    /// reconnect. `yaw` rotates the play area about the vertical axis.
    pub fn set_origin(&self, x: f32, y: f32, z: f32, yaw: f32) {
        let _ = self.cmd_tx.send(Cmd::SetOrigin { x, y, z, yaw });
    }
}

fn worker(cmd_rx: Receiver<Cmd>, status: Arc<Mutex<Status>>) {
    let mut mon: Option<Monado> = None;
    // The desired playspace offset, re-applied whenever we (re)connect so it
    // survives a service restart.
    let mut desired_origin: Option<(f32, f32, f32, f32)> = None;
    // Clients we currently hold frozen. Dropped on disconnect, since a service
    // restart re-allocates client ids (a stale id would freeze the wrong app).
    let mut frozen_ids: HashSet<u32> = HashSet::new();
    // When each device (by serial) was first seen untracked.
    let mut lost_since: HashMap<String, Instant> = HashMap::new();
    // The hold-pose-when-off choice, re-applied on every (re)connect.
    let mut desired_hold: Option<bool> = None;
    loop {
        let was_connected = mon.is_some();
        if mon.is_none() && service_connected() {
            mon = Monado::auto_connect().ok();
        }
        if !was_connected && mon.is_some() {
            if let Some((x, y, z, yaw)) = desired_origin {
                set_origin_offset(&mon, x, y, z, yaw);
            }
            if let (Some(m), Some(hold)) = (&mon, desired_hold) {
                if m.supports_hold_pose_when_off() {
                    let _ = m.set_hold_pose_when_off(hold);
                }
            }
        }
        match cmd_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Cmd::Recenter) => {
                if let Some(m) = &mon {
                    let _ = m.recenter_local_spaces();
                }
                // A recenter can reset spaces — re-assert our offset.
                if let Some((x, y, z, yaw)) = desired_origin {
                    set_origin_offset(&mon, x, y, z, yaw);
                }
            }
            Ok(Cmd::SetOrigin { x, y, z, yaw }) => {
                desired_origin = Some((x, y, z, yaw));
                set_origin_offset(&mon, x, y, z, yaw);
            }
            Ok(Cmd::SetBlock(block)) => apply_block(&mon, block),
            Ok(Cmd::SetFreeze { client_id, freeze }) => {
                if freeze {
                    frozen_ids.insert(client_id);
                } else {
                    frozen_ids.remove(&client_id);
                }
                apply_freeze(&mon, client_id, freeze);
                // Refresh the published list so the toggle reflects promptly.
                let clients = poll_clients(&mon, &frozen_ids);
                status.lock().unwrap().clients = clients;
            }
            Ok(Cmd::SetPrimary { client_id }) => {
                apply_set_primary(&mon, client_id);
                // Refresh so the active marker updates promptly.
                let clients = poll_clients(&mon, &frozen_ids);
                status.lock().unwrap().clients = clients;
            }
            Ok(Cmd::SetHoldPose(hold)) => {
                desired_hold = Some(hold);
                if let Some(m) = mon.as_ref().filter(|m| m.supports_hold_pose_when_off()) {
                    let _ = m.set_hold_pose_when_off(hold);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                let running = poll_running(&mut mon);
                if mon.is_none() {
                    frozen_ids.clear();
                }
                let batteries = poll_batteries(&mon, &mut lost_since);
                let hold_pose = mon
                    .as_ref()
                    .filter(|m| m.supports_hold_pose_when_off())
                    .and_then(|m| m.hold_pose_when_off().ok());
                let clients = poll_clients(&mon, &frozen_ids);
                let freeze_supported = mon.as_ref().map(|m| m.supports_controller_freeze()).unwrap_or(false);
                let gloves = poll_gloves(&mon);
                let mut s = status.lock().unwrap();
                s.running_app = running;
                s.batteries = batteries;
                s.clients = clients;
                s.freeze_supported = freeze_supported;
                s.gloves = gloves;
                s.hold_pose = hold_pose;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn poll_running(mon: &mut Option<Monado>) -> Option<String> {
    if !service_connected() {
        *mon = None;
        return None;
    }
    let m = mon.as_ref()?;
    match primary_app(m) {
        Ok(name) => name,
        Err(()) => {
            *mon = None;
            None
        }
    }
}

/// The primary, non-overlay client's name — the running game.
fn primary_app(m: &Monado) -> Result<Option<String>, ()> {
    let clients = m.clients().map_err(|_| ())?;
    for mut c in clients {
        let Ok(state) = c.state() else { continue };
        if state.contains(ClientState::ClientPrimaryApp) && !state.contains(ClientState::ClientSessionOverlay) {
            if let Ok(name) = c.name() {
                let n = name.trim();
                if !n.is_empty() && !n.eq_ignore_ascii_case("libmonado") {
                    return Ok(Some(name));
                }
            }
        }
    }
    Ok(None)
}

/// Which hand roles are UdCap gloves (they emulate Index controllers but have
/// no trackpad, so the playspace drag falls back to A+B).
fn poll_gloves(mon: &Option<Monado>) -> (bool, bool) {
    let Some(m) = mon else { return (false, false) };
    let Ok(devices) = m.devices() else { return (false, false) };
    let left = m.device_index_from_role(DeviceRole::Left).ok();
    let right = m.device_index_from_role(DeviceRole::Right).ok();
    let names: Vec<(u32, String)> = devices.into_iter().map(|d| (d.index(), d.name.to_lowercase())).collect();
    let is_glove = |idx: Option<u32>| {
        idx.and_then(|i| names.iter().find(|(j, _)| *j == i))
            .map(|(_, n)| n.contains("udcap") || n.contains("glove"))
            .unwrap_or(false)
    };
    (is_glove(left), is_glove(right))
}

/// Tracking has to be gone this long before a device reads as `Lost`, so a
/// hand briefly blocked from the base stations doesn't flicker.
const LOST_AFTER: Duration = Duration::from_secs(1);

/// Battery levels for devices that report one (controllers/gloves/trackers),
/// with whether each is live, untracked or switched off.
fn poll_batteries(mon: &Option<Monado>, lost_since: &mut HashMap<String, Instant>) -> Vec<BatteryInfo> {
    let Some(m) = mon else {
        lost_since.clear();
        return Vec::new();
    };
    let left = m.device_index_from_role(DeviceRole::Left).ok();
    let right = m.device_index_from_role(DeviceRole::Right).ok();
    let Ok(devices) = m.devices() else { return Vec::new() };
    let now = Instant::now();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dev in devices {
        let idx = dev.index();
        let Ok(b) = dev.battery_status() else { continue };
        if !b.present {
            continue;
        }
        let key = dev.serial().unwrap_or_else(|_| format!("#{idx}"));
        let state = match dev.tracking_state() {
            Ok(t) if !t.connected => DevState::Off,
            Ok(t) if !t.tracking => {
                let since = *lost_since.entry(key.clone()).or_insert(now);
                seen.insert(key);
                if now.duration_since(since) >= LOST_AFTER {
                    DevState::Lost
                } else {
                    DevState::Live
                }
            }
            _ => DevState::Live,
        };
        let name = dev.name.to_lowercase();
        let is_glove = name.contains("glove") || name.contains("udcap");
        let is_ctrl = Some(idx) == left
            || Some(idx) == right
            || name.contains("controller")
            || name.contains("knuckles")
            || name.contains("index");
        let kind = if is_glove {
            BatteryKind::Glove
        } else if is_ctrl {
            BatteryKind::Controller
        } else if name.contains("tracker") {
            BatteryKind::Tracker
        } else {
            BatteryKind::Other
        };
        out.push(BatteryInfo { kind, charge: b.charge, charging: b.charging && state != DevState::Off, state });
    }
    lost_since.retain(|k, _| seen.contains(k));
    out
}

/// Apply a playspace offset to the primary tracking origin (OVRAS-style). `yaw`
/// is a rotation about the vertical (Y) axis. No-op if not connected / no origin.
fn set_origin_offset(mon: &Option<Monado>, x: f32, y: f32, z: f32, yaw: f32) {
    let Some(m) = mon else { return };
    let Ok(origins) = m.tracking_origins() else { return };
    let pose = libmonado::Pose {
        position: mint::Vector3 { x, y, z },
        orientation: mint::Quaternion {
            v: mint::Vector3 { x: 0.0, y: (yaw * 0.5).sin(), z: 0.0 },
            s: (yaw * 0.5).cos(),
        },
    };
    if let Some(origin) = origins.into_iter().next() {
        let _ = origin.set_offset(pose);
    }
}

/// Block/unblock controller input on the game client (active + visible + not an
/// overlay), so the dashboard doesn't fight the game for input.
fn apply_block(mon: &Option<Monado>, block: bool) {
    let Some(m) = mon else { return };
    let Ok(clients) = m.clients() else { return };
    for mut c in clients {
        let Ok(state) = c.state() else { continue };
        let is_game = state.contains(ClientState::ClientSessionActive)
            && state.contains(ClientState::ClientSessionVisible)
            && !state.contains(ClientState::ClientSessionOverlay);
        if !is_game {
            continue;
        }
        let flags = if block { BlockFlags::BlockInputs } else { BlockFlags::None };
        let _ = c.set_io_blocks(flags.into());
    }
}

/// Snapshot the freezable app clients: real (named) sessions that aren't overlays.
/// Overlays (monadeck itself, WayVR) are excluded so freezing never breaks the
/// hand you use to drive the dashboard. `frozen` reflects our own freeze set.
fn poll_clients(mon: &Option<Monado>, frozen: &HashSet<u32>) -> Vec<ClientInfo> {
    let Some(m) = mon else { return Vec::new() };
    let Ok(clients) = m.clients() else { return Vec::new() };
    let mut out = Vec::new();
    for mut c in clients {
        let Ok(state) = c.state() else { continue };
        if state.contains(ClientState::ClientSessionOverlay) {
            continue;
        }
        let Ok(name) = c.name() else { continue };
        let n = name.trim();
        if n.is_empty() || n.eq_ignore_ascii_case("libmonado") {
            continue;
        }
        let id = c.id();
        let is_app =
            state.contains(ClientState::ClientSessionVisible) || state.contains(ClientState::ClientPrimaryApp);
        let is_primary = state.contains(ClientState::ClientPrimaryApp);
        out.push(ClientInfo { id, name: n.to_string(), is_app, is_primary, frozen: frozen.contains(&id) });
    }
    out
}

/// Freeze/unfreeze a single client's hand-controller poses by id.
fn apply_freeze(mon: &Option<Monado>, client_id: u32, freeze: bool) {
    let Some(m) = mon else { return };
    let Ok(clients) = m.clients() else { return };
    for mut c in clients {
        if c.id() == client_id {
            let _ = c.set_controller_freeze(freeze);
            return;
        }
    }
}

/// Make a client the active/displayed app (Monado "primary") by id.
fn apply_set_primary(mon: &Option<Monado>, client_id: u32) {
    let Some(m) = mon else { return };
    let Ok(clients) = m.clients() else { return };
    for mut c in clients {
        if c.id() == client_id {
            let _ = c.set_primary();
            return;
        }
    }
}
