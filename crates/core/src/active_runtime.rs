//! Manage `~/.config/openxr/1/active_runtime.json` — the file the OpenXR loader
//! reads to choose a runtime — with a backup so switching to monado and back is
//! always reversible and never eats an existing SteamVR setup.
//!
//! Ported from Envision's `active_runtime_json.rs`. Preferred path: symlink the
//! monado-provided manifest. Fallback: synthesize the JSON pointing at
//! `libopenxr_monado.so` + `MND_libmonado_path`.

use crate::config::MonadeckConfig;
use crate::paths::active_runtime_path;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, create_dir_all};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

const BACKUP_NAME: &str = "active_runtime.json.monadeck.bak";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveRuntimeInner {
    #[serde(
        rename = "VALVE_runtime_is_steamvr",
        skip_serializing_if = "Option::is_none"
    )]
    pub valve_runtime_is_steamvr: Option<bool>,
    #[serde(rename = "MND_libmonado_path", skip_serializing_if = "Option::is_none")]
    pub libmonado_path: Option<PathBuf>,
    pub library_path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveRuntime {
    pub file_format_version: String,
    pub runtime: ActiveRuntimeInner,
}

/// What's installed as the active OpenXR runtime right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveRuntimeKind {
    Monado,
    Wivrn,
    SteamVr,
    Other,
    None,
}

fn backup_path() -> PathBuf {
    active_runtime_path()
        .parent()
        .expect("active_runtime path always has a parent")
        .join(BACKUP_NAME)
}

fn read_runtime(path: &Path) -> Option<ActiveRuntime> {
    let f = fs::File::open(path).ok()?;
    serde_json::from_reader(f).ok()
}

/// The currently installed active runtime, if the file parses.
pub fn current() -> Option<ActiveRuntime> {
    read_runtime(&active_runtime_path())
}

fn looks_like_wivrn(ar: &ActiveRuntime) -> bool {
    // WiVRn's manifest is Monado-shaped (it even names itself "Monado" and
    // carries MND_libmonado_path); its library path is the tell.
    ar.runtime
        .library_path
        .to_string_lossy()
        .to_lowercase()
        .contains("wivrn")
}

fn looks_like_monado(ar: &ActiveRuntime) -> bool {
    !looks_like_wivrn(ar)
        && (ar.runtime.libmonado_path.is_some()
            || ar
                .runtime
                .library_path
                .to_string_lossy()
                .to_lowercase()
                .contains("monado"))
}

/// Either of the runtimes Monadeck itself installs — never worth backing up.
fn looks_like_ours(ar: &ActiveRuntime) -> bool {
    looks_like_monado(ar) || looks_like_wivrn(ar)
}

fn looks_like_steamvr(ar: &ActiveRuntime) -> bool {
    ar.runtime.valve_runtime_is_steamvr == Some(true)
        || ar
            .runtime
            .library_path
            .to_string_lossy()
            .to_lowercase()
            .contains("steamvr")
}

/// Classify the active runtime for display.
pub fn kind() -> ActiveRuntimeKind {
    let p = active_runtime_path();
    if !p.exists() && !p.is_symlink() {
        return ActiveRuntimeKind::None;
    }
    match current() {
        Some(ar) if looks_like_wivrn(&ar) => ActiveRuntimeKind::Wivrn,
        Some(ar) if looks_like_monado(&ar) => ActiveRuntimeKind::Monado,
        Some(ar) if looks_like_steamvr(&ar) => ActiveRuntimeKind::SteamVr,
        Some(_) => ActiveRuntimeKind::Other,
        None => ActiveRuntimeKind::Other,
    }
}

/// Make our own backup writable/removable. The file we replace can be a symlink
/// (don't chmod through it) or a read-only regular file (SteamVR writes those).
fn make_writable(path: &Path) -> Result<()> {
    if path.is_symlink() {
        return Ok(());
    }
    if let Ok(meta) = fs::metadata(path) {
        let mut perms = meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(path, perms).ok();
    }
    Ok(())
}

/// Clear the way for our runtime: back up whatever foreign file is there now
/// (once), or just drop our own previous entry. Returns the destination path.
///
/// Never clobbers an existing backup and never backs up our own symlink, or a
/// crash-without-restore would lose the real pre-Monadeck original (or its
/// absence) forever.
fn prepare_dest() -> Result<PathBuf> {
    let dest = active_runtime_path();
    if dest.is_dir() {
        bail!("{} is a directory, refusing to touch it", dest.display());
    }
    let parent = dest.parent().expect("has parent");
    create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    if dest.is_file() || dest.is_symlink() {
        let is_ours = current().map(|ar| looks_like_ours(&ar)).unwrap_or(false);
        let bak = backup_path();
        make_writable(&dest)?;
        if is_ours || bak.exists() {
            // Already our runtime, or the genuine original is already saved:
            // just drop the current file before recreating it.
            fs::remove_file(&dest).ok();
        } else {
            // First time replacing a foreign runtime (e.g. SteamVR): preserve it.
            fs::rename(&dest, &bak)
                .with_context(|| format!("backing up {} -> {}", dest.display(), bak.display()))?;
        }
    }
    Ok(dest)
}

/// Point the OpenXR loader at WiVRn by symlinking its manifest (which carries
/// `MND_libmonado_path`, so libmonado-rs finds `libmonado_wivrn.so` through it).
/// Backs up any existing foreign runtime first, like [`set_to_monado`].
pub fn set_to_wivrn(manifest: &Path) -> Result<()> {
    if !manifest.is_file() {
        bail!("WiVRn manifest {} not found", manifest.display());
    }
    let dest = prepare_dest()?;
    symlink(manifest, &dest)
        .with_context(|| format!("symlinking {} -> {}", manifest.display(), dest.display()))?;
    Ok(())
}

/// Point the OpenXR loader at monado. Backs up any existing runtime first.
pub fn set_to_monado(config: &MonadeckConfig) -> Result<()> {
    let dest = prepare_dest()?;

    // Preferred: symlink monado's own manifest so it tracks the build.
    let manifest = config.openxr_monado_json();
    if manifest.is_file() {
        symlink(&manifest, &dest)
            .with_context(|| format!("symlinking {} -> {}", manifest.display(), dest.display()))?;
        return Ok(());
    }

    // Fallback: synthesize the JSON from the prefix's libs.
    let lib = config.libopenxr_monado_so();
    if !lib.is_file() {
        bail!(
            "neither {} nor {} exist; is the monado prefix correct?",
            manifest.display(),
            lib.display()
        );
    }
    let ar = ActiveRuntime {
        file_format_version: "1.0.0".to_string(),
        runtime: ActiveRuntimeInner {
            valve_runtime_is_steamvr: None,
            libmonado_path: config
                .libmonado_so()
                .is_file()
                .then(|| config.libmonado_so()),
            library_path: lib,
            name: Some("Monado (Monadeck)".to_string()),
        },
    };
    let f = fs::File::create(&dest).with_context(|| format!("writing {}", dest.display()))?;
    serde_json::to_writer_pretty(f, &ar)?;
    Ok(())
}

/// Restore the backed-up runtime (e.g. SteamVR), undoing [`set_to_monado`].
pub fn restore_backup() -> Result<()> {
    let dest = active_runtime_path();
    let bak = backup_path();
    if !(bak.is_file() || bak.is_symlink()) {
        // Nothing to restore: just remove our monado entry so we leave no runtime
        // dangling rather than asserting one that isn't ours.
        if dest.is_file() || dest.is_symlink() {
            make_writable(&dest)?;
            fs::remove_file(&dest).ok();
        }
        return Ok(());
    }
    if dest.is_dir() {
        bail!("{} is a directory", dest.display());
    }
    if dest.is_file() || dest.is_symlink() {
        make_writable(&dest)?;
        fs::remove_file(&dest).ok();
    }
    fs::rename(&bak, &dest)
        .with_context(|| format!("restoring {} -> {}", bak.display(), dest.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Swap the real `active_runtime.json` to WiVRn's manifest and back, the way
    /// start/stop do. Skipped unless WiVRn is installed; reversible: whatever was
    /// there before (a file, a symlink, or nothing) is put back. Run with
    /// `cargo test -p monadeck-core -- --ignored`.
    #[test]
    #[ignore]
    fn wivrn_runtime_swap_roundtrip() {
        let Some(bin) = crate::wivrn::detect_server() else {
            eprintln!("wivrn-server not installed; skipping");
            return;
        };
        let manifest = crate::wivrn::manifest_for(&bin).expect("manifest");
        let dest = active_runtime_path();
        let before = fs::read_link(&dest)
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .or_else(|| fs::read_to_string(&dest).ok());
        let before_kind = kind();

        set_to_wivrn(&manifest).expect("set_to_wivrn");
        assert_eq!(kind(), ActiveRuntimeKind::Wivrn);
        assert_eq!(fs::read_link(&dest).unwrap(), manifest);
        // Re-applying must not back up our own symlink.
        set_to_wivrn(&manifest).expect("set_to_wivrn again");
        assert_eq!(kind(), ActiveRuntimeKind::Wivrn);

        restore_backup().expect("restore");
        assert_eq!(kind(), before_kind);
        let after = fs::read_link(&dest)
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .or_else(|| fs::read_to_string(&dest).ok());
        assert_eq!(after, before, "active_runtime.json not restored");
    }
}
