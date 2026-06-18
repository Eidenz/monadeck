//! Well-known file locations, computed from the XDG base-directory spec.
//!
//! We avoid an extra crate dependency and read the environment directly: this is
//! the same set of paths Envision touches (`~/.config/openxr/1/active_runtime.json`,
//! `~/.config/openvr/openvrpaths.vrpath`) plus Monadeck's own config dir.

use std::path::PathBuf;

/// `$XDG_CONFIG_HOME`, or `$HOME/.config` as the spec-mandated fallback.
pub fn config_home() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let p = PathBuf::from(xdg);
        if p.is_absolute() {
            return p;
        }
    }
    home().join(".config")
}

/// `$HOME`. Panics only in the degenerate case of an environment with no HOME,
/// which would break essentially every desktop assumption anyway.
pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .expect("HOME is not set")
}

/// `~/.config/openxr` — the OpenXR loader's config dir.
pub fn openxr_config_dir() -> PathBuf {
    config_home().join("openxr")
}

/// `~/.config/openxr/1/active_runtime.json` — the file the OpenXR loader reads to
/// decide which runtime to load. Major version `1` matches the OpenXR API major.
pub fn active_runtime_path() -> PathBuf {
    openxr_config_dir().join("1").join("active_runtime.json")
}

/// `~/.config/openvr` — where OpenVR (and thus xrizer) looks for its config.
pub fn openvr_config_dir() -> PathBuf {
    config_home().join("openvr")
}

/// `~/.config/openvr/openvrpaths.vrpath` — points OpenVR clients at a runtime.
pub fn openvrpaths_path() -> PathBuf {
    openvr_config_dir().join("openvrpaths.vrpath")
}

/// `~/.config/monadeck` — Monadeck's own state directory.
pub fn monadeck_config_dir() -> PathBuf {
    config_home().join("monadeck")
}

/// `$XDG_DATA_HOME`, or `$HOME/.local/share` as the spec-mandated fallback.
pub fn data_home() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        let p = PathBuf::from(xdg);
        if p.is_absolute() {
            return p;
        }
    }
    home().join(".local/share")
}

/// `~/.local/share/monadeck` — where Monadeck parks larger downloaded payloads
/// (built-in Monado runtimes, xrizer), kept out of the config dir.
pub fn monadeck_data_dir() -> PathBuf {
    data_home().join("monadeck")
}

/// Where backups of clobbered runtime files are parked, so a toggle is always
/// reversible and we never destroy an existing SteamVR setup.
pub fn backup_dir() -> PathBuf {
    monadeck_config_dir().join("backups")
}
