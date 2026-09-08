//! Desktop + XSOverlay notifications in the headset (the role WayVR played):
//!
//! - D-Bus: a monitor on `org.freedesktop.Notifications.Notify` (BecomeMonitor,
//!   so the desktop's own notification daemon keeps working untouched);
//! - XSOverlay protocol: JSON over UDP on 127.0.0.1:42069 — what VRCX and other
//!   VR tools send to XSOverlay.
//!
//! Both feed a queue the overlay turns into toasts. Desktop notifications bring
//! their app icon along when it can be found: inline `image-data`, an image
//! path, or an icon name resolved through the hicolor theme (and the sender's
//! `.desktop` entry when the hint is a desktop id).
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use base64::Engine;
use zbus::zvariant::Value;

pub struct Incoming {
    pub app: String,
    pub title: String,
    pub body: String,
    /// Seconds to show.
    pub timeout: f32,
    /// Decoded icon (XSOverlay base64 / freedesktop image hints), small.
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
            let mut icons = IconCache::default();
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
                let Ok((app, _replaces, app_icon, summary, body, _actions, hints, expire)) = msg.body().deserialize::<Body>() else {
                    continue;
                };
                let title = if summary.is_empty() { app.clone() } else { summary };
                let timeout = if expire > 0 { (expire as f32 / 1000.0).clamp(2.0, 15.0) } else { 5.0 };
                let icon = desktop_icon(&app_icon, &hints, &mut icons);
                let _ = tx.send(Incoming { app, title, body: strip_markup(&body), timeout, icon, source: Source::Desktop });
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

// --- desktop icons -------------------------------------------------------------

/// Resolved icon names / paths, so a chatty app doesn't hit the disk per toast.
#[derive(Default)]
struct IconCache(Vec<(String, Option<egui::ColorImage>)>);

impl IconCache {
    fn get(&mut self, key: &str, load: impl FnOnce() -> Option<egui::ColorImage>) -> Option<egui::ColorImage> {
        if let Some((_, img)) = self.0.iter().find(|(k, _)| k == key) {
            return img.clone();
        }
        let img = load();
        if self.0.len() >= 32 {
            self.0.remove(0);
        }
        self.0.push((key.to_string(), img.clone()));
        img
    }
}

/// The icon a desktop notification wants shown, in the spec's priority order:
/// `image-data` (pixels inline), `image-path`, the `app_icon` argument, then
/// the icon of the `desktop-entry` it names.
fn desktop_icon(app_icon: &str, hints: &HashMap<String, zbus::zvariant::OwnedValue>, cache: &mut IconCache) -> Option<egui::ColorImage> {
    let hint = |k: &str| hints.get(k).map(|v| &**v);
    if let Some(img) = hint("image-data").or_else(|| hint("image_data")).or_else(|| hint("icon_data")).and_then(image_data) {
        return Some(img);
    }
    if let Some(Value::Str(p)) = hint("image-path").or_else(|| hint("image_path")) {
        if let Some(img) = cache.get(p, || resolve_icon(p)) {
            return Some(img);
        }
    }
    if !app_icon.trim().is_empty() {
        if let Some(img) = cache.get(app_icon, || resolve_icon(app_icon)) {
            return Some(img);
        }
    }
    if let Some(Value::Str(id)) = hint("desktop-entry") {
        let key = format!("desktop:{id}");
        return cache.get(&key, || desktop_entry_icon(id).and_then(|name| resolve_icon(&name)));
    }
    None
}

/// The spec's `iiibiiay` image: width, height, rowstride, has_alpha,
/// bits_per_sample, channels, data.
fn image_data(v: &Value) -> Option<egui::ColorImage> {
    let Value::Structure(s) = v else { return None };
    let f = s.fields();
    if f.len() != 7 {
        return None;
    }
    let int = |v: &Value| match v {
        Value::I32(x) => Some(*x),
        _ => None,
    };
    let (w, h, stride) = (int(&f[0])? as usize, int(&f[1])? as usize, int(&f[2])? as usize);
    let has_alpha = matches!(f[3], Value::Bool(true));
    let channels = int(&f[5])? as usize;
    if int(&f[4])? != 8 || !(3..=4).contains(&channels) || w == 0 || h == 0 || w > 1024 || h > 1024 {
        return None;
    }
    let Value::Array(a) = &f[6] else { return None };
    let data: Vec<u8> = a.inner().iter().map(|b| if let Value::U8(b) = b { *b } else { 0 }).collect();
    if data.len() < stride * (h - 1) + w * channels {
        return None;
    }
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let i = y * stride + x * channels;
            rgba.extend_from_slice(&data[i..i + 3]);
            rgba.push(if has_alpha && channels == 4 { data[i + 3] } else { 255 });
        }
    }
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba)?;
    Some(to_color_image(image::DynamicImage::ImageRgba8(img)))
}

/// Down to chip size (never up: a 16 px tray icon stays crisp at 16 px).
fn to_color_image(img: image::DynamicImage) -> egui::ColorImage {
    let img = if img.width() > 64 || img.height() > 64 { img.thumbnail(64, 64) } else { img };
    let img = img.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw())
}

/// An icon spec as apps send it: an absolute path, a `file://` URI, or a theme
/// icon name (sometimes with a stray extension).
fn resolve_icon(spec: &str) -> Option<egui::ColorImage> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    let path = spec.strip_prefix("file://").unwrap_or(spec);
    if path.starts_with('/') {
        return load_icon_file(Path::new(path));
    }
    let name = match path.rsplit_once('.') {
        Some((n, "png" | "svg" | "xpm" | "jpg")) => n,
        _ => path,
    };
    icon_by_name(name)
}

fn load_icon_file(p: &Path) -> Option<egui::ColorImage> {
    if p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
        return None; // no SVG rasteriser on board
    }
    let bytes = std::fs::read(p).ok()?;
    image::load_from_memory(&bytes).ok().map(to_color_image)
}

/// The XDG data roots that hold `icons/` and `applications/`, user first.
fn data_roots() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut roots = vec![
        PathBuf::from(format!("{home}/.local/share")),
        PathBuf::from(format!("{home}/.local/share/flatpak/exports/share")),
        PathBuf::from("/var/lib/flatpak/exports/share"),
    ];
    if let Ok(dirs) = std::env::var("XDG_DATA_DIRS") {
        roots.extend(dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    }
    for d in ["/usr/local/share", "/usr/share"] {
        roots.push(PathBuf::from(d));
    }
    roots
}

/// A theme icon by name: hicolor's app icons at a toast-friendly size (PNG
/// only), then the pixmaps fallback.
pub(crate) fn icon_by_name(name: &str) -> Option<egui::ColorImage> {
    if name.is_empty() || name.contains('/') {
        return None;
    }
    let roots = data_roots();
    for size in [64, 48, 128, 96, 256, 72, 32, 512] {
        for root in &roots {
            for sub in ["apps", "status", "devices", "categories"] {
                let p = root.join(format!("icons/hicolor/{size}x{size}/{sub}/{name}.png"));
                if p.is_file() {
                    return load_icon_file(&p);
                }
            }
        }
    }
    for root in &roots {
        for cand in [root.join(format!("pixmaps/{name}.png")), root.join(format!("icons/{name}.png"))] {
            if cand.is_file() {
                return load_icon_file(&cand);
            }
        }
    }
    log::debug!("notifications: no PNG icon for {name:?}");
    None
}

/// `Icon=` of the desktop entry `id` (with or without `.desktop`).
fn desktop_entry_icon(id: &str) -> Option<String> {
    let file = if id.ends_with(".desktop") { id.to_string() } else { format!("{id}.desktop") };
    if file.contains('/') {
        return None;
    }
    for root in data_roots() {
        let p = root.join("applications").join(&file);
        let Ok(txt) = std::fs::read_to_string(&p) else { continue };
        let mut in_entry = false;
        for line in txt.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_entry = line == "[Desktop Entry]";
            } else if in_entry {
                if let Some(v) = line.strip_prefix("Icon=") {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
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
                // Base64 pixels, or one of XSOverlay's built-in icon names
                // (`default`, `error`, `warning`) which map to our own glyphs.
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
    image::load_from_memory(&bytes).ok().map(to_color_image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_stripped() {
        assert_eq!(strip_markup("<b>hi</b> &amp; <a href='x'>there</a>"), "hi & there");
    }

    #[test]
    fn image_data_decodes_rgb_and_rgba() {
        let px = |w: usize, h: usize, ch: usize, stride: usize| {
            let mut data = vec![0u8; stride * h];
            for y in 0..h {
                for x in 0..w {
                    let i = y * stride + x * ch;
                    data[i] = 200;
                    data[i + 1] = 30;
                    data[i + 2] = 30;
                    if ch == 4 {
                        data[i + 3] = 255;
                    }
                }
            }
            Value::Structure(
                zbus::zvariant::StructureBuilder::new()
                    .add_field(w as i32)
                    .add_field(h as i32)
                    .add_field(stride as i32)
                    .add_field(ch == 4)
                    .add_field(8i32)
                    .add_field(ch as i32)
                    .add_field(data)
                    .build()
                    .unwrap(),
            )
        };
        let rgb = image_data(&px(4, 3, 3, 16)).unwrap();
        assert_eq!(rgb.size, [4, 3]);
        assert_eq!(rgb.pixels[0], egui::Color32::from_rgb(200, 30, 30));
        let rgba = image_data(&px(2, 2, 4, 8)).unwrap();
        assert_eq!(rgba.pixels[3].a(), 255);
        assert!(image_data(&Value::I32(3)).is_none());
    }

    #[test]
    fn icon_specs() {
        assert!(resolve_icon("").is_none());
        assert!(resolve_icon("/definitely/not/here.png").is_none());
        assert!(icon_by_name("../etc/passwd").is_none());
    }
}
