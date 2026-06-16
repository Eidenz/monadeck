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
use std::path::PathBuf;

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
        // Ignore libmonado control connections — including monadeck's own polls.
        // Multiple tools share one libmonado, so these would spam the list; not
        // showing them is a deliberate upside over Envision.
        if name.eq_ignore_ascii_case("libmonado") || name.trim().is_empty() {
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

/// Whether the monado service is up, by presence of its IPC socket. Cheap and
/// silent (no libmonado call), so it's safe to poll while the service is down.
pub fn service_connected() -> bool {
    ipc_socket_path().exists()
}
