//! Now-playing + transport for the watch, over MPRIS (org.mpris.MediaPlayer2.*
//! on the session bus). A thread polls the active player once a second and
//! runs Play/Pause/Next/Previous on request.
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zbus::zvariant::OwnedValue;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaState {
    pub player: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum MediaCmd {
    PlayPause,
    Next,
    Previous,
}

pub struct Media {
    state: Arc<Mutex<Option<MediaState>>>,
    tx: mpsc::Sender<MediaCmd>,
}

impl Media {
    pub fn start() -> Self {
        let state: Arc<Mutex<Option<MediaState>>> = Arc::new(Mutex::new(None));
        let (tx, rx) = mpsc::channel::<MediaCmd>();
        let shared = state.clone();
        std::thread::Builder::new()
            .name("mpris".into())
            .spawn(move || {
                let Ok(conn) = zbus::blocking::Connection::session() else {
                    log::warn!("media: no session bus; MPRIS off");
                    return;
                };
                let mut current: Option<String> = None;
                loop {
                    // Commands first (they target the last polled player).
                    while let Ok(cmd) = rx.try_recv() {
                        if let Some(p) = &current {
                            let m = match cmd {
                                MediaCmd::PlayPause => "PlayPause",
                                MediaCmd::Next => "Next",
                                MediaCmd::Previous => "Previous",
                            };
                            if let Err(e) = conn.call_method(Some(p.as_str()), "/org/mpris/MediaPlayer2", Some("org.mpris.MediaPlayer2.Player"), m, &()) {
                                log::warn!("media: {m} on {p}: {e}");
                            }
                        }
                    }
                    let st = poll(&conn, &mut current);
                    if let Ok(mut g) = shared.lock() {
                        *g = st;
                    }
                    std::thread::sleep(Duration::from_millis(1000));
                }
            })
            .expect("spawn mpris thread");
        Self { state, tx }
    }

    pub fn state(&self) -> Option<MediaState> {
        self.state.lock().ok().and_then(|g| g.clone())
    }

    pub fn send(&self, cmd: MediaCmd) {
        let _ = self.tx.send(cmd);
    }
}

fn prop<T: TryFrom<OwnedValue>>(conn: &zbus::blocking::Connection, dest: &str, iface: &str, name: &str) -> Option<T> {
    let reply = conn
        .call_method(Some(dest), "/org/mpris/MediaPlayer2", Some("org.freedesktop.DBus.Properties"), "Get", &(iface, name))
        .ok()?;
    let v: OwnedValue = reply.body().deserialize::<zbus::zvariant::Value>().ok()?.try_to_owned().ok()?;
    T::try_from(v).ok()
}

/// Pick the playing player (else the first), read its now-playing info.
fn poll(conn: &zbus::blocking::Connection, current: &mut Option<String>) -> Option<MediaState> {
    let names: Vec<String> = conn
        .call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "ListNames", &())
        .ok()?
        .body()
        .deserialize()
        .ok()?;
    let players: Vec<String> = names
        .into_iter()
        .filter(|n| n.starts_with("org.mpris.MediaPlayer2.") && !n.contains("playerctld"))
        .collect();
    if players.is_empty() {
        *current = None;
        return None;
    }
    let mut best: Option<(String, String)> = None; // (name, status)
    for p in &players {
        let status: String = prop(conn, p, "org.mpris.MediaPlayer2.Player", "PlaybackStatus").unwrap_or_default();
        let playing = status == "Playing";
        // Prefer a playing one; else keep the previously chosen; else the first.
        let keep = match &best {
            None => true,
            Some((_, s)) => playing && s != "Playing",
        };
        if keep || (current.as_deref() == Some(p.as_str()) && best.as_ref().is_none_or(|(_, s)| s != "Playing")) {
            best = Some((p.clone(), status));
        }
    }
    let (name, status) = best?;
    *current = Some(name.clone());
    let identity: String = prop(conn, &name, "org.mpris.MediaPlayer2", "Identity").unwrap_or_else(|| name.clone());
    let meta: std::collections::HashMap<String, OwnedValue> = prop(conn, &name, "org.mpris.MediaPlayer2.Player", "Metadata").unwrap_or_default();
    let title = meta.get("xesam:title").and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default();
    let artist = meta
        .get("xesam:artist")
        .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
        .map(|a| a.join(", "))
        .unwrap_or_default();
    Some(MediaState { player: identity, title, artist, playing: status == "Playing" })
}
