//! Key labels for the VR keyboard, from the user's *real* xkb keymap: the
//! Wayland seat hands us the compiled keymap (`wl_keyboard.keymap`), and
//! xkbcommon tells us what each evdev key produces at the base, Shift and
//! AltGr levels. Falls back to a US map when no Wayland seat is reachable.
use std::collections::HashMap;
use std::os::fd::AsRawFd;

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_keyboard::{self, WlKeyboard};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use xkbcommon::xkb;

/// Labels per evdev keycode `[base, shift, altgr]`, for every configured
/// layout (xkb group), plus which one is active.
pub struct KeyLabels {
    pub layout_names: Vec<String>,
    pub per_layout: Vec<HashMap<u16, [String; 3]>>,
    pub current: usize,
}

impl KeyLabels {
    pub fn current(&self) -> &HashMap<u16, [String; 3]> {
        static EMPTY: std::sync::OnceLock<HashMap<u16, [String; 3]>> = std::sync::OnceLock::new();
        self.per_layout.get(self.current).unwrap_or_else(|| EMPTY.get_or_init(HashMap::new))
    }

    pub fn layout_name(&self) -> String {
        self.layout_names.get(self.current).cloned().unwrap_or_default()
    }
}

const EVDEV_OFFSET: u32 = 8;

/// Evdev codes we want labels for (printable/main-block keys).
const LABELLED: &[u16] = &[
    2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, // number row
    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, // qwerty row
    30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, // home row + grave
    43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 86, // backslash, bottom row, 102nd
];

pub fn load() -> KeyLabels {
    let ctx = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let keymap = fetch_wayland_keymap()
        .and_then(|s| xkb::Keymap::new_from_string(&ctx, s, xkb::KEYMAP_FORMAT_TEXT_V1, xkb::KEYMAP_COMPILE_NO_FLAGS))
        .or_else(|| {
            log::warn!("desktop: no Wayland keymap; labelling the VR keyboard as US");
            xkb::Keymap::new_from_names(&ctx, "", "", "us", "", None, xkb::KEYMAP_COMPILE_NO_FLAGS)
        });
    let Some(keymap) = keymap else {
        return KeyLabels { layout_names: vec!["?".into()], per_layout: vec![HashMap::new()], current: 0 };
    };
    let shift_mask = 1u32 << keymap.mod_get_index(xkb::MOD_NAME_SHIFT);
    let altgr_mask = 1u32 << keymap.mod_get_index("Mod5");
    let mut layout_names = Vec::new();
    let mut per_layout = Vec::new();
    for layout in 0..keymap.num_layouts().max(1) {
        layout_names.push(keymap.layout_get_name(layout).to_string());
        let mut labels = HashMap::new();
        let mut states = [xkb::State::new(&keymap), xkb::State::new(&keymap), xkb::State::new(&keymap)];
        states[0].update_mask(0, 0, 0, 0, 0, layout);
        states[1].update_mask(shift_mask, 0, 0, 0, 0, layout);
        states[2].update_mask(altgr_mask, 0, 0, 0, 0, layout);
        for &code in LABELLED {
            let kc = xkb::Keycode::new(code as u32 + EVDEV_OFFSET);
            let l = [states[0].key_get_utf8(kc), states[1].key_get_utf8(kc), states[2].key_get_utf8(kc)];
            labels.insert(code, l.map(|s| s.chars().filter(|c| !c.is_control()).collect()));
        }
        per_layout.push(labels);
    }
    let current = kde_current_layout().unwrap_or(0).min(per_layout.len().saturating_sub(1));
    log::info!("desktop: keyboard layouts {layout_names:?}, active #{current}");
    KeyLabels { layout_names, per_layout, current }
}

const KDE_DEST: &str = "org.kde.keyboard";
const KDE_PATH: &str = "/Layouts";
const KDE_IFACE: &str = "org.kde.KeyboardLayouts";

/// Active layout index from KDE (org.kde.keyboard), if that's the desktop.
pub fn kde_current_layout() -> Option<usize> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let reply = conn.call_method(Some(KDE_DEST), KDE_PATH, Some(KDE_IFACE), "getLayout", &()).ok()?;
    let idx: u32 = reply.body().deserialize().ok()?;
    Some(idx as usize)
}

/// Ask KDE to switch the system keyboard layout.
pub fn kde_set_layout(index: usize) -> bool {
    let Ok(conn) = zbus::blocking::Connection::session() else { return false };
    match conn.call_method(Some(KDE_DEST), KDE_PATH, Some(KDE_IFACE), "setLayout", &(index as u32)) {
        Ok(reply) => reply.body().deserialize::<bool>().unwrap_or(true),
        Err(e) => {
            log::warn!("desktop: KDE setLayout failed: {e}");
            false
        }
    }
}

#[derive(Default)]
struct State {
    keyboard: Option<WlKeyboard>,
    keymap: Option<String>,
}

fn fetch_wayland_keymap() -> Option<String> {
    let conn = Connection::connect_to_env().ok()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
    let qh = queue.handle();
    let _seat: WlSeat = globals.bind(&qh, 1..=9, ()).ok()?;
    let mut st = State::default();
    // seat capabilities -> keyboard -> keymap: three roundtrips at most.
    for _ in 0..3 {
        queue.roundtrip(&mut st).ok()?;
        if st.keymap.is_some() {
            break;
        }
    }
    if let Some(k) = st.keyboard.take() {
        k.release();
    }
    st.keymap
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlSeat, ()> for State {
    fn event(st: &mut Self, seat: &WlSeat, event: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event {
            if caps.contains(wl_seat::Capability::Keyboard) && st.keyboard.is_none() {
                st.keyboard = Some(seat.get_keyboard(qh, ()));
            }
        }
    }
}

impl Dispatch<WlKeyboard, ()> for State {
    fn event(st: &mut Self, _: &WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let wl_keyboard::Event::Keymap { format, fd, size } = event {
            if format != WEnum::Value(wl_keyboard::KeymapFormat::XkbV1) {
                return;
            }
            let len = size as usize;
            let p = unsafe { libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_PRIVATE, fd.as_raw_fd(), 0) };
            if p == libc::MAP_FAILED {
                return;
            }
            let bytes = unsafe { std::slice::from_raw_parts(p as *const u8, len) };
            // NUL-terminated text.
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(len);
            st.keymap = String::from_utf8(bytes[..end].to_vec()).ok();
            unsafe { libc::munmap(p, len) };
        }
    }
}
