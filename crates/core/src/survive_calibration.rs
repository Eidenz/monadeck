//! Libsurvive calibration for the FLOSS `survive` lighthouse driver.
//!
//! When the `survive` driver is selected (`LH_DRIVER=survive`), monado tracks via
//! libsurvive, which needs to know where your base stations are. Rather than make
//! the user run libsurvive's own lengthy calibration, we import SteamVR's existing
//! lighthouse solve: `survive-cli --steamvr-calibration <lighthousedb.json>` seeds
//! libsurvive from SteamVR's base-station database, then libsurvive runs for a bit
//! (headset still, in view of the bases) to converge and write its config.
//!
//! Ported from Envision's libsurvive setup window. This is the `survive`-driver
//! sibling of [`crate::floor_calibration`] (which serves the `steamvr` driver).
//!
//! `survive-cli` ships with libsurvive — it is NOT built by monado — so it may be
//! at `<monado_prefix>/bin/survive-cli` (a build that bundles libsurvive) or on
//! `$PATH` (a system libsurvive package). If it's absent we say so and disable the
//! action rather than guessing. We can't reliably detect a *completed* libsurvive
//! calibration (its config path is install/CWD dependent), so unlike the SteamVR
//! floor calibration there's no persistent "done" state — it's a run-on-demand
//! action only.

use crate::cmd_runner::CmdRunner;
use crate::steam;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

/// How long libsurvive runs per pass (Envision uses 30s twice: seed+settle, then
/// refine).
const RUN_SECONDS: u64 = 30;

/// `survive-cli` from the monado prefix's `bin/` first (a build that bundles
/// libsurvive), else from `$PATH` (a system libsurvive package).
fn survive_cli(prefix: &Path) -> Option<PathBuf> {
    let in_prefix = prefix.join("bin").join("survive-cli");
    if in_prefix.is_file() {
        return Some(in_prefix);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|d| {
        let p = d.join("survive-cli");
        p.is_file().then_some(p)
    })
}

/// SteamVR's lighthouse database (base-station solve) to import from.
fn lighthousedb() -> Option<PathBuf> {
    steam::steam_config_roots().into_iter().find_map(|r| {
        let p = r.join("config/lighthouse/lighthousedb.json");
        p.is_file().then_some(p)
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct SurviveCalStatus {
    /// `survive-cli` was located — calibration can be run at all.
    pub available: bool,
    /// A SteamVR `lighthousedb.json` exists to import the base-station solve from.
    pub source_present: bool,
}

/// Cheap filesystem probe. `prefix` is the configured monado build prefix.
pub fn status(prefix: &Path) -> SurviveCalStatus {
    SurviveCalStatus {
        available: survive_cli(prefix).is_some(),
        source_present: lighthousedb().is_some(),
    }
}

/// Import SteamVR's lighthouse calibration into libsurvive. Blocking (~1 min:
/// runs libsurvive twice). The caller MUST ensure monado-service is stopped —
/// survive-cli needs exclusive access to the headset.
pub fn run(prefix: &Path) -> Result<(), String> {
    let cli = survive_cli(prefix).ok_or_else(|| {
        "survive-cli not found. It ships with libsurvive — install libsurvive, or \
         use a Monado build that bundles it."
            .to_string()
    })?;
    let lhdb = lighthousedb().ok_or_else(|| {
        "No SteamVR lighthouse database found (config/lighthouse/lighthousedb.json). \
         Run SteamVR room setup once so it records your base stations."
            .to_string()
    })?;

    // survive-cli loads libsurvive.so; point at the prefix's lib dirs (harmless if
    // it's a system binary that resolves its own).
    let ld = ["lib", "lib64"]
        .iter()
        .map(|d| prefix.join(d).to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(":");
    let mut env = HashMap::new();
    env.insert("LD_LIBRARY_PATH".to_string(), ld);

    let cmd = cli.to_string_lossy().into_owned();
    let args = vec![
        "--steamvr-calibration".to_string(),
        lhdb.to_string_lossy().into_owned(),
    ];

    // Two passes, SIGTERM between/after (CmdRunner::terminate) so libsurvive writes
    // its config on the way down rather than being SIGKILLed mid-write.
    for pass in 0..2 {
        let mut runner = CmdRunner::new();
        runner
            .start(&cmd, &args, &env)
            .map_err(|e| format!("Couldn't start survive-cli: {e}"))?;

        // Catch an immediate exit (bad args, missing lib) on the first pass.
        sleep(Duration::from_millis(800));
        if pass == 0 && !runner.is_running() {
            let tail = runner.lines().join("\n");
            let tail = tail.trim();
            return Err(if tail.is_empty() {
                "survive-cli exited immediately — is libsurvive set up correctly?".to_string()
            } else {
                format!("survive-cli failed:\n{tail}")
            });
        }

        sleep(Duration::from_secs(RUN_SECONDS) - Duration::from_millis(800));
        runner.terminate();
    }
    Ok(())
}
