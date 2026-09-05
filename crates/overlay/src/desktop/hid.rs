//! Virtual mouse + keyboard through `/dev/uinput`. A kernel-level input device
//! works on every compositor (KWin included, which has no wlr virtual-pointer
//! protocol), needs no portal dance, and is exactly what WayVR uses.
//!
//! The mouse is an *absolute* device spanning the whole desktop: the
//! compositor maps its 0..32767 range onto the bounding box of all outputs, so
//! we translate desktop-logical coordinates into that range.
use std::fs::File;

use anyhow::{anyhow, Result};
use input_linux::{
    AbsoluteAxis, AbsoluteInfo, AbsoluteInfoSetup, EventKind, InputId, Key, RelativeAxis, UInputHandle,
};

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;

const EV_SYN: u16 = 0x0;
const EV_KEY: u16 = 0x1;
const EV_REL: u16 = 0x2;
const EV_ABS: u16 = 0x3;
const ABS_MAX: f64 = 32767.0;

pub struct UInput {
    mouse: UInputHandle<File>,
    /// Wired to the on-panel keyboard in a later phase.
    #[allow(dead_code)]
    keyboard: UInputHandle<File>,
    /// Desktop bounding box in logical coordinates.
    origin: (f64, f64),
    extent: (f64, f64),
    last_pos: Option<(f64, f64)>,
}

fn now() -> libc::timeval {
    let mut t = libc::timeval { tv_sec: 0, tv_usec: 0 };
    unsafe { libc::gettimeofday(&mut t, std::ptr::null_mut()) };
    t
}

const fn ev(time: libc::timeval, type_: u16, code: u16, value: i32) -> libc::input_event {
    libc::input_event { time, type_, code, value }
}

impl UInput {
    pub fn open() -> Result<Self> {
        fn map_err(what: &'static str) -> impl Fn(std::io::Error) -> anyhow::Error {
            move |e| anyhow!("{what}: {e}")
        }
        let mouse_file = File::create("/dev/uinput").map_err(map_err("open /dev/uinput"))?;
        let kbd_file = File::create("/dev/uinput").map_err(map_err("open /dev/uinput"))?;
        let mouse = UInputHandle::new(mouse_file);
        let keyboard = UInputHandle::new(kbd_file);

        let abs = |axis| AbsoluteInfoSetup {
            axis,
            info: AbsoluteInfo { value: 0, minimum: 0, maximum: ABS_MAX as i32, fuzz: 0, flat: 0, resolution: 10 },
        };
        mouse.set_evbit(EventKind::Absolute).map_err(map_err("mouse evbit abs"))?;
        mouse.set_evbit(EventKind::Relative).map_err(map_err("mouse evbit rel"))?;
        mouse.set_evbit(EventKind::Key).map_err(map_err("mouse evbit key"))?;
        mouse.set_absbit(AbsoluteAxis::X).map_err(map_err("absbit x"))?;
        mouse.set_absbit(AbsoluteAxis::Y).map_err(map_err("absbit y"))?;
        mouse.set_relbit(RelativeAxis::Wheel).map_err(map_err("relbit wheel"))?;
        mouse.set_relbit(RelativeAxis::HorizontalWheel).map_err(map_err("relbit hwheel"))?;
        mouse.set_relbit(RelativeAxis::WheelHiRes).map_err(map_err("relbit wheel hires"))?;
        mouse.set_relbit(RelativeAxis::HorizontalWheelHiRes).map_err(map_err("relbit hwheel hires"))?;
        for code in BTN_LEFT..=BTN_MIDDLE {
            let key = Key::from_code(code).map_err(|_| anyhow!("bad button code {code}"))?;
            mouse.set_keybit(key).map_err(map_err("mouse keybit"))?;
        }
        mouse
            .create(
                &InputId { bustype: 0x03, vendor: 0x4d44, product: 0x0001, version: 1 },
                b"Monadeck Mouse\0",
                0,
                &[abs(AbsoluteAxis::X), abs(AbsoluteAxis::Y)],
            )
            .map_err(map_err("create mouse"))?;

        keyboard.set_evbit(EventKind::Key).map_err(map_err("kbd evbit"))?;
        for code in 1..=248u16 {
            if let Ok(key) = Key::from_code(code) {
                let _ = keyboard.set_keybit(key);
            }
        }
        keyboard
            .create(&InputId { bustype: 0x03, vendor: 0x4d44, product: 0x0002, version: 1 }, b"Monadeck Keyboard\0", 0, &[])
            .map_err(map_err("create keyboard"))?;

        Ok(Self { mouse, keyboard, origin: (0.0, 0.0), extent: (1.0, 1.0), last_pos: None })
    }

    /// Bounding box of all outputs (logical coordinates).
    pub fn set_desktop(&mut self, origin: (f64, f64), extent: (f64, f64)) {
        self.origin = origin;
        self.extent = (extent.0.max(1.0), extent.1.max(1.0));
    }

    /// Move the cursor to a desktop-logical position.
    pub fn mouse_move(&mut self, x: f64, y: f64) {
        let ax = ((x - self.origin.0) / self.extent.0 * ABS_MAX).round().clamp(0.0, ABS_MAX) as i32;
        let ay = ((y - self.origin.1) / self.extent.1 * ABS_MAX).round().clamp(0.0, ABS_MAX) as i32;
        self.last_pos = Some((x, y));
        let t = now();
        let events = [ev(t, EV_ABS, AbsoluteAxis::X as u16, ax), ev(t, EV_ABS, AbsoluteAxis::Y as u16, ay), ev(t, EV_SYN, 0, 0)];
        if let Err(e) = self.mouse.write(&events) {
            log::warn!("uinput move: {e}");
        }
    }

    pub fn button(&mut self, code: u16, down: bool) {
        let t = now();
        let events = [ev(t, EV_KEY, code, down as i32), ev(t, EV_SYN, 0, 0)];
        if let Err(e) = self.mouse.write(&events) {
            log::warn!("uinput button: {e}");
        }
    }

    /// Scroll by `(dx, dy)` in wheel "notches" (fractional ok; 1.0 = one click).
    pub fn wheel(&mut self, dx: f32, dy: f32) {
        let t = now();
        let hx = (dx * 120.0).round() as i32;
        let hy = (dy * 120.0).round() as i32;
        if hx == 0 && hy == 0 {
            return;
        }
        let events = [
            ev(t, EV_REL, RelativeAxis::WheelHiRes as u16, hy),
            ev(t, EV_REL, RelativeAxis::HorizontalWheelHiRes as u16, hx),
            ev(t, EV_SYN, 0, 0),
        ];
        if let Err(e) = self.mouse.write(&events) {
            log::warn!("uinput wheel: {e}");
        }
    }

    /// Press/release a key by evdev code (`KEY_*` from linux/input-event-codes.h).
    #[allow(dead_code)]
    pub fn key(&mut self, code: u16, down: bool) {
        let t = now();
        let events = [ev(t, EV_KEY, code, down as i32), ev(t, EV_SYN, 0, 0)];
        if let Err(e) = self.keyboard.write(&events) {
            log::warn!("uinput key: {e}");
        }
    }

    #[allow(dead_code)]
    pub fn last_pos(&self) -> Option<(f64, f64)> {
        self.last_pos
    }
}
