//! Persisted Monadeck settings and the paths derived from them.
//!
//! The whole configuration is intentionally tiny — it reflects the narrow
//! workflow Monadeck targets: point at a monado build prefix, optionally set
//! some env vars, register xrizer, and launch a few plugins by path.

use crate::paths::monadeck_config_dir;
use crate::plugins::Plugin;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{create_dir_all, File};
use std::path::PathBuf;

/// The OpenVR compatibility layer to register. Monadeck targets xrizer; the enum
/// exists so the field is explicit and forward-compatible, not because we plan to
/// grow a build system around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OvrRuntime {
    #[default]
    Xrizer,
    /// Don't touch openvrpaths.vrpath at all (OpenXR-only usage).
    None,
}

fn default_true() -> bool {
    true
}

fn default_render_scale() -> u32 {
    // Match Envision's Lighthouse default — a sharper image out of the box.
    140
}

fn default_lh_driver() -> String {
    // The Bigscreen Beyond (and other SteamVR-tracked HMDs that aren't Vive/Index)
    // need monado's steamvr_lh wrapper, enabled via STEAMVR_LH_ENABLE — the same
    // default as Envision's Lighthouse profile.
    "steamvr".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonadeckConfig {
    /// Install/build prefix of the monado fork, e.g. `~/monado/build/install`.
    /// `monado-service`, the libs, and the runtime manifest are all derived from
    /// this — an explicit anchor is the only reliable way to find a custom fork.
    pub monado_prefix: PathBuf,

    /// Directory that contains the xrizer OpenVR runtime (the path written into
    /// `openvrpaths.vrpath`'s `runtime` list). `None` until the user sets it.
    pub xrizer_path: Option<PathBuf>,

    /// Which OpenVR compat layer to register on start.
    #[serde(default)]
    pub ovr_runtime: OvrRuntime,

    /// Hide the deck to the tray on close instead of quitting the app.
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,

    /// Start monado-service automatically when Monadeck launches.
    #[serde(default)]
    pub auto_start: bool,

    /// Compositor render scale, percent (`XRT_COMPOSITOR_SCALE_PERCENTAGE`).
    /// 100 = native; >100 supersamples for a sharper image (Envision uses 140).
    #[serde(default = "default_render_scale")]
    pub render_scale: u32,

    /// `U_PACING_APP_USE_MIN_FRAME_PERIOD` — unlock the compositor refresh from a
    /// power-of-two of the HMD rate; usually a sizeable perf boost.
    #[serde(default = "default_true")]
    pub min_frame_period: bool,

    /// `XRT_COMPOSITOR_COMPUTE` — use the GPU compute compositor.
    #[serde(default = "default_true")]
    pub compute_compositor: bool,

    /// `XRT_DEBUG_GUI` (+ `XRT_CURATED_GUI`) — monado's debug/preview window.
    #[serde(default)]
    pub debug_gui: bool,

    /// Lighthouse tracking driver. `steamvr` uses monado's steamvr_lh wrapper
    /// (enabled via `STEAMVR_LH_ENABLE=true`) and is needed for the Bigscreen
    /// Beyond; `vive`/`survive` use the FLOSS drivers (set via `LH_DRIVER`). An
    /// explicit `LH_DRIVER` in the env overrides this. See start_service.
    #[serde(default = "default_lh_driver")]
    pub lighthouse_driver: String,

    /// Environment variables injected into `monado-service` (your custom vars).
    #[serde(default)]
    pub environment: BTreeMap<String, String>,

    /// Apps to launch by explicit path alongside the service.
    #[serde(default)]
    pub plugins: Vec<Plugin>,
}

impl Default for MonadeckConfig {
    fn default() -> Self {
        Self {
            // A sensible guess; the UI lets the user correct it. Empty would be
            // more honest but this makes first-run autodetect cheaper to attempt.
            monado_prefix: PathBuf::new(),
            xrizer_path: None,
            ovr_runtime: OvrRuntime::default(),
            minimize_to_tray: true,
            auto_start: false,
            render_scale: default_render_scale(),
            min_frame_period: true,
            compute_compositor: true,
            debug_gui: false,
            lighthouse_driver: default_lh_driver(),
            environment: BTreeMap::new(),
            plugins: Vec::new(),
        }
    }
}

impl MonadeckConfig {
    fn config_file() -> PathBuf {
        monadeck_config_dir().join("config.json")
    }

    /// Load config, returning defaults if the file doesn't exist yet.
    pub fn load() -> Self {
        let path = Self::config_file();
        match File::open(&path) {
            Ok(f) => match serde_json::from_reader(f) {
                Ok(cfg) => cfg,
                Err(e) => {
                    log::warn!("config at {} is invalid ({e}); using defaults", path.display());
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_file();
        if let Some(parent) = path.parent() {
            create_dir_all(parent)
                .with_context(|| format!("creating config dir {}", parent.display()))?;
        }
        let f = File::create(&path).with_context(|| format!("writing {}", path.display()))?;
        serde_json::to_writer_pretty(f, self)?;
        Ok(())
    }

    // --- Paths derived from the monado prefix -------------------------------

    /// `<prefix>/bin/monado-service` — the binary we spawn and setcap.
    pub fn monado_service_bin(&self) -> PathBuf {
        self.monado_prefix.join("bin").join("monado-service")
    }

    /// First existing `<prefix>/<lib|lib64>/<name>`, preferring `lib`. Falls back
    /// to the `lib` path even when neither exists so callers get a usable value.
    fn lib_file(&self, name: &str) -> PathBuf {
        for libdir in ["lib", "lib64"] {
            let p = self.monado_prefix.join(libdir).join(name);
            if p.exists() {
                return p;
            }
        }
        self.monado_prefix.join("lib").join(name)
    }

    /// The OpenXR runtime shared object monado provides.
    pub fn libopenxr_monado_so(&self) -> PathBuf {
        self.lib_file("libopenxr_monado.so")
    }

    /// The libmonado shared object — what `MND_libmonado_path` points at and what
    /// libmonado-rs `dlopen`s to talk to the running service.
    pub fn libmonado_so(&self) -> PathBuf {
        self.lib_file("libmonado.so")
    }

    /// Monado's prebuilt OpenXR manifest, symlinked as the active runtime when
    /// present (preferred over synthesizing the JSON ourselves).
    pub fn openxr_monado_json(&self) -> PathBuf {
        self.monado_prefix
            .join("share")
            .join("openxr")
            .join("1")
            .join("openxr_monado.json")
    }

    /// Quick sanity check that the prefix actually points at a monado build.
    pub fn prefix_looks_valid(&self) -> bool {
        self.monado_service_bin().is_file()
    }
}
