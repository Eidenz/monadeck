//! A virtual Xbox 360 pad (plus a relative mouse) through `/dev/uinput`, for
//! gaming mode: VR controller inputs become a gamepad every flat game already
//! understands. The device identifies itself as a wired X360 pad (`045e:028e`),
//! the one GUID SDL's controller database, Steam Input and Proton's xinput all
//! know — the same trick Sunshine uses for streamed controllers.
//!
//! Force feedback is wired the other way: the game uploads rumble effects
//! into the device and plays them; we read those requests back off the uinput
//! fd and turn them into controller haptics.
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use input_linux::sys as sys;
pub use monadeck_core::gamepad_profiles::{Axis, Btn};
use input_linux::{
    AbsoluteAxis, AbsoluteInfo, AbsoluteInfoSetup, EventKind, ForceFeedbackKind, InputId, Key, RelativeAxis, UInputHandle,
};

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;
const EV_FF: u16 = 0x15;
/// uinput's own event type: force-feedback upload/erase requests.
const EV_UINPUT: u16 = 0x0101;
const UI_FF_UPLOAD: u16 = 1;
const UI_FF_ERASE: u16 = 2;
const FF_GAIN: u16 = 0x60;

/// evdev key code of a button (None for the d-pad, which is a hat axis pair).
fn btn_code(b: Btn) -> Option<u16> {
    Some(match b {
        Btn::A => 0x130,     // BTN_SOUTH
        Btn::B => 0x131,     // BTN_EAST
        Btn::X => 0x133,     // BTN_NORTH (yes: X is "north" in evdev)
        Btn::Y => 0x134,     // BTN_WEST
        Btn::LB => 0x136,    // BTN_TL
        Btn::RB => 0x137,    // BTN_TR
        Btn::Back => 0x13a,  // BTN_SELECT
        Btn::Start => 0x13b, // BTN_START
        Btn::Guide => 0x13c, // BTN_MODE
        Btn::L3 => 0x13d,    // BTN_THUMBL
        Btn::R3 => 0x13e,    // BTN_THUMBR
        Btn::DpadUp | Btn::DpadDown | Btn::DpadLeft | Btn::DpadRight => return None,
    })
}

fn axis_code(a: Axis) -> u16 {
    match a {
        Axis::LX => 0x00, // ABS_X
        Axis::LY => 0x01, // ABS_Y
        Axis::RX => 0x03, // ABS_RX
        Axis::RY => 0x04, // ABS_RY
        Axis::LT => 0x02, // ABS_Z
        Axis::RT => 0x05, // ABS_RZ
    }
}

/// Kernel value for a normalised input (-1..1 sticks, 0..1 triggers).
fn axis_raw(a: Axis, v: f32) -> i32 {
    match a {
        // ABS_Y / ABS_RY point DOWN on a real pad; OpenXR sticks point up.
        Axis::LY | Axis::RY => (-v.clamp(-1.0, 1.0) * 32767.0).round() as i32,
        Axis::LX | Axis::RX => (v.clamp(-1.0, 1.0) * 32767.0).round() as i32,
        Axis::LT | Axis::RT => (v.clamp(0.0, 1.0) * 255.0).round() as i32,
    }
}

const ABS_HAT0X: u16 = 0x10;
const ABS_HAT0Y: u16 = 0x11;

/// Rumble the game is asking for right now, 0..1 per motor.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rumble {
    /// Low-frequency motor (the left one on a real pad).
    pub strong: f32,
    /// High-frequency motor (right).
    pub weak: f32,
}

impl Rumble {
    pub fn is_on(&self) -> bool {
        self.strong > 0.01 || self.weak > 0.01
    }
}

/// A playing rumble effect: magnitudes + when it stops (None = until stopped).
struct Playing {
    strong: u16,
    weak: u16,
    until: Option<Instant>,
}

fn now() -> libc::timeval {
    let mut t = libc::timeval { tv_sec: 0, tv_usec: 0 };
    unsafe { libc::gettimeofday(&mut t, std::ptr::null_mut()) };
    t
}

const fn ev(time: libc::timeval, type_: u16, code: u16, value: i32) -> libc::input_event {
    libc::input_event { time, type_, code, value }
}

pub struct VirtualPad {
    dev: UInputHandle<File>,
    buttons: HashMap<Btn, bool>,
    axes: HashMap<Axis, i32>,
    hat: (i32, i32),
    pending: Vec<libc::input_event>,
    /// Uploaded effects by id: (strong, weak, length ms).
    effects: HashMap<i16, (u16, u16, u16)>,
    playing: HashMap<i16, Playing>,
    gain: f32,
    /// Short taps (Guide from the watch) release themselves.
    taps: Vec<(Btn, Instant)>,
    pub rumble: Rumble,
}

impl VirtualPad {
    pub fn open() -> Result<Self> {
        let err = |what: &'static str| move |e: std::io::Error| anyhow!("{what}: {e}");
        // Read+write: rumble requests come back to us on this fd.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/uinput")
            .map_err(err("open /dev/uinput"))?;
        let dev = UInputHandle::new(file);
        dev.set_evbit(EventKind::Key).map_err(err("pad evbit key"))?;
        dev.set_evbit(EventKind::Absolute).map_err(err("pad evbit abs"))?;
        dev.set_evbit(EventKind::ForceFeedback).map_err(err("pad evbit ff"))?;
        for b in Btn::ALL {
            if let Some(code) = btn_code(b) {
                let key = Key::from_code(code).map_err(|_| anyhow!("bad button code {code}"))?;
                dev.set_keybit(key).map_err(err("pad keybit"))?;
            }
        }
        let stick = |axis| AbsoluteInfoSetup {
            axis,
            info: AbsoluteInfo { value: 0, minimum: -32768, maximum: 32767, fuzz: 16, flat: 128, resolution: 0 },
        };
        let trigger = |axis| AbsoluteInfoSetup {
            axis,
            info: AbsoluteInfo { value: 0, minimum: 0, maximum: 255, fuzz: 0, flat: 0, resolution: 0 },
        };
        let hat = |axis| AbsoluteInfoSetup {
            axis,
            info: AbsoluteInfo { value: 0, minimum: -1, maximum: 1, fuzz: 0, flat: 0, resolution: 0 },
        };
        let abs = [
            stick(AbsoluteAxis::X),
            stick(AbsoluteAxis::Y),
            stick(AbsoluteAxis::RX),
            stick(AbsoluteAxis::RY),
            trigger(AbsoluteAxis::Z),
            trigger(AbsoluteAxis::RZ),
            hat(AbsoluteAxis::Hat0X),
            hat(AbsoluteAxis::Hat0Y),
        ];
        for a in &abs {
            dev.set_absbit(a.axis).map_err(err("pad absbit"))?;
        }
        dev.set_ffbit(ForceFeedbackKind::Rumble).map_err(err("pad ffbit rumble"))?;
        dev.set_ffbit(ForceFeedbackKind::Periodic).map_err(err("pad ffbit periodic"))?;
        dev.set_ffbit(ForceFeedbackKind::Square).map_err(err("pad ffbit square"))?;
        dev.set_ffbit(ForceFeedbackKind::Triangle).map_err(err("pad ffbit triangle"))?;
        dev.set_ffbit(ForceFeedbackKind::Sine).map_err(err("pad ffbit sine"))?;
        dev.set_ffbit(ForceFeedbackKind::Gain).map_err(err("pad ffbit gain"))?;
        dev.create(
            // Wired Xbox 360 pad as xpad reports it (bcdDevice 0x0114): GUID
            // 030000005e0400008e02000014010000 in SDL's database.
            &InputId { bustype: 0x03, vendor: 0x045e, product: 0x028e, version: 0x0114 },
            b"Microsoft X-Box 360 pad\0",
            16,
            &abs,
        )
        .map_err(err("create pad"))?;
        Ok(Self {
            dev,
            buttons: HashMap::new(),
            axes: HashMap::new(),
            hat: (0, 0),
            pending: Vec::new(),
            effects: HashMap::new(),
            playing: HashMap::new(),
            gain: 1.0,
            taps: Vec::new(),
            rumble: Rumble::default(),
        })
    }

    /// The evdev node the kernel gave this pad (`/dev/input/eventN`), if it
    /// can be found through sysfs.
    pub fn node(&self) -> Option<String> {
        self.dev.evdev_path().ok().map(|p| p.to_string_lossy().to_string())
    }

    pub fn button(&mut self, b: Btn, down: bool) {
        if self.buttons.insert(b, down) == Some(down) {
            return;
        }
        let t = now();
        match btn_code(b) {
            Some(code) => self.pending.push(ev(t, EV_KEY, code, down as i32)),
            None => {
                let want = |neg: Btn, pos: Btn, buttons: &HashMap<Btn, bool>| -> i32 {
                    let n = buttons.get(&neg).copied().unwrap_or(false);
                    let p = buttons.get(&pos).copied().unwrap_or(false);
                    (p as i32) - (n as i32)
                };
                let x = want(Btn::DpadLeft, Btn::DpadRight, &self.buttons);
                let y = want(Btn::DpadUp, Btn::DpadDown, &self.buttons);
                if x != self.hat.0 {
                    self.pending.push(ev(t, EV_ABS, ABS_HAT0X, x));
                }
                if y != self.hat.1 {
                    self.pending.push(ev(t, EV_ABS, ABS_HAT0Y, y));
                }
                self.hat = (x, y);
            }
        }
    }

    /// Press-and-release (about 80 ms) — the watch's Guide button.
    pub fn tap(&mut self, b: Btn) {
        self.button(b, true);
        self.taps.push((b, Instant::now()));
    }

    /// Normalised axis value (-1..1 sticks, 0..1 triggers).
    pub fn axis(&mut self, a: Axis, v: f32) {
        let raw = axis_raw(a, v);
        if self.axes.insert(a, raw) == Some(raw) {
            return;
        }
        self.pending.push(ev(now(), EV_ABS, axis_code(a), raw));
    }

    /// Everything back to neutral (leaving gaming mode, dashboard summoned…).
    pub fn release_all(&mut self) {
        for b in Btn::ALL {
            self.button(b, false);
        }
        for a in Axis::ALL {
            self.axis(a, 0.0);
        }
        self.taps.clear();
        self.flush();
    }

    /// Send this frame's changes as one report.
    pub fn flush(&mut self) {
        let t = Instant::now();
        let mut done = Vec::new();
        for (i, (b, at)) in self.taps.iter().enumerate() {
            if t.duration_since(*at) > Duration::from_millis(80) {
                done.push((i, *b));
            }
        }
        for (i, b) in done.into_iter().rev() {
            self.taps.remove(i);
            self.button(b, false);
        }
        if self.pending.is_empty() {
            return;
        }
        self.pending.push(ev(now(), EV_SYN, 0, 0));
        if let Err(e) = self.dev.write(&self.pending) {
            log::warn!("gamepad: write: {e}");
        }
        self.pending.clear();
    }

    /// Service the game's force-feedback requests and update `rumble`.
    pub fn poll(&mut self) -> Rumble {
        let mut buf = [libc::input_event { time: now(), type_: 0, code: 0, value: 0 }; 16];
        loop {
            let n = match self.dev.read(&mut buf) {
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => 0,
                Err(e) => {
                    log::warn!("gamepad: read: {e}");
                    0
                }
            };
            if n == 0 {
                break;
            }
            for e in &buf[..n] {
                match (e.type_, e.code) {
                    (EV_UINPUT, UI_FF_UPLOAD) => self.ff_upload(e.value as u32),
                    (EV_UINPUT, UI_FF_ERASE) => self.ff_erase(e.value as u32),
                    (EV_FF, FF_GAIN) => self.gain = (e.value.clamp(0, 0xffff) as f32) / 65535.0,
                    (EV_FF, code) => self.ff_play(code as i16, e.value),
                    _ => {}
                }
            }
        }
        let t = Instant::now();
        self.playing.retain(|_, p| p.until.is_none_or(|u| u > t));
        let (mut strong, mut weak) = (0u32, 0u32);
        for p in self.playing.values() {
            strong = strong.max(p.strong as u32);
            weak = weak.max(p.weak as u32);
        }
        self.rumble = Rumble {
            strong: strong as f32 / 65535.0 * self.gain,
            weak: weak as f32 / 65535.0 * self.gain,
        };
        self.rumble
    }

    fn ff_upload(&mut self, request_id: u32) {
        // SAFETY: plain C structs handed to the kernel's begin/end ioctls; the
        // kernel fills `effect` in `begin`, we only read it.
        let mut up: sys::uinput_ff_upload = unsafe { std::mem::zeroed() };
        up.request_id = request_id;
        if let Err(e) = self.dev.ff_upload_begin(&mut up) {
            log::warn!("gamepad: ff upload begin: {e}");
            return;
        }
        let effect = up.effect;
        let id = effect.id;
        let (strong, weak) = match effect.type_ {
            sys::FF_RUMBLE => {
                let r = <&sys::ff_effect_union>::from(&effect).rumble();
                (r.strong_magnitude, r.weak_magnitude)
            }
            sys::FF_PERIODIC => {
                // Games sometimes rumble through a periodic effect: treat its
                // magnitude as both motors.
                let p = <&sys::ff_effect_union>::from(&effect).periodic();
                let m = p.magnitude.unsigned_abs().saturating_mul(2);
                (m, m)
            }
            _ => (0, 0),
        };
        self.effects.insert(id, (strong, weak, effect.replay.length));
        // A re-upload of a playing effect changes its motors live.
        if let Some(p) = self.playing.get_mut(&id) {
            p.strong = strong;
            p.weak = weak;
        }
        up.retval = 0;
        if let Err(e) = self.dev.ff_upload_end(&up) {
            log::warn!("gamepad: ff upload end: {e}");
        }
    }

    fn ff_erase(&mut self, request_id: u32) {
        let mut er: sys::uinput_ff_erase = unsafe { std::mem::zeroed() };
        er.request_id = request_id;
        if let Err(e) = self.dev.ff_erase_begin(&mut er) {
            log::warn!("gamepad: ff erase begin: {e}");
            return;
        }
        let id = er.effect_id as i16;
        self.effects.remove(&id);
        self.playing.remove(&id);
        er.retval = 0;
        if let Err(e) = self.dev.ff_erase_end(&er) {
            log::warn!("gamepad: ff erase end: {e}");
        }
    }

    fn ff_play(&mut self, id: i16, count: i32) {
        if count <= 0 {
            self.playing.remove(&id);
            return;
        }
        if let Some(&(strong, weak, length)) = self.effects.get(&id) {
            let until = (length > 0).then(|| Instant::now() + Duration::from_millis(length as u64));
            self.playing.insert(id, Playing { strong, weak, until });
        }
    }
}

/// A relative mouse for stick-look and click bindings in remap profiles (the
/// desktop viewer's absolute mouse is the laser's; mixing the two on one
/// device confuses compositors).
pub struct PadMouse {
    dev: UInputHandle<File>,
    buttons: HashMap<u16, bool>,
    /// Sub-pixel remainder so slow stick motion still adds up.
    carry: (f32, f32),
}

impl PadMouse {
    pub fn open() -> Result<Self> {
        let err = |what: &'static str| move |e: std::io::Error| anyhow!("{what}: {e}");
        let file = File::create("/dev/uinput").map_err(err("open /dev/uinput"))?;
        let dev = UInputHandle::new(file);
        dev.set_evbit(EventKind::Key).map_err(err("mouse evbit key"))?;
        dev.set_evbit(EventKind::Relative).map_err(err("mouse evbit rel"))?;
        for axis in [RelativeAxis::X, RelativeAxis::Y, RelativeAxis::Wheel, RelativeAxis::HorizontalWheel, RelativeAxis::WheelHiRes, RelativeAxis::HorizontalWheelHiRes] {
            dev.set_relbit(axis).map_err(err("mouse relbit"))?;
        }
        for code in 0x110..=0x114u16 {
            let key = Key::from_code(code).map_err(|_| anyhow!("bad button code {code}"))?;
            dev.set_keybit(key).map_err(err("mouse keybit"))?;
        }
        dev.create(&InputId { bustype: 0x03, vendor: 0x4d44, product: 0x0003, version: 1 }, b"Monadeck Pad Mouse\0", 0, &[])
            .map_err(err("create pad mouse"))?;
        Ok(Self { dev, buttons: HashMap::new(), carry: (0.0, 0.0) })
    }

    /// Move by a fractional number of pixels.
    pub fn motion(&mut self, dx: f32, dy: f32) {
        self.carry.0 += dx;
        self.carry.1 += dy;
        let (ix, iy) = (self.carry.0.trunc() as i32, self.carry.1.trunc() as i32);
        if ix == 0 && iy == 0 {
            return;
        }
        self.carry.0 -= ix as f32;
        self.carry.1 -= iy as f32;
        let t = now();
        let events = [ev(t, EV_REL, 0x00, ix), ev(t, EV_REL, 0x01, iy), ev(t, EV_SYN, 0, 0)];
        if let Err(e) = self.dev.write(&events) {
            log::warn!("pad mouse: motion: {e}");
        }
    }

    pub fn button(&mut self, code: u16, down: bool) {
        if self.buttons.insert(code, down) == Some(down) {
            return;
        }
        let t = now();
        let events = [ev(t, EV_KEY, code, down as i32), ev(t, EV_SYN, 0, 0)];
        if let Err(e) = self.dev.write(&events) {
            log::warn!("pad mouse: button: {e}");
        }
    }

    /// Scroll in wheel notches (fractional ok).
    pub fn wheel(&mut self, dx: f32, dy: f32) {
        let hx = (dx * 120.0).round() as i32;
        let hy = (dy * 120.0).round() as i32;
        if hx == 0 && hy == 0 {
            return;
        }
        let t = now();
        let events = [ev(t, EV_REL, 0x0c, hy), ev(t, EV_REL, 0x0b, hx), ev(t, EV_SYN, 0, 0)];
        if let Err(e) = self.dev.write(&events) {
            log::warn!("pad mouse: wheel: {e}");
        }
    }

    pub fn release_all(&mut self) {
        let down: Vec<u16> = self.buttons.iter().filter(|(_, d)| **d).map(|(c, _)| *c).collect();
        for c in down {
            self.button(c, false);
        }
    }
}

/// `monadeck-overlay --gamepad-selftest`: create the pad, show what the
/// system made of it (node, udev joystick tag), echo a scripted input burst
/// back through evdev, and round-trip a rumble effect. No headset needed.
pub fn selftest() -> Result<()> {
    use input_linux::EvdevHandle;
    let mut pad = VirtualPad::open()?;
    std::thread::sleep(Duration::from_millis(300));
    let node = pad.node().ok_or_else(|| anyhow!("could not find the pad's /dev/input/event* node"))?;
    println!("pad: {node}");
    if let Ok(out) = std::process::Command::new("udevadm").args(["info", "--query=property", "--name", &node]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for key in ["ID_INPUT_JOYSTICK", "ID_VENDOR_ID", "ID_MODEL_ID", "NAME", "ID_BUS"] {
            if let Some(l) = text.lines().find(|l| l.starts_with(&format!("{key}="))) {
                println!("  udev {l}");
            }
        }
        if !text.contains("ID_INPUT_JOYSTICK=1") {
            println!("  WARNING: udev did not tag it as a joystick (SDL/Steam may ignore it)");
        }
    }
    let file = OpenOptions::new().read(true).write(true).custom_flags(libc::O_NONBLOCK).open(&node)?;
    let ev = EvdevHandle::new(file);
    if let Ok(n) = ev.device_name() {
        println!("  evdev name: {}", String::from_utf8_lossy(&n).trim_end_matches('\0'));
    }
    println!("  ff effects the kernel will hold: {:?}", ev.effects_count());

    // Scripted burst: every button once, sticks in a circle, triggers ramped.
    let mut got: Vec<(u16, u16, i32)> = Vec::new();
    let drain = |got: &mut Vec<(u16, u16, i32)>| {
        let mut buf = [libc::input_event { time: now(), type_: 0, code: 0, value: 0 }; 64];
        while let Ok(n) = ev.read(&mut buf) {
            if n == 0 {
                break;
            }
            for e in &buf[..n] {
                if e.type_ != EV_SYN {
                    got.push((e.type_, e.code, e.value));
                }
            }
        }
    };
    for b in Btn::ALL {
        pad.button(b, true);
        pad.flush();
        std::thread::sleep(Duration::from_millis(15));
        pad.button(b, false);
        pad.flush();
        std::thread::sleep(Duration::from_millis(15));
        drain(&mut got);
    }
    for i in 0..60 {
        let a = i as f32 / 60.0 * std::f32::consts::TAU;
        pad.axis(Axis::LX, a.cos());
        pad.axis(Axis::LY, a.sin());
        pad.axis(Axis::RX, -a.cos());
        pad.axis(Axis::RY, -a.sin());
        pad.axis(Axis::LT, i as f32 / 59.0);
        pad.axis(Axis::RT, 1.0 - i as f32 / 59.0);
        pad.flush();
        std::thread::sleep(Duration::from_millis(8));
        drain(&mut got);
    }
    pad.release_all();
    std::thread::sleep(Duration::from_millis(30));
    drain(&mut got);
    let keys = got.iter().filter(|(t, _, _)| *t == EV_KEY).count();
    let abs = got.iter().filter(|(t, _, _)| *t == EV_ABS).count();
    println!("echoed back: {keys} key events, {abs} axis events");
    if keys < 22 || abs < 100 {
        println!("  WARNING: fewer events than sent — is the node readable by this user?");
    }

    // Rumble round-trip: upload + play a 300 ms effect from the game's side.
    // The kernel blocks EVIOCSFF until the device side answers the upload, so
    // the pad polls on its own thread (the overlay does that every frame).
    let poller = std::thread::spawn(move || {
        let start = Instant::now();
        let mut seen = Rumble::default();
        let mut seen_late = Rumble::default();
        while start.elapsed() < Duration::from_millis(900) {
            let r = pad.poll();
            if r.is_on() {
                seen = r;
            }
            if start.elapsed() > Duration::from_millis(600) {
                seen_late = r;
            }
            std::thread::sleep(Duration::from_millis(4));
        }
        (pad, seen, seen_late)
    });
    let mut effect: sys::ff_effect = unsafe { std::mem::zeroed() };
    effect.type_ = sys::FF_RUMBLE;
    effect.id = -1;
    effect.replay.length = 300;
    {
        let r = <&mut sys::ff_effect_union>::from(&mut effect).rumble_mut();
        r.strong_magnitude = 0xc000;
        r.weak_magnitude = 0x4000;
    }
    ev.send_force_feedback(&mut effect).map_err(|e| anyhow!("EVIOCSFF: {e}"))?;
    println!("rumble: uploaded effect id {}", effect.id);
    let play = [ev_(EV_FF, effect.id as u16, 1), ev_(EV_SYN, 0, 0)];
    ev.write(&play)?;
    std::thread::sleep(Duration::from_millis(500));
    let stop = [ev_(EV_FF, effect.id as u16, 0), ev_(EV_SYN, 0, 0)];
    ev.write(&stop)?;
    ev.erase_force_feedback(effect.id).map_err(|e| anyhow!("EVIOCRMFF: {e}"))?;
    let (_pad, seen, late) = poller.join().map_err(|_| anyhow!("poller panicked"))?;
    println!("rumble seen while playing: strong {:.2} weak {:.2} (want 0.75 / 0.25)", seen.strong, seen.weak);
    println!("rumble after it ended:     strong {:.2} weak {:.2} (want 0 / 0)", late.strong, late.weak);
    if (seen.strong - 0.75).abs() > 0.05 || (seen.weak - 0.25).abs() > 0.05 {
        bail_rumble(seen)?;
    }
    if late.is_on() {
        return Err(anyhow!("rumble kept going after its 300 ms"));
    }
    println!("gamepad selftest OK");
    Ok(())
}

fn ev_(type_: u16, code: u16, value: i32) -> libc::input_event {
    ev(now(), type_, code, value)
}

fn bail_rumble(r: Rumble) -> Result<()> {
    Err(anyhow!("rumble did not round-trip (got strong {:.2} weak {:.2}, wanted 0.75 / 0.25)", r.strong, r.weak))
}
