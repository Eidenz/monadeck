//! Desktop + XSOverlay notifications in the headset (the role WayVR played):
//! - D-Bus: a monitor on `org.freedesktop.Notifications.Notify` (BecomeMonitor,
//!   so the desktop's own notification daemon keeps working untouched);
//! - XSOverlay protocol: JSON over UDP on 127.0.0.1:42069 — what VRCX and other
//!   VR tools send to XSOverlay.
//! Both feed a queue the overlay turns into toasts.
use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

use base64::Engine;

pub struct Incoming {
    pub app: String,
    pub title: String,
    pub body: String,
    /// Seconds to show.
    pub timeout: f32,
    /// Decoded icon (XSOverlay base64), small.
    pub icon: Option<egui::ColorImage>,
    pub source: Source,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Desktop,
    XsOverlay,
}

pub struct Notifications {
    rx: mpsc::Receiver<Incoming>,
    pub dbus_ok: bool,
    pub udp_ok: bool,
}

impl Notifications {
    pub fn start(desktop: bool, xso: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        let dbus_ok = desktop && start_dbus(tx.clone());
        let udp_ok = xso && start_udp(tx);
        Self { rx, dbus_ok, udp_ok }
    }

    pub fn drain(&self) -> Vec<Incoming> {
        self.rx.try_iter().collect()
    }
}

// --- D-Bus ------------------------------------------------------------------------

fn start_dbus(tx: mpsc::Sender<Incoming>) -> bool {
    let conn = match zbus::blocking::Connection::session() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("notifications: no session bus ({e}); desktop notifications off");
            return false;
        }
    };
    let rule = "type='method_call',interface='org.freedesktop.Notifications',member='Notify'";
    let res = conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus.Monitoring"),
        "BecomeMonitor",
        &(vec![rule], 0u32),
    );
    if let Err(e) = res {
        log::warn!("notifications: BecomeMonitor refused ({e}); desktop notifications off");
        return false;
    }
    std::thread::Builder::new()
        .name("notif-dbus".into())
        .spawn(move || {
            let iter = zbus::blocking::MessageIterator::from(&conn);
            for msg in iter {
                let Ok(msg) = msg else { continue };
                let hdr = msg.header();
                if hdr.message_type() != zbus::message::Type::MethodCall
                    || hdr.member().map(|m| m.as_str()) != Some("Notify")
                {
                    continue;
                }
                type Body = (String, u32, String, String, String, Vec<String>, HashMap<String, zbus::zvariant::OwnedValue>, i32);
                let Ok((app, _replaces, _icon, summary, body, _actions, _hints, expire)) = msg.body().deserialize::<Body>() else {
                    continue;
                };
                let title = if summary.is_empty() { app.clone() } else { summary };
                let timeout = if expire > 0 { (expire as f32 / 1000.0).clamp(2.0, 15.0) } else { 5.0 };
                let _ = tx.send(Incoming { app, title, body: strip_markup(&body), timeout, icon: None, source: Source::Desktop });
            }
            log::warn!("notifications: D-Bus monitor ended");
        })
        .expect("spawn notification monitor");
    log::info!("notifications: listening to desktop notifications (D-Bus monitor)");
    true
}

/// Notification bodies may carry a little HTML (freedesktop spec): drop tags.
fn strip_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&").replace("&quot;", "\"")
}

// --- XSOverlay (UDP JSON) --------------------------------------------------------

#[allow(non_snake_case, dead_code)]
#[derive(serde::Deserialize)]
struct XsoMessage {
    messageType: i32,
    #[serde(default)]
    index: Option<i32>,
    #[serde(default)]
    volume: Option<f32>,
    #[serde(default)]
    audioPath: Option<String>,
    #[serde(default)]
    timeout: Option<f32>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    height: Option<f32>,
    #[serde(default)]
    opacity: Option<f32>,
    #[serde(default)]
    useBase64Icon: Option<bool>,
    #[serde(default)]
    sourceApp: Option<String>,
    #[serde(default)]
    alwaysShow: Option<bool>,
}

fn start_udp(tx: mpsc::Sender<Incoming>) -> bool {
    let socket = match std::net::UdpSocket::bind("127.0.0.1:42069") {
        Ok(s) => s,
        Err(e) => {
            log::warn!("notifications: can't bind XSOverlay port 42069 ({e}) — is WayVR/XSOverlay running? XSO notifications off");
            return false;
        }
    };
    let _ = socket.set_read_timeout(Some(Duration::from_millis(500)));
    std::thread::Builder::new()
        .name("notif-xso".into())
        .spawn(move || {
            let mut buf = vec![0u8; 256 * 1024]; // VRCX embeds base64 icons
            loop {
                let n = match socket.recv(&mut buf) {
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(e) => {
                        log::warn!("notifications: XSO socket error: {e}");
                        break;
                    }
                };
                let Ok(txt) = std::str::from_utf8(&buf[..n]) else { continue };
                let msg = match serde_json::from_str::<XsoMessage>(txt.trim_end_matches('\0')) {
                    Ok(m) => m,
                    Err(e) => {
                        log::debug!("notifications: bad XSO message: {e}");
                        continue;
                    }
                };
                if msg.messageType != 1 {
                    continue; // 2 = media player info: not a toast
                }
                let icon = msg
                    .icon
                    .as_deref()
                    .filter(|_| msg.useBase64Icon.unwrap_or(false))
                    .and_then(decode_icon);
                let app = msg.sourceApp.clone().unwrap_or_else(|| "XSOverlay".into());
                let _ = tx.send(Incoming {
                    title: msg.title.clone().unwrap_or_else(|| app.clone()),
                    app,
                    body: msg.content.unwrap_or_default(),
                    timeout: msg.timeout.unwrap_or(5.0).clamp(1.0, 30.0),
                    icon,
                    source: Source::XsOverlay,
                });
            }
        })
        .expect("spawn XSO listener");
    log::info!("notifications: listening for XSOverlay messages on udp/42069");
    true
}

fn decode_icon(b64: &str) -> Option<egui::ColorImage> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.thumbnail(64, 64).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}
