//! The WiVRn backend — everything Monadeck needs to run `wivrn-server` in place
//! of `monado-service`.
//!
//! WiVRn is Monado underneath (it ships its own `libmonado`, OpenXR manifest and
//! the `XR_EXTX_overlay` extension), so the device strip, app list and the
//! in-headset overlay all work unchanged. What differs is the control surface:
//!
//! - **Start** = spawn `wivrn-server` (no D-Bus activation exists). We pass
//!   `--no-manage-active-runtime` so *we* wire the runtime files with our own
//!   backup/restore, exactly like the Monado backend — WiVRn would otherwise
//!   symlink `active_runtime.json` and rewrite `openvrpaths.vrpath` itself.
//! - **Stop / status / pairing** = the session-bus interface
//!   `io.github.wivrn.Server` (the same one `wivrnctl` and the WiVRn dashboard
//!   use). `Quit` is a clean shutdown that restores nothing of ours.
//! - **Sessions are headset-scoped.** The server listens for a headset from
//!   start, forks a Monado IPC child *when one connects*, and tears it down on
//!   disconnect. `libmonado` must only be used while `SessionRunning` is true —
//!   connecting earlier blocks forever (the socket exists, nobody accepts).
//!   That's why [`service_connected`] gates on the D-Bus property, and why
//!   plugins/overlay are launched per session (see the desktop's session watch).
//!
//! Verified against WiVRn 26.6.2 (Fedora RPM); the interface XML lives in the
//! WiVRn repo under `dbus/io.github.wivrn.Server.xml`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use zbus::blocking::fdo::{DBusProxy, PropertiesProxy};
use zbus::blocking::Connection;
use zbus::names::{BusName, InterfaceName};
use zbus::zvariant::{OwnedValue, Value};

pub const DBUS_NAME: &str = "io.github.wivrn.Server";
pub const DBUS_PATH: &str = "/io/github/wivrn/Server";
pub const DBUS_IFACE: &str = "io.github.wivrn.Server";

// --- Detection -------------------------------------------------------------

/// Find `wivrn-server`: `$PATH` first, then the usual install prefixes.
pub fn detect_server() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join("wivrn-server");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    ["/usr/bin", "/usr/local/bin", "/opt/wivrn/bin"]
        .iter()
        .map(|d| Path::new(d).join("wivrn-server"))
        .find(|p| p.is_file())
}

/// WiVRn's OpenXR runtime manifest (`openxr_wivrn.json`), derived from where the
/// server binary lives (`<prefix>/bin/wivrn-server` → `<prefix>/share/openxr/1/`).
/// Multi-arch installs suffix the file (`openxr_wivrn.x86_64.json`); we prefer
/// the plain name, then any `openxr_wivrn*.json`.
pub fn manifest_for(server_bin: &Path) -> Option<PathBuf> {
    let prefix = server_bin.parent()?.parent()?;
    let dir = prefix.join("share").join("openxr").join("1");
    let plain = dir.join("openxr_wivrn.json");
    if plain.is_file() {
        return Some(plain);
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("openxr_wivrn") && n.ends_with(".json"))
                    .unwrap_or(false)
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

/// `$XDG_RUNTIME_DIR/wivrn/comp_ipc` — WiVRn's Monado IPC socket (it is *not*
/// `monado_comp_ipc`, so both runtimes can coexist on disk).
pub fn ipc_socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
    dir.join("wivrn").join("comp_ipc")
}

/// Command-line args we start the server with. `--no-manage-active-runtime`
/// disables *both* of WiVRn's runtime-file rewrites (OpenXR manifest symlink and
/// `openvrpaths.vrpath`) — Monadeck owns those with its own backup scheme.
pub fn server_args() -> Vec<String> {
    vec!["--no-manage-active-runtime".to_string()]
}

// --- D-Bus -------------------------------------------------------------------

static CONN: Mutex<Option<Connection>> = Mutex::new(None);

/// A (cached) session-bus connection. Cheap to clone; re-established if the
/// first attempt failed (e.g. no session bus yet).
fn conn() -> zbus::Result<Connection> {
    let mut guard = CONN.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(c) = guard.as_ref() {
        return Ok(c.clone());
    }
    let c = Connection::session()?;
    *guard = Some(c.clone());
    Ok(c)
}

fn properties() -> zbus::Result<PropertiesProxy<'static>> {
    PropertiesProxy::builder(&conn()?)
        .destination(DBUS_NAME)?
        .path(DBUS_PATH)?
        .build()
}

fn iface() -> InterfaceName<'static> {
    InterfaceName::from_static_str_unchecked(DBUS_IFACE)
}

/// Call a method on the server interface with no reply payload.
fn call<B: serde::Serialize + zbus::zvariant::DynamicType>(
    method: &str,
    body: &B,
) -> zbus::Result<()> {
    conn()?.call_method(Some(DBUS_NAME), DBUS_PATH, Some(DBUS_IFACE), method, body)?;
    Ok(())
}

/// Whether a WiVRn server (ours or anyone's, e.g. the dashboard's) currently owns
/// the well-known bus name. The one reliable "is WiVRn up?" signal, and the guard
/// against spawning a second instance — which would unlink the first one's IPC
/// socket path and then die.
pub fn dbus_name_owned() -> bool {
    let Ok(c) = conn() else { return false };
    let Ok(dbus) = DBusProxy::new(&c) else {
        return false;
    };
    let Ok(name) = BusName::try_from(DBUS_NAME) else {
        return false;
    };
    dbus.name_has_owner(name).unwrap_or(false)
}

/// The `SessionRunning` property: a headset is connected and the Monado IPC
/// child is serving. `false` when the server is down or idle.
pub fn session_running() -> bool {
    properties()
        .ok()
        .and_then(|p| p.get(iface(), "SessionRunning").ok())
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}

/// WiVRn's equivalent of "the service is serving IPC": the server is up *and*
/// a headset session is running. Only then may libmonado connect (see module
/// docs) — so this is what [`crate::devices::service_connected`] returns for
/// this backend, and what gates the persistent libmonado worker.
pub fn service_connected() -> bool {
    session_running() && crate::devices::socket_is_listening(&ipc_socket_path())
}

/// A paired headset, as listed by the server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KnownHeadset {
    pub name: String,
    pub public_key: String,
    /// Seconds since the epoch of the last connection, `0` when never.
    pub last_connection: i64,
}

/// One snapshot of the server's exported state (a single `GetAll` round trip).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct WivrnStatus {
    pub headset_connected: bool,
    pub session_running: bool,
    pub pairing_enabled: bool,
    pub encryption_enabled: bool,
    /// The pairing PIN while pairing is enabled, else empty.
    pub pin: String,
    /// Headset model reported by the client, once connected.
    pub system_name: String,
    /// Launch-options prefix WiVRn wants Steam games to use.
    pub steam_command: String,
    pub known_keys: Vec<KnownHeadset>,
    pub preferred_refresh_rate: f64,
    pub available_refresh_rates: Vec<f64>,
    /// Stream bitrate in bits/s (headset-side setting), `0` until connected.
    pub bitrate: u32,
    pub supported_codecs: Vec<String>,
}

fn get<T: TryFrom<OwnedValue>>(map: &HashMap<String, OwnedValue>, key: &str) -> Option<T> {
    map.get(key).cloned().and_then(|v| T::try_from(v).ok())
}

/// Read the whole status in one call. `None` when the server isn't reachable.
pub fn status() -> Option<WivrnStatus> {
    let map = properties().ok()?.get_all(iface()).ok()?;
    let known_keys = get::<Vec<(String, String, i64)>>(&map, "KnownKeys")
        .unwrap_or_default()
        .into_iter()
        .map(|(name, public_key, last_connection)| KnownHeadset {
            name,
            public_key,
            last_connection,
        })
        .collect();
    Some(WivrnStatus {
        headset_connected: get(&map, "HeadsetConnected").unwrap_or(false),
        session_running: get(&map, "SessionRunning").unwrap_or(false),
        pairing_enabled: get(&map, "PairingEnabled").unwrap_or(false),
        encryption_enabled: get(&map, "EncryptionEnabled").unwrap_or(false),
        pin: get(&map, "Pin").unwrap_or_default(),
        system_name: get(&map, "SystemName").unwrap_or_default(),
        steam_command: get::<String>(&map, "SteamCommand")
            .unwrap_or_default()
            .trim()
            .to_string(),
        known_keys,
        preferred_refresh_rate: get(&map, "PreferredRefreshRate").unwrap_or(0.0),
        available_refresh_rates: get(&map, "AvailableRefreshRates").unwrap_or_default(),
        bitrate: get(&map, "Bitrate").unwrap_or(0),
        supported_codecs: get(&map, "SupportedCodecs").unwrap_or_default(),
    })
}

/// Ask the server to shut down cleanly (`Quit`). Returns once the request is
/// acknowledged; the process exits right after when idle, or after it has torn
/// down the session and any app it launched.
pub fn quit() -> zbus::Result<()> {
    call("Quit", &())
}

/// Drop the current headset connection (the server keeps running and listening).
pub fn disconnect() -> zbus::Result<()> {
    call("Disconnect", &())
}

/// Allow a new headset to pair for `timeout_secs` (`-1` = until disabled).
/// Returns the PIN to type on the headset; empty when refused because a session
/// is active.
pub fn enable_pairing(timeout_secs: i32) -> zbus::Result<String> {
    let reply = conn()?.call_method(
        Some(DBUS_NAME),
        DBUS_PATH,
        Some(DBUS_IFACE),
        "EnablePairing",
        &(timeout_secs,),
    )?;
    reply.body().deserialize::<String>()
}

pub fn disable_pairing() -> zbus::Result<()> {
    call("DisablePairing", &())
}

/// Forget a paired headset (by its public key from [`WivrnStatus::known_keys`]).
pub fn revoke_key(public_key: &str) -> zbus::Result<()> {
    call("RevokeKey", &(public_key,))
}

pub fn rename_key(public_key: &str, name: &str) -> zbus::Result<()> {
    call("RenameKey", &(public_key, name))
}

/// The server's merged configuration as JSON text (what it read from
/// `~/.config/wivrn/config.json` and friends).
pub fn json_configuration() -> zbus::Result<String> {
    let v = properties()?.get(iface(), "JsonConfiguration")?;
    String::try_from(v).map_err(Into::into)
}

/// Replace the server's configuration. The server validates nothing here — it
/// writes the text to its config file verbatim — so callers should parse the
/// JSON first. Encoder changes apply on the next headset connection.
pub fn set_json_configuration(json: &str) -> zbus::Result<()> {
    Ok(properties()?.set(iface(), "JsonConfiguration", Value::from(json))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_lookup_prefers_plain_name() {
        let root = std::env::temp_dir().join(format!("monadeck_wivrn_{}", std::process::id()));
        let bin = root.join("bin").join("wivrn-server");
        let dir = root.join("share").join("openxr").join("1");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&bin, "").unwrap();
        std::fs::write(dir.join("openxr_wivrn.x86_64.json"), "{}").unwrap();
        assert_eq!(
            manifest_for(&bin).unwrap(),
            dir.join("openxr_wivrn.x86_64.json")
        );
        std::fs::write(dir.join("openxr_wivrn.json"), "{}").unwrap();
        assert_eq!(manifest_for(&bin).unwrap(), dir.join("openxr_wivrn.json"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// End-to-end against a real `wivrn-server` (skipped unless installed; run
    /// with `cargo test -p monadeck-core -- --ignored`). Starts the server the
    /// way the desktop does, waits for the bus name, reads a status snapshot,
    /// and shuts it down over D-Bus. Never touches runtime files.
    #[test]
    #[ignore]
    fn live_dbus_roundtrip() {
        let Some(bin) = detect_server() else {
            eprintln!("wivrn-server not installed; skipping");
            return;
        };
        assert!(!dbus_name_owned(), "a WiVRn server is already running");
        let mut child = std::process::Command::new(&bin)
            .args(server_args())
            .arg("--no-publish-service")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn wivrn-server");
        let mut up = false;
        for _ in 0..50 {
            if dbus_name_owned() {
                up = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(up, "bus name never appeared");
        let st = status().expect("status");
        assert!(!st.session_running);
        assert!(!st.headset_connected);
        assert!(!service_connected());
        assert!(json_configuration().is_ok());
        quit().expect("quit");
        let mut exited = false;
        for _ in 0..50 {
            if child.try_wait().expect("wait").is_some() {
                exited = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if !exited {
            let _ = child.kill();
        }
        assert!(exited, "server did not exit after Quit");
        assert!(!dbus_name_owned());
    }
}
