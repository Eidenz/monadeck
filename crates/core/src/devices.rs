//! Live device introspection via libmonado.
//!
//! `Monado::auto_connect()` finds the running service through the active
//! runtime's `MND_libmonado_path` and `dlopen`s your fork's `libmonado.so` — so
//! this only returns data while monado is actually running. Mirrors NemuriXR's
//! approach, expanded to the full device strip (role, serial, battery) that
//! drives the SteamVR-style icon row.

use libmonado::{ClientLogic, ClientState, DeviceLogic, DeviceRole, Monado};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A coarse device class the frontend maps to an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Hmd,
    Controller,
    Glove,
    Tracker,
    Gamepad,
    BaseStation,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Battery {
    pub charging: bool,
    /// 0.0–1.0.
    pub charge: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub index: u32,
    pub name_id: u32,
    pub name: String,
    /// Resolved role string ("head", "left", "right", "gamepad"), if any.
    pub role: Option<String>,
    pub kind: DeviceKind,
    pub serial: Option<String>,
    pub battery: Option<Battery>,
}

fn classify(role: Option<&str>, name: &str) -> DeviceKind {
    let n = name.to_lowercase();
    // Gloves (UDCAP) hold the left/right controller roles, so check by name
    // before the role match or they'd render as controllers.
    let is_glove = n.contains("glove") || n.contains("udcap");
    match role {
        Some("head") | Some("eyes") => return DeviceKind::Hmd,
        Some("left") | Some("right") => {
            return if is_glove {
                DeviceKind::Glove
            } else {
                DeviceKind::Controller
            }
        }
        Some("gamepad") => return DeviceKind::Gamepad,
        _ => {}
    }
    if is_glove {
        DeviceKind::Glove
    } else if n.contains("tracker") {
        DeviceKind::Tracker
    } else if n.contains("base") || n.contains("lighthouse") || n.contains("station") {
        DeviceKind::BaseStation
    } else if n.contains("controller") || n.contains("knuckles") || n.contains("index") {
        DeviceKind::Controller
    } else if n.contains("hmd") || n.contains("headset") || n.contains("beyond") {
        DeviceKind::Hmd
    } else {
        DeviceKind::Unknown
    }
}

/// Build index→role-string map by querying each known role. A role that isn't
/// currently assigned simply doesn't appear.
fn role_map(monado: &Monado) -> Vec<(u32, &'static str)> {
    let roles = [
        DeviceRole::Head,
        DeviceRole::Left,
        DeviceRole::Right,
        DeviceRole::Gamepad,
    ];
    let mut out = Vec::new();
    for role in roles {
        if let Ok(idx) = monado.device_index_from_role(role) {
            out.push((idx, <&'static str>::from(role)));
        }
    }
    out
}

/// Path to monado's compositor IPC socket, the cheap "is the service up?" signal.
/// Checking this file avoids invoking libmonado's `auto_connect` while the
/// service is down — which spams stderr with C-side connection-failure messages.
pub fn ipc_socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
    dir.join("monado_comp_ipc")
}

/// A connected client (OpenXR app / overlay) — what shows in the "apps" row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    /// The currently focused app.
    pub focused: bool,
    /// The primary app (the "game") — what shows under "Now Playing".
    pub primary: bool,
    pub overlay: bool,
}

/// Devices + clients in one shot — the deck's live snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub devices: Vec<DeviceInfo>,
    pub clients: Vec<ClientInfo>,
}

fn devices_from(monado: &Monado) -> Result<Vec<DeviceInfo>, String> {
    let roles = role_map(monado);
    let devices = monado
        .devices()
        .map_err(|e| format!("device enumeration failed: {e:?}"))?;

    let mut out = Vec::new();
    for dev in devices {
        let index = dev.index();
        let role = roles
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, r)| (*r).to_string());
        let serial = dev.serial().ok().filter(|s| !s.is_empty());
        let battery = match dev.battery_status() {
            Ok(b) if b.present => Some(Battery {
                charging: b.charging,
                charge: b.charge,
            }),
            _ => None,
        };
        let kind = classify(role.as_deref(), &dev.name);
        out.push(DeviceInfo {
            index,
            name_id: dev.name_id,
            name: dev.name.clone(),
            role,
            kind,
            serial,
            battery,
        });
    }
    Ok(out)
}

fn clients_from(monado: &Monado) -> Result<Vec<ClientInfo>, String> {
    let clients = monado
        .clients()
        .map_err(|e| format!("client enumeration failed: {e:?}"))?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for mut c in clients {
        let Ok(name) = c.name() else { continue };
        // Ignore libmonado control connections — including monadeck's own polls —
        // and our own in-headset overlay (it's part of monadeck, not a "connected
        // app" the user cares to see). Multiple tools share one libmonado, so these
        // would otherwise spam the list; hiding them is a deliberate upside.
        if name.eq_ignore_ascii_case("libmonado")
            || name.eq_ignore_ascii_case("monadeck-overlay")
            || name.trim().is_empty()
        {
            continue;
        }
        // One entry per app name (an app may hold several connections).
        if !seen.insert(name.to_lowercase()) {
            continue;
        }
        let (focused, primary, overlay) = match c.state() {
            Ok(s) => (
                s.contains(ClientState::ClientSessionFocused),
                s.contains(ClientState::ClientPrimaryApp),
                s.contains(ClientState::ClientSessionOverlay),
            ),
            Err(_) => (false, false, false),
        };
        out.push(ClientInfo {
            name,
            focused,
            primary,
            overlay,
        });
    }
    Ok(out)
}

/// Build both lists from an already-open connection. Used by the persistent
/// [`crate::monado_conn::MonadoConn`] worker so the service sees one long-lived
/// client instead of a connect/disconnect per poll.
pub(crate) fn build_snapshot_from(monado: &Monado) -> Result<Snapshot, String> {
    Ok(Snapshot {
        devices: devices_from(monado)?,
        clients: clients_from(monado)?,
    })
}

/// One-shot snapshot (opens and drops a connection). Prefer the persistent
/// `MonadoConn` for polling.
pub fn snapshot() -> Result<Snapshot, String> {
    // Guard on the socket so we never call auto_connect (and trigger its noisy
    // failure logging) when the service isn't actually up.
    if !service_connected() {
        return Err("monado service is not running".into());
    }
    let monado = Monado::auto_connect().map_err(|e| format!("connect failed: {e}"))?;
    build_snapshot_from(&monado)
}

/// Devices only (kept for callers/tests that don't need clients).
pub fn list() -> Result<Vec<DeviceInfo>, String> {
    snapshot().map(|s| s.devices)
}

/// Whether a process is actually listening on `path`, by scanning
/// `/proc/net/unix`. This distinguishes a live service from a *stale* socket
/// file left behind by an unclean exit (freeze/crash/SIGKILL): the leftover file
/// still passes `Path::exists()`, but with no listener the kernel never lists it
/// here.
///
/// Passive on purpose — unlike a `connect()` probe it opens no connection, so it
/// adds zero client connect/disconnect churn to monado's log even when polled
/// every tick (the whole reason [`crate::monado_conn`] holds one persistent
/// connection). If procfs is somehow unreadable we fall back to file presence.
fn socket_is_listening(path: &Path) -> bool {
    // /proc/net/unix columns: Num RefCount Protocol Flags Type St Inode Path.
    // A bound, listening socket has the SO_ACCEPTCON flag (0x10000) set in
    // `Flags`; a stale path has no row at all.
    const SO_ACCEPTCON: u64 = 0x1_0000;
    let Ok(table) = std::fs::read_to_string("/proc/net/unix") else {
        return path.exists();
    };
    let want = path.to_string_lossy();
    table.lines().skip(1).any(|line| {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 8 {
            return false;
        }
        let listening = u64::from_str_radix(cols[3], 16).unwrap_or(0) & SO_ACCEPTCON != 0;
        // Join the tail in case a socket path ever contains a space.
        listening && cols[7..].join(" ") == want
    })
}

/// Whether the monado service is up — i.e. something is actually listening on
/// its IPC socket, not merely that the socket file exists. Cheap and silent (no
/// libmonado call, no connection opened), so it's safe to poll while the service
/// is down. Returning `false` on a leftover socket is deliberate: an unclean
/// exit leaves the file behind, and treating that as "up" would make the UI lie
/// and make the persistent worker keep retrying `auto_connect` against a corpse.
pub fn service_connected() -> bool {
    let path = ipc_socket_path();
    // Stat first: in the common down-state there's no file, so we never read
    // /proc/net/unix at all.
    path.exists() && socket_is_listening(&path)
}

/// Remove monado's IPC socket if it's *stale* — the file exists but nothing is
/// listening, as left by an unclean exit (freeze/crash/SIGKILL). Without this the
/// next `monado-service` start fails to `bind()` with "Address already in use"
/// and refuses to boot.
///
/// A socket a live service is still listening on is never removed (we check
/// first), so this can't disrupt a running runtime. Returns whether a stale
/// socket was removed.
pub fn reclaim_stale_socket() -> bool {
    let path = ipc_socket_path();
    if !path.exists() || socket_is_listening(&path) {
        return false; // absent, or a live service owns it — leave it alone.
    }
    match std::fs::remove_file(&path) {
        Ok(()) => {
            log::info!(
                "removed stale monado IPC socket {} left by a previous unclean exit",
                path.display()
            );
            true
        }
        Err(e) => {
            log::warn!(
                "could not remove stale monado IPC socket {}: {e}",
                path.display()
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn live_listener_reads_up_stale_socket_reads_down() {
        let dir = std::env::temp_dir();
        let pid = std::process::id();
        let live = dir.join(format!("monadeck_live_{pid}.sock"));
        let stale = dir.join(format!("monadeck_stale_{pid}.sock"));
        let _ = std::fs::remove_file(&live);
        let _ = std::fs::remove_file(&stale);

        // A real listener is accepting → shows up in /proc/net/unix.
        let listener = UnixListener::bind(&live).expect("bind live socket");
        // Bind then drop: the path lingers as a socket file with no listener —
        // exactly the leftover an unclean monado exit leaves behind. (Rust does
        // not unlink a UnixListener's path on drop.)
        let dead = UnixListener::bind(&stale).expect("bind stale socket");
        drop(dead);

        assert!(stale.exists(), "dropped listener should leave the socket file");
        assert!(socket_is_listening(&live), "live listener must read as up");
        assert!(
            !socket_is_listening(&stale),
            "stale socket must read as down"
        );

        drop(listener);
        let _ = std::fs::remove_file(&live);
        let _ = std::fs::remove_file(&stale);
    }
}
