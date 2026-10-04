//! Lighthouse base stations over Bluetooth LE: find them, switch them on, to
//! sleep or to standby, set a 2.0 station's channel, make one blink. Talks to
//! BlueZ on the system bus, so it needs a Bluetooth adapter on the PC (the
//! headset's own radio isn't reachable without SteamVR).
//!
//! The protocol is public (ShayBox/Lighthouse, MIT; lighthouse_pm):
//! - 2.0 stations advertise as `LHB-XXXXXXXX`. Characteristics under service
//!   `00001523-1212-efde-1523-785feabcd124`: power `…1525…` (write 0x00 sleep,
//!   0x01 on, 0x02 standby; reads 0x00 sleep, 0x02 standby, 0x0b on, anything
//!   else while waking up), channel `…1524…` (1–16), identify `…8421…` (any
//!   byte blinks the front LED).
//! - 1.0 stations advertise as `HTC BS XXXXXX` and take a 20-byte command on
//!   `0000cb01-0000-1000-8000-00805f9b34fb` carrying the station's ID (printed
//!   on its back): on `12 00 00 00 <id LE>…`, sleep `12 02 00 01 <id LE>…`.
//!   They have no standby, channel or identify over Bluetooth.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::thread::sleep;
use std::time::{Duration, Instant};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const BLUEZ: &str = "org.bluez";
const ADAPTER_IFACE: &str = "org.bluez.Adapter1";
const DEVICE_IFACE: &str = "org.bluez.Device1";
const CHAR_IFACE: &str = "org.bluez.GattCharacteristic1";

const V2_POWER: &str = "00001525-1212-efde-1523-785feabcd124";
const V2_CHANNEL: &str = "00001524-1212-efde-1523-785feabcd124";
const V2_IDENTIFY: &str = "00008421-1212-efde-1523-785feabcd124";
const V1_COMMAND: &str = "0000cb01-0000-1000-8000-00805f9b34fb";

const CONNECT_TRIES: usize = 3;
const SERVICES_TIMEOUT: Duration = Duration::from_secs(10);

type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Version {
    V1,
    V2,
}

impl Version {
    fn from_name(name: &str) -> Option<Self> {
        if name.starts_with("LHB-") {
            Some(Self::V2)
        } else if name.starts_with("HTC BS") {
            Some(Self::V1)
        } else {
            None
        }
    }
}

/// What to put a station into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Power {
    On,
    /// Everything off but Bluetooth (what SteamVR does).
    Sleep,
    /// Lasers off, rotor kept spinning: wakes faster, but you can hear it.
    Standby,
}

/// What a station reports (2.0 only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerState {
    On,
    Sleep,
    Standby,
    Waking,
}

/// A base station seen over Bluetooth.
#[derive(Debug, Clone, Serialize)]
pub struct Found {
    pub address: String,
    pub name: String,
    pub version: Version,
    /// Signal strength from the scan, when BlueZ has one.
    pub rssi: Option<i16>,
}

/// A base station kept in the config, to switch with VR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saved {
    pub address: String,
    pub name: String,
    pub version: Version,
    /// 1.0 only: the ID printed on its back, needed to switch it.
    #[serde(default)]
    pub bsid: Option<String>,
}

/// A station's current state, read over a connection.
#[derive(Debug, Clone, Serialize)]
pub struct StationState {
    pub power: Option<PowerState>,
    pub channel: Option<u8>,
}

// --- BlueZ plumbing ------------------------------------------------------------

fn bus() -> Result<Connection, String> {
    Connection::system().map_err(|e| format!("Couldn't reach the system bus: {e}"))
}

fn not_available(e: zbus::Error) -> String {
    let text = e.to_string();
    // BlueZ isn't running: its service won't start without an adapter.
    if text.contains("NameHasNoOwner") || text.contains("ServiceUnknown") {
        "Bluetooth isn't available: no adapter found, or the bluetooth service isn't running.".into()
    } else {
        format!("Bluetooth isn't available: {text}")
    }
}

fn objects(conn: &Connection) -> Result<Objects, String> {
    let reply = conn
        .call_method(Some(BLUEZ), "/", Some("org.freedesktop.DBus.ObjectManager"), "GetManagedObjects", &())
        .map_err(not_available)?;
    reply.body().deserialize::<Objects>().map_err(|e| e.to_string())
}

fn get<T: TryFrom<OwnedValue>>(props: &HashMap<String, OwnedValue>, key: &str) -> Option<T> {
    props.get(key).cloned().and_then(|v| T::try_from(v).ok())
}

fn call(conn: &Connection, path: &str, iface: &str, method: &str) -> zbus::Result<()> {
    conn.call_method(Some(BLUEZ), path, Some(iface), method, &())?;
    Ok(())
}

/// The first powered adapter's object path.
fn adapter(objs: &Objects) -> Result<String, String> {
    let mut adapters: Vec<(&OwnedObjectPath, bool)> = objs
        .iter()
        .filter_map(|(path, ifaces)| ifaces.get(ADAPTER_IFACE).map(|a| (path, get::<bool>(a, "Powered").unwrap_or(false))))
        .collect();
    adapters.sort_by_key(|(p, _)| p.as_str().to_string());
    match adapters.iter().find(|(_, powered)| *powered) {
        Some((path, _)) => Ok(path.as_str().to_string()),
        None if adapters.is_empty() => Err("No Bluetooth adapter found. Base stations are reached over \
                                            Bluetooth, so the PC needs one (a USB dongle works)."
            .into()),
        None => Err("Bluetooth is switched off. Turn it on, then try again.".into()),
    }
}

/// Base stations BlueZ knows about (seen in a scan, now or earlier).
fn stations_in(objs: &Objects) -> Vec<(String, Found)> {
    let mut out: Vec<(String, Found)> = objs
        .iter()
        .filter_map(|(path, ifaces)| {
            let dev = ifaces.get(DEVICE_IFACE)?;
            let name: String = get(dev, "Name").or_else(|| get(dev, "Alias"))?;
            let version = Version::from_name(&name)?;
            let address: String = get(dev, "Address")?;
            Some((path.as_str().to_string(), Found { address, name, version, rssi: get(dev, "RSSI") }))
        })
        .collect();
    out.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    out
}

/// Look for base stations for `secs` seconds. Returns every station BlueZ now
/// knows, the ones that answered this scan first (they have an `rssi`).
pub fn scan(secs: u64) -> Result<Vec<Found>, String> {
    let conn = bus()?;
    let adapter = adapter(&objects(&conn)?)?;
    let filter: HashMap<&str, Value> = HashMap::from([("Transport", Value::from("le"))]);
    let _ = conn.call_method(Some(BLUEZ), adapter.as_str(), Some(ADAPTER_IFACE), "SetDiscoveryFilter", &(filter,));
    call(&conn, &adapter, ADAPTER_IFACE, "StartDiscovery").map_err(|e| format!("Couldn't start a Bluetooth scan: {e}"))?;
    sleep(Duration::from_secs(secs));
    let _ = call(&conn, &adapter, ADAPTER_IFACE, "StopDiscovery");
    let mut found: Vec<Found> = stations_in(&objects(&conn)?).into_iter().map(|(_, f)| f).collect();
    found.sort_by_key(|f| f.rssi.is_none());
    Ok(found)
}

/// A connected station: its object path and characteristics by UUID. Drops the
/// connection when done (stations take few connections at once).
struct Link<'c> {
    conn: &'c Connection,
    device: String,
    chars: HashMap<String, String>,
}

impl Drop for Link<'_> {
    fn drop(&mut self) {
        let _ = call(self.conn, &self.device, DEVICE_IFACE, "Disconnect");
    }
}

fn device_path(objs: &Objects, address: &str) -> Option<String> {
    objs.iter().find_map(|(path, ifaces)| {
        let addr: String = get(ifaces.get(DEVICE_IFACE)?, "Address")?;
        addr.eq_ignore_ascii_case(address).then(|| path.as_str().to_string())
    })
}

fn connect<'c>(conn: &'c Connection, address: &str) -> Result<Link<'c>, String> {
    let objs = objects(conn)?;
    adapter(&objs)?;
    let device = device_path(&objs, address)
        .ok_or_else(|| format!("Base station {address} isn't known to Bluetooth yet. Scan for it first."))?;
    let mut last = String::new();
    for attempt in 0..CONNECT_TRIES {
        if attempt > 0 {
            sleep(Duration::from_millis(500));
        }
        match call(conn, &device, DEVICE_IFACE, "Connect") {
            Ok(()) => {
                last.clear();
                break;
            }
            Err(e) => last = e.to_string(),
        }
    }
    if !last.is_empty() {
        return Err(format!("Couldn't connect to base station {address}: {last}"));
    }
    let link_device = device.clone();
    let mut link = Link { conn, device, chars: HashMap::new() };
    // The characteristics appear once BlueZ has resolved the services.
    let deadline = Instant::now() + SERVICES_TIMEOUT;
    loop {
        let objs = objects(conn)?;
        let resolved = objs
            .get(&OwnedObjectPath::try_from(link_device.as_str()).map_err(|e| e.to_string())?)
            .and_then(|i| i.get(DEVICE_IFACE))
            .and_then(|d| get::<bool>(d, "ServicesResolved"))
            .unwrap_or(false);
        if resolved {
            let prefix = format!("{link_device}/");
            link.chars = objs
                .iter()
                .filter(|(path, _)| path.as_str().starts_with(&prefix))
                .filter_map(|(path, ifaces)| {
                    let uuid: String = get(ifaces.get(CHAR_IFACE)?, "UUID")?;
                    Some((uuid.to_lowercase(), path.as_str().to_string()))
                })
                .collect();
            return Ok(link);
        }
        if Instant::now() > deadline {
            return Err(format!("Base station {address} connected but didn't list its services."));
        }
        sleep(Duration::from_millis(200));
    }
}

impl Link<'_> {
    fn char_path(&self, uuid: &str) -> Result<&str, String> {
        self.chars
            .get(uuid)
            .map(String::as_str)
            .ok_or_else(|| "This base station doesn't offer that setting.".to_string())
    }

    fn write(&self, uuid: &str, value: &[u8]) -> Result<(), String> {
        let path = self.char_path(uuid)?;
        let opts: HashMap<&str, Value> = HashMap::new();
        self.conn
            .call_method(Some(BLUEZ), path, Some(CHAR_IFACE), "WriteValue", &(value, opts))
            .map(|_| ())
            .map_err(|e| format!("The base station didn't take the command: {e}"))
    }

    fn read(&self, uuid: &str) -> Result<Vec<u8>, String> {
        let path = self.char_path(uuid)?;
        let opts: HashMap<&str, Value> = HashMap::new();
        let reply = self
            .conn
            .call_method(Some(BLUEZ), path, Some(CHAR_IFACE), "ReadValue", &(opts,))
            .map_err(|e| format!("Couldn't read from the base station: {e}"))?;
        reply.body().deserialize::<Vec<u8>>().map_err(|e| e.to_string())
    }
}

// --- Commands ------------------------------------------------------------------

/// The 1.0 command for `power`, for the station with ID `bsid` (8 hex digits).
fn v1_command(power: Power, bsid: &str) -> Result<Vec<u8>, String> {
    let id = u32::from_str_radix(bsid.trim(), 16)
        .ok()
        .filter(|_| bsid.trim().len() == 8)
        .ok_or_else(|| "A 1.0 base station needs its ID: the 8 characters printed on its back.".to_string())?;
    let [a, b, c, d] = id.to_be_bytes();
    let mut cmd = vec![0u8; 20];
    cmd[0] = 0x12;
    match power {
        Power::On => {}
        Power::Sleep => {
            cmd[1] = 0x02;
            cmd[3] = 0x01;
        }
        Power::Standby => return Err("1.0 base stations have no standby, only on and sleep.".into()),
    }
    cmd[4..8].copy_from_slice(&[d, c, b, a]);
    Ok(cmd)
}

fn v2_power_byte(power: Power) -> u8 {
    match power {
        Power::Sleep => 0x00,
        Power::On => 0x01,
        Power::Standby => 0x02,
    }
}

fn v2_power_state(byte: u8) -> PowerState {
    match byte {
        0x00 => PowerState::Sleep,
        0x02 => PowerState::Standby,
        0x0b => PowerState::On,
        _ => PowerState::Waking,
    }
}

/// Switch one station. `bsid` is only needed (and used) for 1.0 stations.
pub fn set_power(address: &str, version: Version, power: Power, bsid: Option<&str>) -> Result<(), String> {
    let conn = bus()?;
    let link = connect(&conn, address)?;
    match version {
        Version::V2 => link.write(V2_POWER, &[v2_power_byte(power)]),
        Version::V1 => link.write(V1_COMMAND, &v1_command(power, bsid.unwrap_or_default())?),
    }
}

/// Switch every saved station, one after the other (BlueZ copes badly with
/// parallel LE connections). Returns one message per station that failed.
/// 1.0 stations without an ID are skipped; they sleep where 2.0 ones stand by.
pub fn switch_all(stations: &[Saved], power: Power) -> Vec<String> {
    let mut failures = Vec::new();
    for s in stations {
        let power = match (s.version, power) {
            (Version::V1, _) if s.bsid.as_deref().unwrap_or("").is_empty() => continue,
            // A 1.0 station sleeps where a 2.0 one would stand by.
            (Version::V1, Power::Standby) => Power::Sleep,
            (_, p) => p,
        };
        if let Err(e) = set_power(&s.address, s.version, power, s.bsid.as_deref()) {
            failures.push(format!("{}: {e}", s.name));
        }
    }
    failures
}

/// Set a 2.0 station's channel (1–16). Every station in a room needs its own.
pub fn set_channel(address: &str, channel: u8) -> Result<(), String> {
    if !(1..=16).contains(&channel) {
        return Err(format!("Channels go from 1 to 16, not {channel}."));
    }
    let conn = bus()?;
    let link = connect(&conn, address)?;
    link.write(V2_CHANNEL, &[channel])
}

/// Blink a 2.0 station's front LED, to tell which one is which.
pub fn identify(address: &str) -> Result<(), String> {
    let conn = bus()?;
    let link = connect(&conn, address)?;
    link.write(V2_IDENTIFY, &[0x00])
}

/// Read a 2.0 station's power state and channel.
pub fn state(address: &str) -> Result<StationState, String> {
    let conn = bus()?;
    let link = connect(&conn, address)?;
    let power = link.read(V2_POWER).ok().and_then(|v| v.first().copied()).map(v2_power_state);
    let channel = link.read(V2_CHANNEL).ok().and_then(|v| v.first().copied()).filter(|c| (1..=16).contains(c));
    Ok(StationState { power, channel })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_from_names() {
        assert_eq!(Version::from_name("LHB-1A2B3C4D"), Some(Version::V2));
        assert_eq!(Version::from_name("HTC BS 4B3C2D"), Some(Version::V1));
        assert_eq!(Version::from_name("Pixel Buds"), None);
    }

    #[test]
    fn v1_commands_carry_the_id_little_endian() {
        let on = v1_command(Power::On, "AABBCCDD").unwrap();
        assert_eq!(&on[..8], &[0x12, 0x00, 0x00, 0x00, 0xdd, 0xcc, 0xbb, 0xaa]);
        assert_eq!(on.len(), 20);
        let off = v1_command(Power::Sleep, "aabbccdd").unwrap();
        assert_eq!(&off[..8], &[0x12, 0x02, 0x00, 0x01, 0xdd, 0xcc, 0xbb, 0xaa]);
        assert!(v1_command(Power::Standby, "AABBCCDD").is_err());
        assert!(v1_command(Power::On, "AABB").is_err());
        assert!(v1_command(Power::On, "ZZZZZZZZ").is_err());
    }

    #[test]
    fn v2_power_bytes() {
        assert_eq!(v2_power_byte(Power::Sleep), 0x00);
        assert_eq!(v2_power_byte(Power::On), 0x01);
        assert_eq!(v2_power_byte(Power::Standby), 0x02);
        assert_eq!(v2_power_state(0x0b), PowerState::On);
        assert_eq!(v2_power_state(0x00), PowerState::Sleep);
        assert_eq!(v2_power_state(0x02), PowerState::Standby);
        assert_eq!(v2_power_state(0x09), PowerState::Waking);
    }
}
