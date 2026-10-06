//! Built-in in-headset overlay launcher.
//!
//! The overlay (`monadeck-overlay`, from `crates/overlay`) ships inside the
//! bundle as a Tauri sidecar. It's surfaced as a permanent, non-removable entry
//! in the auto-launch list (toggled by `config.overlay_enabled`) rather than a
//! user-added plugin, so we resolve its path ourselves instead of storing one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use monadeck_core::host;

/// Locate the overlay binary: env override → sidecar next to us → dev target → PATH.
/// Mirrors nemurixr's resolver so dev (`pnpm tauri dev`) and bundled installs
/// both work.
fn overlay_bin() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MONADECK_OVERLAY_BIN") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Bundled: Tauri stages the sidecar (suffix stripped) next to the app.
            let sidecar = dir.join("monadeck-overlay");
            if sidecar.exists() {
                return Some(sidecar);
            }
            // Dev: desktop/src-tauri/target/<profile>/ → root workspace target.
            for rel in [
                "../../../../target/debug/monadeck-overlay",
                "../../../../target/release/monadeck-overlay",
            ] {
                let c = dir.join(rel);
                if c.exists() {
                    return Some(c);
                }
            }
        }
    }
    std::env::var("PATH").ok().and_then(|path| {
        path.split(':')
            .map(|d| Path::new(d).join("monadeck-overlay"))
            .find(|c| c.exists())
    })
}

/// `~/.local/share/monadeck/overlay.log`, emptied.
fn log_file() -> Option<std::fs::File> {
    let dir = monadeck_core::paths::monadeck_data_dir();
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::File::create(dir.join("overlay.log")).ok()
}

/// Spawn the overlay detached, with `env` overlaid (so it inherits the same
/// runtime wiring monado-service got). Returns the spawned child so the caller
/// can stop it when the service goes down.
pub fn launch(env: &HashMap<String, String>) -> Result<Child, String> {
    let bin = overlay_bin()
        .ok_or_else(|| "overlay binary not found (build it: cargo build -p monadeck-overlay)".to_string())?;
    // Its log (and why it stopped, if it does), fresh each launch.
    let log = log_file();
    let out = log.as_ref().and_then(|f| f.try_clone().ok()).map_or_else(Stdio::null, Stdio::from);
    let err = log.map_or_else(Stdio::null, Stdio::from);
    let child = host::command(&bin)
        .envs(env)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("launching overlay {}: {e}", bin.display()))?;
    log::info!("launched built-in overlay: {}", bin.display());
    Ok(child)
}
