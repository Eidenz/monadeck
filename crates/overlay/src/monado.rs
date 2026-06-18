//! Background link to the running Monado service via libmonado. Runs on its own
//! thread (the `Monado` handle isn't `Send`), polling for the running game and
//! handling one-shot commands: recenter the playspace, and arbitrate input
//! (block the game's controller input while the dashboard is in use — the same
//! pattern monado-frame uses, so summoning over a game doesn't double-input).
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use libmonado::{BlockFlags, ClientLogic, ClientState, DeviceLogic, DeviceRole, Monado};
use monadeck_core::devices::service_connected;

enum Cmd {
    SetBlock(bool),
    Recenter,
    /// Set the playspace tracking-origin offset: translation (m) + yaw (rad).
    SetOrigin { x: f32, y: f32, z: f32, yaw: f32 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BatteryKind {
    Controller,
    Glove,
    Tracker,
    Other,
}

#[derive(Clone)]
pub struct BatteryInfo {
    pub kind: BatteryKind,
    pub charge: f32, // 0..1
    pub charging: bool,
}

#[derive(Default)]
struct Status {
    /// Name of the primary (non-overlay) app, i.e. the running game.
    running_app: Option<String>,
    /// Per-device battery levels (controllers/gloves/trackers).
    batteries: Vec<BatteryInfo>,
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
    loop {
        let was_connected = mon.is_some();
        if mon.is_none() && service_connected() {
            mon = Monado::auto_connect().ok();
        }
        if !was_connected && mon.is_some() {
            if let Some((x, y, z, yaw)) = desired_origin {
                set_origin_offset(&mon, x, y, z, yaw);
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
            Err(RecvTimeoutError::Timeout) => {
                let running = poll_running(&mut mon);
                let batteries = poll_batteries(&mon);
                let mut s = status.lock().unwrap();
                s.running_app = running;
                s.batteries = batteries;
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

/// Battery levels for devices that report one (controllers/gloves/trackers).
fn poll_batteries(mon: &Option<Monado>) -> Vec<BatteryInfo> {
    let Some(m) = mon else { return Vec::new() };
    let left = m.device_index_from_role(DeviceRole::Left).ok();
    let right = m.device_index_from_role(DeviceRole::Right).ok();
    let Ok(devices) = m.devices() else { return Vec::new() };
    let mut out = Vec::new();
    for dev in devices {
        let idx = dev.index();
        let Ok(b) = dev.battery_status() else { continue };
        if !b.present {
            continue;
        }
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
        out.push(BatteryInfo { kind, charge: b.charge, charging: b.charging });
    }
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
