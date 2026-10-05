//! Lighthouse commands: room setup, pairing controllers/trackers to their
//! receivers, base stations over Bluetooth, and the VR udev rules. All of it
//! works without SteamVR; see the matching `monadeck-core` modules.

use crate::state::AppState;
use monadeck_core::basestations::{self, Found, Power, Saved, StationState, Version};
use monadeck_core::config::Backend;
use monadeck_core::devices;
use monadeck_core::floor_calibration;
use monadeck_core::pairing::{self, ReceiverGroup};
use monadeck_core::preflight;
use monadeck_core::room_setup::{self, RoomResult};
use serde::Serialize;
use tauri::State;

type CmdResult<T> = Result<T, String>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

// --- Room setup ----------------------------------------------------------------

#[derive(Serialize)]
pub struct FloorStatus {
    /// SteamVR's `vrcmd` was found: the fallback calibration can run.
    available: bool,
    /// A room setup exists for a universe the lighthouse driver knows.
    calibrated: bool,
    /// The runtime can report the headset's pose: the native room setup can
    /// run (with VR started).
    native: bool,
    /// The lighthouse driver has found base stations at least once.
    has_universe: bool,
}

/// Room setup status for the `steamvr_lh` driver. Cheap fs probe, polled.
#[tauri::command]
pub async fn floor_cal_status(state: State<'_, AppState>) -> CmdResult<FloorStatus> {
    let so = state.config.lock().unwrap().libmonado_so();
    blocking(move || {
        let room = room_setup::status(&so);
        Ok(FloorStatus {
            available: floor_calibration::status().available,
            calibrated: room.calibrated,
            native: room.native,
            has_universe: room.has_universe,
        })
    })
    .await
}

/// Set floor, centre and forward from the headset standing on the floor, with
/// VR running. Blocking (~4 s of sampling + the live-apply check).
#[tauri::command]
pub async fn run_room_setup(state: State<'_, AppState>) -> CmdResult<RoomResult> {
    let st = state.inner().clone();
    blocking(move || {
        if st.config.lock().unwrap().backend != Backend::Monado || !devices::service_connected() {
            return Err("Start VR first: room setup reads where the headset is.".into());
        }
        room_setup::calibrate(|| st.monado.head_pose())
    })
    .await
}

/// The headset's height above the floor right now (metres), for the live
/// readout. `None` when VR isn't running or the runtime can't say.
#[tauri::command]
pub async fn head_height(state: State<'_, AppState>) -> CmdResult<Option<f64>> {
    let st = state.inner().clone();
    blocking(move || {
        if !devices::service_connected() {
            return Ok(None);
        }
        Ok(st.monado.head_pose().ok().filter(|h| h.tracked).map(|h| h.position[1]))
    })
    .await
}

// --- Pairing -------------------------------------------------------------------

/// Plugged-in receivers, named and grouped (a Tundra dongle once). With VR
/// running, each receiver says whether a device is connected through it
/// (listens ~0.4 s).
#[tauri::command]
pub async fn pairing_receivers() -> CmdResult<Vec<ReceiverGroup>> {
    blocking(|| {
        let mut groups = pairing::groups();
        if devices::service_connected() {
            let all: Vec<_> = groups.iter().flat_map(|g| g.receivers.iter().cloned()).collect();
            let active = pairing::activity(&all);
            for r in groups.iter_mut().flat_map(|g| g.receivers.iter_mut()) {
                r.active = active.get(&r.node).copied();
            }
        }
        Ok(groups)
    })
    .await
}

/// Put receivers into pairing mode, all at once (for `pairing::PAIRING_WINDOW`).
#[tauri::command]
pub async fn pairing_start(serials: Vec<String>) -> CmdResult<u64> {
    blocking(move || {
        pairing::start_pairing(&serials)?;
        Ok(pairing::PAIRING_WINDOW.as_secs())
    })
    .await
}

// --- Base stations -------------------------------------------------------------

#[tauri::command]
pub async fn bs_scan(secs: u64) -> CmdResult<Vec<Found>> {
    blocking(move || basestations::scan(secs.clamp(2, 20))).await
}

#[tauri::command]
pub async fn bs_state(address: String) -> CmdResult<StationState> {
    blocking(move || basestations::state(&address)).await
}

#[tauri::command]
pub async fn bs_set_power(address: String, version: Version, power: Power, bsid: Option<String>) -> CmdResult<()> {
    blocking(move || basestations::set_power(&address, version, power, bsid.as_deref())).await
}

#[tauri::command]
pub async fn bs_set_channel(address: String, channel: u8) -> CmdResult<()> {
    blocking(move || basestations::set_channel(&address, channel)).await
}

#[tauri::command]
pub async fn bs_identify(address: String) -> CmdResult<()> {
    blocking(move || basestations::identify(&address)).await
}

/// Switch the saved base stations with VR, if the user asked for it. On a
/// thread of its own: each station takes a couple of seconds over Bluetooth,
/// and VR shouldn't wait for them.
pub(crate) fn switch_base_stations(st: &AppState, starting: bool) {
    let cfg = st.config.lock().unwrap().clone();
    if !cfg.base_stations_auto || cfg.base_stations.is_empty() {
        return;
    }
    let power = if starting { Power::On } else { cfg.base_stations_off };
    let stations: Vec<Saved> = cfg.base_stations;
    std::thread::spawn(move || {
        for failure in basestations::switch_all(&stations, power) {
            log::warn!("base stations: {failure}");
        }
        log::info!("base stations: switched {} to {power:?}", stations.len());
    });
}

/// The same, waiting for it (bounded): for quitting, where a background
/// thread would die with the process.
pub(crate) fn switch_base_stations_off_now(st: &AppState) {
    let cfg = st.config.lock().unwrap().clone();
    if !cfg.base_stations_auto || cfg.base_stations.is_empty() {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let power = cfg.base_stations_off;
    std::thread::spawn(move || {
        let _ = tx.send(basestations::switch_all(&cfg.base_stations, power));
    });
    match rx.recv_timeout(std::time::Duration::from_secs(20)) {
        Ok(failures) => failures.iter().for_each(|f| log::warn!("base stations: {f}")),
        Err(_) => log::warn!("base stations: gave up switching them off on quit"),
    }
}

// --- System --------------------------------------------------------------------

/// Install our copy of the xr-hardware udev rules (prompts via pkexec).
#[tauri::command]
pub async fn install_udev_rules() -> CmdResult<()> {
    blocking(preflight::install_udev_rules).await
}

/// SteamVR's lighthouse driver is installed (monado's steamvr_lh loads it).
#[tauri::command]
pub async fn steamvr_installed() -> CmdResult<bool> {
    blocking(|| Ok(monadeck_core::steamvr::lighthouse_driver_installed())).await
}
