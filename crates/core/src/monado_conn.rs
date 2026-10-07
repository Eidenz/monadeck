//! A single, long-lived libmonado connection served from a dedicated thread.
//!
//! Two reasons it lives on its own thread behind a channel:
//! - libmonado's `Monado` handle holds a dlopen container + raw pointer, so it
//!   isn't `Send` — it can't be parked in a shared mutex across Tauri's blocking
//!   pool.
//! - Reconnecting on every poll makes monado-service log a client
//!   connect/disconnect each time. Holding one connection means the service sees
//!   a single persistent client, exactly like Envision.

use crate::devices::{build_snapshot_from, service_connected, Snapshot};
use crate::room_setup::HeadSample;
use libmonado::{DeviceLogic, DeviceRole, Monado};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

enum Req {
    Snapshot(SyncSender<Result<Snapshot, String>>),
    HeadPose(SyncSender<Result<HeadSample, String>>),
    Disconnect(SyncSender<()>),
}

/// Handle to the connection worker. Cheap to clone-share via `Arc`.
pub struct MonadoConn {
    tx: Mutex<Sender<Req>>,
    /// Let go of the service: no connection until [`MonadoConn::resume`].
    released: Arc<AtomicBool>,
}

impl Default for MonadoConn {
    fn default() -> Self {
        Self::new()
    }
}

impl MonadoConn {
    pub fn new() -> Self {
        let (tx, rx) = channel::<Req>();
        let released = Arc::new(AtomicBool::new(false));
        let worker_released = released.clone();
        thread::Builder::new()
            .name("monado-conn".into())
            .spawn(move || worker_loop(rx, worker_released))
            .expect("failed to spawn monado-conn thread");
        Self { tx: Mutex::new(tx), released }
    }

    /// Close the connection and stay away until [`resume`](Self::resume):
    /// before a stop. monado-service waits up to 3 s for every client to
    /// leave before it shuts down, and the time it then has left goes to
    /// switching the devices off.
    pub fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
        let (reply_tx, reply_rx) = sync_channel::<()>(1);
        let sent = self.tx.lock().map(|tx| tx.send(Req::Disconnect(reply_tx)).is_ok()).unwrap_or(false);
        if sent {
            // Behind a slow query at worst; don't hold the stop up for long.
            let _ = reply_rx.recv_timeout(Duration::from_secs(2));
        }
    }

    /// Connect again on the next request (the service is starting).
    pub fn resume(&self) {
        self.released.store(false, Ordering::SeqCst);
    }

    /// Fetch a devices+clients snapshot over the persistent connection. Blocking
    /// (call from a blocking task), returns `Err` when the service is down.
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        let (reply_tx, reply_rx) = sync_channel::<Result<Snapshot, String>>(1);
        self.tx
            .lock()
            .map_err(|_| "monado conn mutex poisoned".to_string())?
            .send(Req::Snapshot(reply_tx))
            .map_err(|_| "monado conn worker is gone".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "monado conn worker dropped the reply".to_string())?
    }

    /// Where the headset is right now (room setup applied, no offsets), over
    /// the persistent connection. Blocking; `Err` when the service is down or
    /// the runtime can't report device poses (stock Monado).
    pub fn head_pose(&self) -> Result<HeadSample, String> {
        let (reply_tx, reply_rx) = sync_channel::<Result<HeadSample, String>>(1);
        self.tx
            .lock()
            .map_err(|_| "monado conn mutex poisoned".to_string())?
            .send(Req::HeadPose(reply_tx))
            .map_err(|_| "monado conn worker is gone".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "monado conn worker dropped the reply".to_string())?
    }
}

fn worker_loop(rx: Receiver<Req>, released: Arc<AtomicBool>) {
    // The one connection, owned entirely by this thread.
    let mut conn: Option<Monado> = None;
    while let Ok(req) = rx.recv() {
        if released.load(Ordering::SeqCst) {
            conn = None;
        }
        match req {
            Req::Snapshot(reply) => {
                let _ = reply.send(if conn_allowed(&released) { query(&mut conn) } else { Err(STOPPING.into()) });
            }
            Req::HeadPose(reply) => {
                let _ = reply.send(if conn_allowed(&released) { head_pose(&mut conn) } else { Err(STOPPING.into()) });
            }
            Req::Disconnect(reply) => {
                conn = None;
                let _ = reply.send(());
            }
        }
    }
}

const STOPPING: &str = "monado service is stopping";

fn conn_allowed(released: &AtomicBool) -> bool {
    !released.load(Ordering::SeqCst)
}

/// The live connection, (re)connecting if needed. Drops a stale one when the
/// service is down.
fn connection(conn: &mut Option<Monado>) -> Result<&Monado, String> {
    // Socket gone → service is down; drop any stale connection and bail quietly.
    if !service_connected() {
        *conn = None;
        return Err("monado service is not running".into());
    }
    // Connect once, then reuse.
    if conn.is_none() {
        *conn = Some(Monado::auto_connect().map_err(|e| format!("connect failed: {e}"))?);
    }
    Ok(conn.as_ref().unwrap())
}

fn head_pose(conn: &mut Option<Monado>) -> Result<HeadSample, String> {
    let pose = {
        let m = connection(conn)?;
        if !m.supports_device_pose() {
            return Err("This Monado can't report where the headset is. Update to the Monado fork \
                        25.1.0-eidenz10 or newer."
                .into());
        }
        let head = m
            .device_from_role(DeviceRole::Head)
            .map_err(|e| format!("No headset found ({e:?})"))?;
        head.pose().map(|(pose, tracked)| HeadSample {
            position: [pose.position.x as f64, pose.position.y as f64, pose.position.z as f64],
            orientation: [
                pose.orientation.v.x as f64,
                pose.orientation.v.y as f64,
                pose.orientation.v.z as f64,
                pose.orientation.s as f64,
            ],
            tracked,
            name: head.name.clone(),
        })
    };
    // Likely a dead connection (service restarted): reconnect next time.
    pose.map_err(|e| {
        *conn = None;
        format!("Couldn't read the headset's pose ({e:?})")
    })
}

fn query(conn: &mut Option<Monado>) -> Result<Snapshot, String> {
    connection(conn)?;
    match build_snapshot_from(conn.as_ref().unwrap()) {
        Ok(snapshot) => Ok(snapshot),
        Err(e) => {
            // Likely a dead connection (service restarted) — drop it so the next
            // call reconnects cleanly.
            *conn = None;
            Err(e)
        }
    }
}
