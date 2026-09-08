//! OSC control: a small UDP listener so games and tools can drive the overlay —
//! VRChat avatar parameters (`/avatar/parameters/MonadeckWatch` …), VRCOSC and
//! friends, or anything that can send an OSC packet to 127.0.0.1:<port>.
//!
//! Addresses (an optional bool / int / float argument sets the state; none
//! toggles):
//!
//! - `/monadeck/watch`         show / hide the wrist watch
//! - `/monadeck/watch/mini`    minimal (clock-only) watch
//! - `/monadeck/dashboard`     the dashboard
//! - `/monadeck/screens`       the desktop screens
//! - `/monadeck/keyboard`      the keyboard
//! - `/monadeck/notify "title" ["body"]`  a toast
//!
//! VRChat sends changed avatar parameters as `/avatar/parameters/<name>`; a
//! parameter named `MonadeckWatch`, `Monadeck/Watch`, `monadeck_watch_mini`,
//! `MonadeckDashboard`, `MonadeckScreens` or `MonadeckKeyboard` maps to the
//! matching address above.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Watch(Option<bool>),
    WatchMini(Option<bool>),
    Dashboard(Option<bool>),
    Screens(Option<bool>),
    Keyboard(Option<bool>),
    Notify { title: String, body: String },
}

pub struct Osc {
    rx: mpsc::Receiver<Cmd>,
    /// Bound and listening (else the port was busy).
    pub ok: bool,
    pub port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Osc {
    pub fn start(port: u16) -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = listen(port, tx, stop.clone());
        Self { rx, ok: thread.is_some(), port, stop, thread }
    }

    pub fn drain(&self) -> Vec<Cmd> {
        self.rx.try_iter().collect()
    }
}

impl Drop for Osc {
    /// Stops the listener and waits for it, so the port is free again for a
    /// restart (the thread wakes every 100 ms to check).
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn listen(port: u16, tx: mpsc::Sender<Cmd>, stop: Arc<AtomicBool>) -> Option<std::thread::JoinHandle<()>> {
    let socket = match std::net::UdpSocket::bind(("127.0.0.1", port)) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("osc: can't bind udp/{port} ({e}); OSC control off");
            return None;
        }
    };
    let _ = socket.set_read_timeout(Some(Duration::from_millis(100)));
    let handle = std::thread::Builder::new()
        .name("osc".into())
        .spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            while !stop.load(Ordering::Relaxed) {
                let n = match socket.recv(&mut buf) {
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(e) => {
                        log::warn!("osc: socket error: {e}");
                        break;
                    }
                };
                for (addr, args) in parse_packet(&buf[..n]) {
                    if let Some(cmd) = command(&addr, &args) {
                        log::info!("osc: {addr} {args:?} → {cmd:?}");
                        let _ = tx.send(cmd);
                    }
                }
            }
            log::info!("osc: listener on udp/{port} stopped");
        })
        .expect("spawn osc listener");
    log::info!("osc: listening on udp/{port}");
    Some(handle)
}

// --- mapping -------------------------------------------------------------------

/// Which overlay action an address names, if any.
fn command(addr: &str, args: &[Arg]) -> Option<Cmd> {
    let flag = || args.first().and_then(Arg::as_bool);
    // `/monadeck/watch/mini`, `/avatar/parameters/MonadeckWatchMini`,
    // `/avatar/parameters/Monadeck/watch_mini` … all normalise to "watchmini".
    let key: String = addr
        .strip_prefix("/monadeck")
        .or_else(|| addr.strip_prefix("/avatar/parameters/").and_then(|p| strip_ci(p, "monadeck")))?
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    match key.as_str() {
        "watch" => Some(Cmd::Watch(flag())),
        "watchmini" | "miniwatch" | "minimalwatch" => Some(Cmd::WatchMini(flag())),
        "dashboard" => Some(Cmd::Dashboard(flag())),
        "screens" | "desktop" => Some(Cmd::Screens(flag())),
        "keyboard" => Some(Cmd::Keyboard(flag())),
        "notify" | "toast" => {
            let mut strs = args.iter().filter_map(|a| if let Arg::Str(s) = a { Some(s.trim()) } else { None }).filter(|s| !s.is_empty());
            let title = strs.next()?.to_string();
            let body = strs.next().unwrap_or("").to_string();
            Some(Cmd::Notify { title, body })
        }
        _ => None,
    }
}

fn strip_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let n = prefix.len();
    (s.len() >= n && s.is_char_boundary(n) && s[..n].eq_ignore_ascii_case(prefix)).then(|| &s[n..])
}

// --- wire format -------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Nil,
}

impl Arg {
    fn as_bool(&self) -> Option<bool> {
        match self {
            Arg::Bool(b) => Some(*b),
            Arg::Int(i) => Some(*i != 0),
            Arg::Float(f) => Some(*f > 0.5),
            Arg::Str(s) => match s.to_ascii_lowercase().as_str() {
                "1" | "true" | "on" | "show" => Some(true),
                "0" | "false" | "off" | "hide" => Some(false),
                _ => None,
            },
            Arg::Nil => None,
        }
    }
}

/// Messages in a packet (a bundle is flattened; nested bundles too).
fn parse_packet(buf: &[u8]) -> Vec<(String, Vec<Arg>)> {
    let mut out = Vec::new();
    if buf.starts_with(b"#bundle\0") {
        let mut i = 16; // "#bundle\0" + 8-byte time tag
        while i + 4 <= buf.len() {
            let n = u32::from_be_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]) as usize;
            i += 4;
            if n == 0 || i + n > buf.len() {
                break;
            }
            out.extend(parse_packet(&buf[i..i + n]));
            i += n;
        }
    } else if let Some(m) = parse_message(buf) {
        out.push(m);
    }
    out
}

fn parse_message(buf: &[u8]) -> Option<(String, Vec<Arg>)> {
    let (addr, mut i) = read_str(buf, 0)?;
    if !addr.starts_with('/') {
        return None;
    }
    let mut args = Vec::new();
    // Type tags are optional in old OSC; without them it's a bare address.
    if i < buf.len() && buf[i] == b',' {
        let (tags, next) = read_str(buf, i)?;
        i = next;
        for t in tags[1..].chars() {
            match t {
                'i' => {
                    let b = buf.get(i..i + 4)?;
                    args.push(Arg::Int(i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as i64));
                    i += 4;
                }
                'h' => {
                    let b: [u8; 8] = buf.get(i..i + 8)?.try_into().ok()?;
                    args.push(Arg::Int(i64::from_be_bytes(b)));
                    i += 8;
                }
                'f' => {
                    let b = buf.get(i..i + 4)?;
                    args.push(Arg::Float(f32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f64));
                    i += 4;
                }
                'd' => {
                    let b: [u8; 8] = buf.get(i..i + 8)?.try_into().ok()?;
                    args.push(Arg::Float(f64::from_be_bytes(b)));
                    i += 8;
                }
                's' | 'S' => {
                    let (s, next) = read_str(buf, i)?;
                    args.push(Arg::Str(s));
                    i = next;
                }
                'b' => {
                    let b = buf.get(i..i + 4)?;
                    let n = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
                    i += 4 + n.div_ceil(4) * 4;
                }
                'T' => args.push(Arg::Bool(true)),
                'F' => args.push(Arg::Bool(false)),
                'N' => args.push(Arg::Nil),
                'I' => args.push(Arg::Int(1)),
                // 't' (time tag), 'c', 'r', 'm' … : fixed-size, skipped.
                't' => i += 8,
                'c' | 'r' | 'm' => i += 4,
                _ => return None,
            }
        }
    }
    Some((addr, args))
}

/// A NUL-terminated string padded to 4 bytes: (string, next offset).
fn read_str(buf: &[u8], at: usize) -> Option<(String, usize)> {
    let rest = buf.get(at..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    let s = std::str::from_utf8(&rest[..end]).ok()?.to_string();
    let next = at + (end + 1).div_ceil(4) * 4;
    Some((s, next.min(buf.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(addr: &str, tags: &str, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        let pad = |v: &mut Vec<u8>, s: &str| {
            v.extend_from_slice(s.as_bytes());
            v.push(0);
            while v.len() % 4 != 0 {
                v.push(0);
            }
        };
        pad(&mut v, addr);
        if !tags.is_empty() {
            pad(&mut v, tags);
        }
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn parses_typed_arguments() {
        let m = msg("/monadeck/watch", ",T", &[]);
        assert_eq!(parse_packet(&m), vec![("/monadeck/watch".to_string(), vec![Arg::Bool(true)])]);
        let m = msg("/avatar/parameters/MonadeckWatchMini", ",f", &0.75f32.to_be_bytes());
        assert_eq!(parse_packet(&m)[0].1, vec![Arg::Float(0.75)]);
        let mut body = Vec::new();
        body.extend_from_slice(b"hi\0\0");
        body.extend_from_slice(b"there\0\0\0");
        let m = msg("/monadeck/notify", ",ss", &body);
        assert_eq!(parse_packet(&m)[0].1, vec![Arg::Str("hi".into()), Arg::Str("there".into())]);
        // Bare address, no type tags.
        assert_eq!(parse_packet(&msg("/monadeck/dashboard", "", &[]))[0].1, vec![]);
    }

    #[test]
    fn parses_bundles() {
        let a = msg("/monadeck/watch", ",i", &1i32.to_be_bytes());
        let b = msg("/monadeck/screens", "", &[]);
        let mut v = b"#bundle\0".to_vec();
        v.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
        for m in [&a, &b] {
            v.extend_from_slice(&(m.len() as u32).to_be_bytes());
            v.extend_from_slice(m);
        }
        let got = parse_packet(&v);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "/monadeck/watch");
        assert_eq!(got[1].0, "/monadeck/screens");
    }

    #[test]
    fn maps_addresses_and_avatar_parameters() {
        assert_eq!(command("/monadeck/watch", &[]), Some(Cmd::Watch(None)));
        assert_eq!(command("/monadeck/watch", &[Arg::Int(0)]), Some(Cmd::Watch(Some(false))));
        assert_eq!(command("/monadeck/watch/mini", &[Arg::Bool(true)]), Some(Cmd::WatchMini(Some(true))));
        assert_eq!(command("/avatar/parameters/MonadeckWatch", &[Arg::Bool(true)]), Some(Cmd::Watch(Some(true))));
        assert_eq!(command("/avatar/parameters/Monadeck/Watch_Mini", &[Arg::Float(1.0)]), Some(Cmd::WatchMini(Some(true))));
        assert_eq!(command("/avatar/parameters/monadeck_dashboard", &[Arg::Float(0.0)]), Some(Cmd::Dashboard(Some(false))));
        assert_eq!(command("/avatar/parameters/SomethingElse", &[Arg::Bool(true)]), None);
        assert_eq!(command("/monadeck/nope", &[]), None);
        assert_eq!(
            command("/monadeck/notify", &[Arg::Str("Raid".into()), Arg::Str("starting now".into())]),
            Some(Cmd::Notify { title: "Raid".into(), body: "starting now".into() })
        );
        assert_eq!(command("/monadeck/notify", &[]), None);
    }

    #[test]
    fn listener_receives_and_frees_its_port_on_drop() {
        let port = 39371;
        let o = Osc::start(port);
        assert!(o.ok);
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        s.send_to(&msg("/monadeck/watch", ",F", &[]), ("127.0.0.1", port)).unwrap();
        let mut got = Vec::new();
        for _ in 0..50 {
            got = o.drain();
            if !got.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(got, vec![Cmd::Watch(Some(false))]);
        drop(o);
        assert!(Osc::start(port).ok, "port should be free again after drop");
    }

    #[test]
    fn truncated_packets_are_dropped() {
        let m = msg("/monadeck/watch", ",i", &[0, 0]);
        assert!(parse_packet(&m).is_empty());
        assert!(parse_packet(b"no-nul-terminator").is_empty());
    }
}
