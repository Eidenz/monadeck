//! UEVR ("VR Mod") integration — launch a non-Steam Unreal Engine game with
//! praydog's UEVR injected, headlessly, via the `chihuahua` injector under Proton.
//!
//! This reproduces the recipe proven by hand (see `~/PROJECTS/UEVR-LINUX-SPIKE.md`):
//!
//! ```text
//! protontricks-launch --no-runtime --no-bwrap --appid <id> \
//!     <chihuahua.exe> '<Z:\ wine path to game .exe>' \
//!     --runtime OpenXR --delay <N> --uevr-build Nightly
//! ```
//!
//! `chihuahua` is a self-contained .NET 8 Windows exe that launches the game,
//! waits, auto-downloads + injects UEVR, and cleans up on exit — so no .NET need
//! be installed in the prefix. `--no-runtime --no-bwrap` runs it outside the Steam
//! Linux Runtime sandbox so (a) it can see the game PID and (b) the host OpenXR
//! runtime (monado) is directly visible to the in-prefix Wine OpenXR loader.
//!
//! v1 scope: **non-Steam shortcuts only** — they carry the launch exe + working
//! dir in `shortcuts.vdf`, and `--appid <shortcut id>` names their compatdata
//! prefix. (Steam apps launch via `steam://rungameid` and don't expose an exe path
//! here; supporting them is a later extension.)

use crate::paths::{home, monadeck_config_dir, monadeck_data_dir};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// --- per-game "VR Mod enabled" store (keyed by cover id, like favorites) -------

fn enabled_file() -> PathBuf {
    monadeck_config_dir().join("uevr_games.json")
}

/// The set of game keys (cover id = Steam appid or non-Steam shortcut appid) the
/// user has flagged to launch through UEVR. Empty if none / unreadable.
pub fn load_enabled() -> HashSet<String> {
    fs::read_to_string(enabled_file())
        .ok()
        .and_then(|c| serde_json::from_str::<Vec<String>>(&c).ok())
        .map(|v| v.into_iter().collect())
        .unwrap_or_default()
}

/// Persist the set of UEVR-enabled game keys (best-effort).
pub fn save_enabled(games: &HashSet<String>) {
    let path = enabled_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut keys: Vec<&String> = games.iter().collect();
    keys.sort();
    if let Ok(json) = serde_json::to_string_pretty(&keys) {
        let _ = fs::write(path, json);
    }
}

// --- launch -------------------------------------------------------------------

/// Knobs for an injection launch. Defaults match the proven recipe.
#[derive(Debug, Clone)]
pub struct LaunchOpts {
    /// Path to `chihuahua.exe`. `None` → autodetect (see [`detect_chihuahua`]).
    pub chihuahua: Option<PathBuf>,
    /// UEVR runtime: "OpenXR" (direct to monado) or "OpenVR" (via xrizer).
    pub runtime: String,
    /// Seconds chihuahua waits after launching the game before injecting.
    pub delay: u32,
}

impl Default for LaunchOpts {
    fn default() -> Self {
        Self { chihuahua: None, runtime: "OpenXR".into(), delay: 30 }
    }
}

/// Launch `appid`'s game with UEVR injected. `exe`/`start_dir` come from the
/// non-Steam shortcut (Linux paths). Fire-and-forget: chihuahua owns the game's
/// lifetime. Returns an error (without launching) if chihuahua or the game binary
/// can't be found, so the caller can fall back to a plain launch.
pub fn launch(appid: &str, exe: &str, start_dir: &str, opts: &LaunchOpts) -> std::io::Result<()> {
    let chihuahua = opts.chihuahua.clone().or_else(detect_chihuahua).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "chihuahua.exe not found — set MONADECK_CHIHUAHUA or place it at ~/PROJECTS/chihuahua/chihuahua.exe",
        )
    })?;
    let game_exe = shipping_exe(start_dir, exe).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no game executable found to inject")
    })?;
    let wine = to_wine_path(&game_exe);
    let delay = opts.delay.to_string();

    let log_path = monadeck_data_dir().join("chihuahua").join("last-launch.log");
    if let Some(dir) = log_path.parent() {
        let _ = fs::create_dir_all(dir);
    }

    // chihuahua sets `Console.OutputEncoding` on startup, which throws under wine
    // ("Invalid access") unless stdout is a real terminal — so it crashes instantly
    // and the game never launches when spawned with a pipe/file (as from the
    // overlay). Run it under a PTY via `script` (util-linux), which both gives it a
    // tty AND captures its output to the log. This mirrors the proven manual run.
    let cstr = chihuahua.to_string_lossy();
    let inner = shell_join(&[
        "protontricks-launch",
        "--no-runtime",
        "--no-bwrap",
        "--appid",
        appid,
        cstr.as_ref(),
        wine.as_str(),
        "--runtime",
        opts.runtime.as_str(),
        "--delay",
        delay.as_str(),
        "--uevr-build",
        "Nightly",
    ]);
    log::info!("UEVR: launching under pty: {inner}  (output -> {})", log_path.display());

    let spawned = Command::new("script")
        .arg("-qec")
        .arg(&inner)
        .arg(&log_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Err(e) = spawned {
        // `script` missing — fall back to a direct spawn. chihuahua may still crash
        // without a tty, but this is better than failing to launch entirely.
        log::warn!("`script` unavailable ({e}); spawning chihuahua directly (no pty)");
        Command::new("protontricks-launch")
            .args(["--no-runtime", "--no-bwrap", "--appid", appid])
            .arg(&chihuahua)
            .arg(&wine)
            .arg("--runtime")
            .arg(&opts.runtime)
            .arg("--delay")
            .arg(&delay)
            .args(["--uevr-build", "Nightly"])
            .spawn()?;
    }
    Ok(())
}

/// Locate the actual Unreal shipping binary to inject. The shortcut's own `exe`
/// is often a thin top-level launcher (e.g. `Game.exe`), whereas UEVR must target
/// the `<Project>/Binaries/Win64/<Project>-Win64-Shipping.exe` process — so probe
/// the game dir for a `*-Shipping.exe` first (bounded depth; the dir is the game's
/// own folder, not a whole Steam library, so this stays fast). Fall back to the
/// shortcut exe if no shipping binary is found.
fn shipping_exe(start_dir: &str, fallback_exe: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(dequote(start_dir));
    if dir.is_dir() {
        for e in walkdir::WalkDir::new(&dir).max_depth(5).into_iter().filter_map(|e| e.ok()) {
            if e.file_name().to_string_lossy().to_lowercase().ends_with("-shipping.exe") {
                return Some(e.path().to_path_buf());
            }
        }
    }
    let fb = PathBuf::from(dequote(fallback_exe));
    fb.is_file().then_some(fb)
}

/// An absolute Linux path as a Wine path: Wine maps `Z:\` to the filesystem root,
/// so `/home/u/game.exe` → `Z:\home\u\game.exe`.
fn to_wine_path(p: &Path) -> String {
    format!("Z:{}", p.to_string_lossy().replace('/', "\\"))
}

fn dequote(s: &str) -> String {
    s.trim().trim_matches('"').to_string()
}

/// Join args into a single POSIX-shell command string (each arg single-quoted) for
/// `script -c`. Single-quoting keeps `Z:\…` backslashes literal and handles spaces.
fn shell_join(args: &[&str]) -> String {
    args.iter().map(|a| shell_quote(a)).collect::<Vec<_>>().join(" ")
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Find `chihuahua.exe`: the `MONADECK_CHIHUAHUA` env override first, then the
/// well-known spots (the user's clone dir, Downloads, monadeck's data dir).
pub fn detect_chihuahua() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MONADECK_CHIHUAHUA") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let h = home();
    [
        h.join("PROJECTS/chihuahua/chihuahua.exe"),
        h.join("Downloads/chihuahua/chihuahua.exe"),
        monadeck_data_dir().join("chihuahua/chihuahua.exe"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// The chihuahua release ships a single `chihuahua.zip` asset; the `latest/download`
/// URL always resolves to the newest one (no GitHub API call needed).
const CHIHUAHUA_ZIP_URL: &str =
    "https://github.com/keton/chihuahua/releases/latest/download/chihuahua.zip";

/// Locate chihuahua, or download + unzip the latest release into the data dir on
/// first run. **Blocking** (shells out to `curl` + `unzip`, like `core::installer`)
/// — call off the render thread. Errors (offline, no curl/unzip, asset moved) are
/// returned so the caller can log them; the launch path still degrades gracefully.
pub fn ensure_chihuahua() -> std::io::Result<PathBuf> {
    if let Some(p) = detect_chihuahua() {
        return Ok(p);
    }
    download_chihuahua(&monadeck_data_dir().join("chihuahua"))
}

fn download_chihuahua(dir: &Path) -> std::io::Result<PathBuf> {
    let err = |m: &str| std::io::Error::new(std::io::ErrorKind::Other, m.to_string());
    fs::create_dir_all(dir)?;
    let zip = dir.join("chihuahua.zip");
    log::info!("downloading chihuahua injector from {CHIHUAHUA_ZIP_URL}");
    let ok = Command::new("curl")
        .args(["-fsSL", "--retry", "2", "-o"])
        .arg(&zip)
        .arg(CHIHUAHUA_ZIP_URL)
        .status()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, format!("curl unavailable: {e}")))?
        .success();
    if !ok {
        return Err(err("failed to download chihuahua (curl)"));
    }
    let ok = Command::new("unzip")
        .args(["-o", "-q"])
        .arg(&zip)
        .arg("-d")
        .arg(dir)
        .status()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, format!("unzip unavailable: {e}")))?
        .success();
    let _ = fs::remove_file(&zip);
    if !ok {
        return Err(err("failed to unzip chihuahua"));
    }
    let direct = dir.join("chihuahua.exe");
    if direct.is_file() {
        log::info!("chihuahua injector ready at {}", direct.display());
        return Ok(direct);
    }
    // Archive nested the exe in a subdir — find it.
    walkdir::WalkDir::new(dir)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().eq_ignore_ascii_case("chihuahua.exe"))
        .map(|e| e.path().to_path_buf())
        .ok_or_else(|| err("chihuahua.exe not found in downloaded archive"))
}

/// Is `protontricks-launch` on `PATH`? UEVR launching needs it; the overlay hides
/// the VR-Mod toggle entirely when it's absent.
pub fn protontricks_available() -> bool {
    which_on_path("protontricks-launch")
}

fn which_on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wine_path_maps_root_to_z() {
        let p = PathBuf::from("/home/eidenz/Games/HB/HB-Win64-Shipping.exe");
        assert_eq!(to_wine_path(&p), r"Z:\home\eidenz\Games\HB\HB-Win64-Shipping.exe");
    }

    #[test]
    fn dequote_strips_quotes_and_space() {
        assert_eq!(dequote(" \"/a/b.exe\" "), "/a/b.exe");
    }

    #[test]
    fn shell_quote_keeps_backslashes_and_escapes_quotes() {
        assert_eq!(shell_quote(r"Z:\a\b.exe"), r"'Z:\a\b.exe'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(
            shell_join(&["protontricks-launch", r"Z:\g\x.exe"]),
            r"'protontricks-launch' 'Z:\g\x.exe'"
        );
    }
}
