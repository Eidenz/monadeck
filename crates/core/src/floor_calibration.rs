//! SteamVR floor / play-space calibration for the `steamvr_lh` tracking driver.
//!
//! Monado's SteamVR Lighthouse wrapper does NOT compute the floor itself — on
//! start it reads SteamVR's `config/chaperone_info.vrchap` (the `standing` pose:
//! floor height Y + recenter yaw) and applies it to every tracked pose. With no
//! chaperone file the play-space center and floor are wrong — you sink into or
//! float above the floor and face the wrong way. See monado
//! `src/xrt/drivers/steamvr_lh/device.cpp` `Device::init_chaperone`.
//!
//! So a lighthouse user must have run SteamVR room setup at least once (and again
//! after meaningfully moving their base stations, which can mint a new tracking
//! "universe" the old calibration no longer matches). We don't need SteamVR's GUI
//! wizard for that: SteamVR ships a headless `vrcmd` tool, and
//! `vrcmd --resetroomsetup` writes the same chaperone file. This is exactly what
//! Envision's "SteamVR Quick Calibration" does, ported here — bring up a
//! pose-polling server (`vrcmd --pollposes`) so there's a live HMD pose to anchor
//! the floor to, then run `--resetroomsetup`.
//!
//! This is distinct from [`crate::playspace_overrides`]: that nudges the play
//! space at runtime (OVRAS-style) *on top of* whatever base the driver reports;
//! this sets that base.
//!
//! The HMD must be on the floor, in the middle of the play area, with controllers
//! off; the HMD's facing sets the play-space forward direction.

use crate::steam;
use serde::Serialize;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

/// `<lib>/steamapps/common/SteamVR/bin/linux64` — the dir holding `vrcmd` — found
/// by scanning the Steam libraries, the same place Envision looks. `None` if
/// SteamVR isn't installed.
fn steamvr_bin_dir() -> Option<PathBuf> {
    steam::library_folders().into_iter().find_map(|lib| {
        let dir = lib.join("steamapps/common/SteamVR/bin/linux64");
        dir.join("vrcmd").is_file().then_some(dir)
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct FloorCalStatus {
    /// SteamVR's `vrcmd` tool was located — floor calibration can be run at all.
    pub available: bool,
    /// A `chaperone_info.vrchap` exists where steamvr_lh reads it: room setup /
    /// floor calibration has been done at least once.
    pub calibrated: bool,
}

/// Cheap filesystem probe of the floor-calibration status. Safe to poll.
pub fn status() -> FloorCalStatus {
    let calibrated = steam::steam_config_roots()
        .iter()
        .any(|r| r.join("config/chaperone_info.vrchap").is_file());
    FloorCalStatus {
        available: steamvr_bin_dir().is_some(),
        calibrated,
    }
}

/// Run a quick SteamVR floor / play-space calibration via `vrcmd`, writing
/// `chaperone_info.vrchap`. Blocking: spawns vrserver and waits for the reset.
///
/// The caller MUST ensure monado-service is stopped first — `vrcmd` brings up
/// vrserver, which needs exclusive access to the headset.
pub fn run() -> Result<(), String> {
    let bin_dir = steamvr_bin_dir().ok_or_else(|| {
        "SteamVR not found. The lighthouse driver needs SteamVR installed — it \
         provides the vrcmd tool used to calibrate the floor."
            .to_string()
    })?;
    let vrcmd = bin_dir.join("vrcmd");
    // vrcmd dlopen()s vrserver and the driver libs from its own bin dir.
    let ld = bin_dir.as_os_str();

    // Pose-polling server: brings vrserver + the lighthouse driver up so the reset
    // has a live HMD pose (the headset sitting on the floor) to anchor to.
    let mut server = Command::new(&vrcmd)
        .arg("--pollposes")
        .env("LD_LIBRARY_PATH", ld)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Couldn't start vrcmd: {e}"))?;

    // Give vrserver a moment to come up before the reset reads poses — the same
    // 2s grace Envision's calibration uses.
    sleep(Duration::from_secs(2));

    // The actual calibration: writes chaperone_info.vrchap from the current pose.
    let out = Command::new(&vrcmd)
        .arg("--resetroomsetup")
        .env("LD_LIBRARY_PATH", ld)
        .output();

    // Always tear the server back down, whatever the reset did.
    let _ = server.kill();
    let _ = server.wait();

    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let detail = String::from_utf8_lossy(&o.stderr);
            let detail = detail.trim();
            match (o.status.code(), detail.is_empty()) {
                (Some(c), false) => Err(format!("Calibration failed (code {c}): {detail}")),
                (Some(c), true) => Err(format!("Calibration failed (code {c})")),
                (None, _) => Err("Calibration was terminated before it finished".to_string()),
            }
        }
        Err(e) => Err(format!("Couldn't run vrcmd --resetroomsetup: {e}")),
    }
}
