//! Pairing controllers and trackers to their USB receivers, without SteamVR.
//!
//! A Lighthouse receiver ("Watchman dongle", Valve VID 28de, PID 2101/2102)
//! enters pairing mode on one HID feature report: command report `0xFF`,
//! command `0xAD` (pair), 3 bytes of payload `01 10 27` (start, then 10000
//! little-endian: a 10 s window). The bytes are what SteamVR sends, as recorded
//! by libsurvive (MIT, `src/driver_vive.c`, `vive_request_pairing`). Whatever
//! is in pairing mode (an Index controller held at B + System, a tracker held
//! at its power button) then pairs with it, replacing what it was paired with.
//!
//! It goes through the receiver's hidraw node, which monado-service may hold
//! open at the same time: a feature report goes straight to the device, and
//! every hidraw reader gets its own copy of the input reports. So pairing works
//! with VR running, and the new device shows up like one that was switched on.

use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

const VALVE_VID: u32 = 0x28de;
/// Receivers: the original Watchman dongle and the newer Valve one (also built
/// into the Index headset).
const RECEIVER_PIDS: [u32; 2] = [0x2101, 0x2102];

/// `ff ad 03 01 10 27`: command report, pair, 3 bytes, start, 10000 ms.
const PAIR_REQUEST: [u8; 6] = [0xff, 0xad, 0x03, 0x01, 0x10, 0x27];
/// How long the receiver listens for a device after [`start_pairing`].
pub const PAIRING_WINDOW: Duration = Duration::from_secs(10);

/// How long [`activity`] listens for input reports.
const LISTEN: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Serialize)]
pub struct Receiver {
    /// The receiver's USB serial (what lighthouse_console calls it).
    pub serial: String,
    /// Product name, without the vendor ("Watchman Dongle", "Valve VR Radio"…).
    pub name: String,
    /// `/dev/hidrawN`.
    pub node: String,
    /// Streaming data right now, i.e. a device is connected through it. Only
    /// known while the lighthouse driver runs (it starts the stream).
    pub active: Option<bool>,
}

/// Parse a hidraw `uevent`: (vendor, product, name, serial).
fn parse_uevent(text: &str) -> Option<(u32, u32, String, String)> {
    let mut id = None;
    let mut name = String::new();
    let mut serial = String::new();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("HID_ID=") {
            // bus:vendor:product, all hex.
            let mut parts = v.split(':').skip(1);
            let vid = u32::from_str_radix(parts.next()?, 16).ok()?;
            let pid = u32::from_str_radix(parts.next()?, 16).ok()?;
            id = Some((vid, pid));
        } else if let Some(v) = line.strip_prefix("HID_NAME=") {
            name = v.to_string();
        } else if let Some(v) = line.strip_prefix("HID_UNIQ=") {
            serial = v.to_string();
        }
    }
    let (vid, pid) = id?;
    Some((vid, pid, name, serial))
}

/// "Valve Software Watchman Dongle" → "Watchman Dongle".
fn short_name(name: &str) -> String {
    let n = name
        .strip_prefix("Valve Software ")
        .or_else(|| name.strip_prefix("Valve Corporation "))
        .unwrap_or(name);
    if n.is_empty() { "Receiver".into() } else { n.to_string() }
}

/// Every Lighthouse receiver plugged in, sorted by serial. `active` is left
/// unknown; see [`activity`].
pub fn receivers() -> Vec<Receiver> {
    let Ok(dir) = fs::read_dir("/sys/class/hidraw") else { return Vec::new() };
    let mut out: Vec<Receiver> = dir
        .flatten()
        .filter_map(|e| {
            let node = e.file_name().to_string_lossy().into_owned();
            let text = fs::read_to_string(e.path().join("device/uevent")).ok()?;
            let (vid, pid, name, serial) = parse_uevent(&text)?;
            (vid == VALVE_VID && RECEIVER_PIDS.contains(&pid)).then(|| Receiver {
                serial: if serial.is_empty() { node.clone() } else { serial },
                name: short_name(&name),
                node: format!("/dev/{node}"),
                active: None,
            })
        })
        .collect();
    out.sort_by(|a, b| a.serial.cmp(&b.serial));
    out
}

/// Which receivers stream input reports right now (a device is connected
/// through them), keyed by node. Listens to all of them at once for a moment,
/// read-only: other readers (monado-service) still get every report.
pub fn activity(receivers: &[Receiver]) -> HashMap<String, bool> {
    let mut files: Vec<(String, fs::File)> = receivers
        .iter()
        .filter_map(|r| {
            OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&r.node)
                .ok()
                .map(|f| (r.node.clone(), f))
        })
        .collect();
    let mut seen: HashMap<String, bool> = files.iter().map(|(n, _)| (n.clone(), false)).collect();
    let end = Instant::now() + LISTEN;
    let mut buf = [0u8; 64];
    while Instant::now() < end && seen.values().any(|v| !v) {
        let mut fds: Vec<libc::pollfd> = files
            .iter()
            .map(|(_, f)| libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLIN, revents: 0 })
            .collect();
        let left = end.saturating_duration_since(Instant::now()).as_millis() as i32;
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, left.max(1)) };
        if n <= 0 {
            break;
        }
        for (pfd, (node, file)) in fds.iter().zip(files.iter_mut()) {
            if pfd.revents & libc::POLLIN != 0 && file.read(&mut buf).is_ok_and(|len| len > 0) {
                seen.insert(node.clone(), true);
            }
        }
    }
    seen
}

/// `HIDIOCSFEATURE(len)`: `_IOC(_IOC_WRITE | _IOC_READ, 'H', 0x06, len)`.
fn hidiocsfeature(len: usize) -> libc::c_ulong {
    (3 << 30) | ((len as libc::c_ulong) << 16) | ((b'H' as libc::c_ulong) << 8) | 0x06
}

/// Put the receiver with this serial into pairing mode for [`PAIRING_WINDOW`].
pub fn start_pairing(serial: &str) -> Result<(), String> {
    let receiver = receivers()
        .into_iter()
        .find(|r| r.serial == serial)
        .ok_or_else(|| format!("Receiver {serial} isn't plugged in."))?;
    send_pair_request(Path::new(&receiver.node))
}

fn send_pair_request(node: &Path) -> Result<(), String> {
    let file = OpenOptions::new().read(true).write(true).open(node).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => format!(
            "No permission to use {} — install the VR udev rules (Settings → Environment).",
            node.display()
        ),
        _ => format!("Couldn't open {}: {e}", node.display()),
    })?;
    let mut report = PAIR_REQUEST;
    let rc = unsafe { libc::ioctl(file.as_raw_fd(), hidiocsfeature(report.len()) as _, report.as_mut_ptr()) };
    if rc < 0 {
        return Err(format!("The receiver refused the pairing request: {}", std::io::Error::last_os_error()));
    }
    log::info!("pairing: {} is listening for a device", node.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_receiver_uevent() {
        let text = "DRIVER=hid-generic\nHID_ID=0003:000028DE:00002101\nHID_NAME=Valve Software Watchman Dongle\nHID_PHYS=usb-0000:0d:00.0-1/input0\nHID_UNIQ=DE4B39BF50\nMODALIAS=hid:b0003g0001v000028DEp00002101\n";
        let (vid, pid, name, serial) = parse_uevent(text).unwrap();
        assert_eq!((vid, pid), (0x28de, 0x2101));
        assert_eq!(short_name(&name), "Watchman Dongle");
        assert_eq!(serial, "DE4B39BF50");
    }

    #[test]
    fn short_names() {
        assert_eq!(short_name("Valve Corporation Valve VR Radio"), "Valve VR Radio");
        assert_eq!(short_name("Something Else"), "Something Else");
        assert_eq!(short_name(""), "Receiver");
    }

    #[test]
    fn feature_ioctl_number() {
        // HIDIOCSFEATURE(6) from linux/hidraw.h.
        assert_eq!(hidiocsfeature(6), 0xC006_4806);
    }
}
