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

/// Labels per evdev keycode: `[base, shift, altgr]`.
pub struct KeyLabels {
    pub layout_name: String,
    pub labels: HashMap<u16, [String; 3]>,
}

const EVDEV_OFFSET: u32 = 8;
const KEY_LEFTSHIFT: u32 = 42;
const KEY_RIGHTALT: u32 = 100;

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
        return KeyLabels { layout_name: "?".into(), labels: HashMap::new() };
    };
    let layout_name = keymap.layout_get_name(0).to_string();
    let base = xkb::State::new(&keymap);
    let mut shift = xkb::State::new(&keymap);
    shift.update_key(xkb::Keycode::new(KEY_LEFTSHIFT + EVDEV_OFFSET), xkb::KeyDirection::Down);
    let mut altgr = xkb::State::new(&keymap);
    altgr.update_key(xkb::Keycode::new(KEY_RIGHTALT + EVDEV_OFFSET), xkb::KeyDirection::Down);
    let mut labels = HashMap::new();
    for &code in LABELLED {
        let kc = xkb::Keycode::new(code as u32 + EVDEV_OFFSET);
        let l = [base.key_get_utf8(kc), shift.key_get_utf8(kc), altgr.key_get_utf8(kc)];
        labels.insert(code, l.map(|s| s.chars().filter(|c| !c.is_control()).collect()));
    }
    log::info!("desktop: keyboard layout '{layout_name}' ({} labelled keys)", labels.len());
    KeyLabels { layout_name, labels }
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
