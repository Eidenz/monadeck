//! Background link to the running Monado service via libmonado. Runs on its own
//! thread (the `Monado` handle isn't `Send`), polling for the running game and
//! handling one-shot commands: recenter the playspace, and arbitrate input
//! (block the game's controller input while the dashboard is in use — the same
//! pattern monado-frame uses, so summoning over a game doesn't double-input).
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use libmonado::{BlockFlags, ClientLogic, ClientState, Monado};
use monadeck_core::devices::service_connected;

enum Cmd {
    SetBlock(bool),
    Recenter,
}

#[derive(Default)]
struct Status {
    /// Name of the primary (non-overlay) app, i.e. the running game.
    running_app: Option<String>,
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

    /// Block (or unblock) the running game's controller input. Edge-triggered by
    /// the caller — only send on change.
    pub fn set_block(&self, block: bool) {
        let _ = self.cmd_tx.send(Cmd::SetBlock(block));
    }

    /// Recenter the VR playspace (`recenter_local_spaces`).
    pub fn recenter(&self) {
        let _ = self.cmd_tx.send(Cmd::Recenter);
    }
}

fn worker(cmd_rx: Receiver<Cmd>, status: Arc<Mutex<Status>>) {
    let mut mon: Option<Monado> = None;
    loop {
        if mon.is_none() && service_connected() {
            mon = Monado::auto_connect().ok();
        }
        match cmd_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Cmd::Recenter) => {
                if let Some(m) = &mon {
                    let _ = m.recenter_local_spaces();
                }
            }
            Ok(Cmd::SetBlock(block)) => apply_block(&mon, block),
            Err(RecvTimeoutError::Timeout) => {
                let running = poll_running(&mut mon);
                status.lock().unwrap().running_app = running;
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
