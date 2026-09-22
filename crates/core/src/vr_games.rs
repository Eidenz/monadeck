//! Which library games are VR, and keeping gaming mode's virtual gamepad away
//! from them.
//!
//! The pad gaming mode creates is a system-wide uinput device, so a VR game
//! running behind your flat game reads it too (VRChat toggles its mic and
//! opens menus off gamepad buttons). Steam's `steam://` launch URLs can't carry
//! environment variables and Steam owns its launch-options file while it runs,
//! so the per-game hook we use is a **protonfixes local fix**
//! (`~/.config/protonfixes/localfixes/<appid>.py`, honoured by GE-Proton and
//! its derivatives): it sets SDL's ignore / blacklist variables for that one
//! game, whichever way it is launched. One small marked file per game; deleting
//! it undoes it.
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::paths::{config_home, home, monadeck_config_dir};

/// The virtual pad's USB id (a wired Xbox 360 pad), as SDL's hints spell it.
pub const PAD_DEVICE: &str = "0x045e/0x028e";
/// Marks a local fix as ours (never touch a file without it).
const MARKER: &str = "monadeck:hide-virtual-pad";

/// VR titles we know by Steam appid — the floor under the dynamic signals
/// (launch options, the shortcut VR flag, and games seen as XR clients).
const KNOWN_VR: &[&str] = &[
    "438100",  // VRChat
    "2519830", // Resonite
    "661130",  // ChilloutVR
    "740250",  // Neos VR
    "546560",  // Half-Life: Alyx
    "620980",  // Beat Saber
    "629730",  // Blade & Sorcery
    "555160",  // Pavlov VR
    "823500",  // BONEWORKS
    "1592190", // BONELAB
    "450390",  // The Lab
    "617830",  // SUPERHOT VR
    "1079800", // Pistol Whip
    "1533390", // Gorilla Tag
    "1012790", // Into the Radius
    "450540",  // H3VR
    "667970",  // VTOL VR
    "611670",  // Skyrim VR
    "611660",  // Fallout 4 VR
    "250820",  // SteamVR
];

pub fn is_known_vr(app_id: &str) -> bool {
    KNOWN_VR.contains(&app_id)
}

/// Do these Steam launch options wire the game to a VR runtime? (`VR_OVERRIDE`
/// for xrizer, the pressure-vessel OpenXR import, an explicit runtime json, or
/// the usual engine switches — but not `-vrmode none`, which turns VR off.)
pub fn launch_options_look_vr(opts: &str) -> bool {
    let o = opts.to_lowercase();
    if o.contains("vr_override=") || o.contains("pressure_vessel_import_openxr") || o.contains("xr_runtime_json=") {
        return true;
    }
    let words: Vec<&str> = o.split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        match *w {
            "-vr" | "-openvr" | "-openxr" | "-steamvr" | "--vr" | "-hmd" => return true,
            "-vrmode" => {
                if words.get(i + 1).is_some_and(|m| *m != "none") {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Do these launch options already keep the virtual pad out (the SDL ignore
/// variable naming its USB id)? Then the game needs nothing from us.
pub fn launch_options_hide_pad(opts: &str) -> bool {
    let o = opts.to_lowercase();
    o.contains("sdl_gamecontroller_ignore_devices") && o.contains(PAD_DEVICE)
}

/// Does the Proton that set this prefix up read protonfixes local fixes?
/// GE-Proton and its derivatives stamp a name ("GE-Proton11-7",
/// "dwproton-10.0-26"); Valve's Proton stamps a bare version ("11.0-100") and
/// has no protonfixes at all.
pub fn proton_reads_local_fixes(prefix_version: &str) -> bool {
    !prefix_version.trim().chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// How (or whether) a VR game is kept away from the virtual pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadStatus {
    /// Its Steam launch options already do it.
    LaunchOptions,
    /// Our protonfixes local fix does it (GE-style Proton).
    LocalFix,
    /// Valve's Proton ignores local fixes: only launch options can.
    NeedsLaunchOption,
    /// Never launched (no prefix yet): can't tell which Proton it uses.
    Unknown,
}

pub fn pad_status(hidden_by_launch_options: bool, prefix_version: Option<&str>) -> PadStatus {
    if hidden_by_launch_options {
        return PadStatus::LaunchOptions;
    }
    match prefix_version {
        None => PadStatus::Unknown,
        Some(v) if proton_reads_local_fixes(v) => PadStatus::LocalFix,
        Some(_) => PadStatus::NeedsLaunchOption,
    }
}

/// The variables to add to a game's launch options (before `%command%`).
pub fn pad_launch_option_vars() -> String {
    format!("SDL_GAMECONTROLLER_IGNORE_DEVICES={PAD_DEVICE} SDL_JOYSTICK_BLACKLIST_DEVICES={PAD_DEVICE}")
}

fn learned_path() -> PathBuf {
    monadeck_config_dir().join("vr_games.json")
}

/// Games the overlay has seen running as XR clients (by Steam / shortcut id).
pub fn load_learned() -> HashSet<String> {
    fs::read_to_string(learned_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save_learned(set: &HashSet<String>) {
    let mut ids: Vec<&String> = set.iter().collect();
    ids.sort();
    if let Ok(json) = serde_json::to_string_pretty(&ids) {
        let _ = fs::create_dir_all(monadeck_config_dir());
        let _ = fs::write(learned_path(), json);
    }
}

fn localfixes_dir() -> PathBuf {
    config_home().join("protonfixes").join("localfixes")
}

fn fix_path(id: &str) -> PathBuf {
    localfixes_dir().join(format!("{id}.py"))
}

fn is_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())
}

/// Is any installed Proton build protonfixes-capable (GE-Proton & friends)?
/// Without one the local fix is never read, and only launch options help.
pub fn protonfixes_available() -> bool {
    let tools = [
        home().join(".local/share/Steam/compatibilitytools.d"),
        home().join(".steam/root/compatibilitytools.d"),
        home().join(".steam/steam/compatibilitytools.d"),
        PathBuf::from("/usr/share/steam/compatibilitytools.d"),
    ];
    tools.iter().filter_map(|d| fs::read_dir(d).ok()).flatten().flatten().any(|e| e.path().join("protonfixes").is_dir())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PadFix {
    /// Written just now: takes effect the next time the game starts.
    Installed,
    AlreadyThere,
    /// The game has a local fix of its own; left untouched.
    Foreign,
    Failed(String),
}

fn fix_source(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control() && *c != '"' && *c != '\\').collect();
    format!(
        r#""""{name}: keep Monadeck's virtual gamepad out of this VR game.

{MARKER}
Gaming mode turns the VR controllers into an Xbox pad for the flat game on your
screen. That pad is a system-wide device, so this game (running behind) would
read it as a gamepad too. Delete this file to undo; Monadeck rewrites it while
"Hide the pad from VR games" is on. Note: a real wired Xbox 360 pad shares the
same USB id and is hidden from this game as well.
"""
import os

from protonfixes import util

PAD = '{PAD_DEVICE}'


def main() -> None:
    for var in ('SDL_GAMECONTROLLER_IGNORE_DEVICES', 'SDL_JOYSTICK_BLACKLIST_DEVICES'):
        have = [v.strip() for v in os.environ.get(var, '').split(',') if v.strip()]
        if PAD not in have:
            have.append(PAD)
        util.set_environment(var, ','.join(have))
"#
    )
}

pub fn pad_fix_installed(id: &str) -> bool {
    fs::read_to_string(fix_path(id)).is_ok_and(|s| s.contains(MARKER))
}

/// Make this game ignore the virtual pad from its next launch on.
pub fn install_pad_fix(id: &str, name: &str) -> PadFix {
    if !is_id(id) {
        return PadFix::Failed(format!("bad game id {id:?}"));
    }
    let path = fix_path(id);
    if let Ok(existing) = fs::read_to_string(&path) {
        return if existing.contains(MARKER) { PadFix::AlreadyThere } else { PadFix::Foreign };
    }
    if let Err(e) = fs::create_dir_all(localfixes_dir()) {
        return PadFix::Failed(e.to_string());
    }
    match fs::write(&path, fix_source(name)) {
        Ok(()) => PadFix::Installed,
        Err(e) => PadFix::Failed(e.to_string()),
    }
}

/// Remove every local fix we wrote (the setting was turned off). Returns how many.
pub fn remove_all_pad_fixes() -> usize {
    let Ok(entries) = fs::read_dir(localfixes_dir()) else { return 0 };
    let mut n = 0;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "py") && fs::read_to_string(&p).is_ok_and(|s| s.contains(MARKER)) && fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_options_vr_signals() {
        assert!(launch_options_look_vr("VR_OVERRIDE=/home/u/.local/share/xrizer/xrizer-nightly PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES=1 %command% -vrmode OpenVR"));
        assert!(launch_options_look_vr("%command% -vrmode openvr"));
        assert!(launch_options_look_vr("gamemoderun %command% -vr"));
        assert!(!launch_options_look_vr("-vrmode None"));
        assert!(!launch_options_look_vr("gamemoderun %command% -novid"));
        assert!(!launch_options_look_vr(""));
    }

    #[test]
    fn pad_status_by_proton_family() {
        assert!(proton_reads_local_fixes("GE-Proton11-7"));
        assert!(proton_reads_local_fixes("dwproton-10.0-26"));
        assert!(!proton_reads_local_fixes("11.0-100"));
        assert_eq!(pad_status(false, Some("GE-Proton11-7")), PadStatus::LocalFix);
        assert_eq!(pad_status(false, Some("11.0-100")), PadStatus::NeedsLaunchOption);
        assert_eq!(pad_status(false, None), PadStatus::Unknown);
        let opts = "VR_OVERRIDE=/x SDL_GAMECONTROLLER_IGNORE_DEVICES=0x045e/0x028e %command%";
        assert!(launch_options_hide_pad(opts));
        assert!(!launch_options_hide_pad("SDL_GAMECONTROLLER_IGNORE_DEVICES=0x1234/0x5678 %command%"));
        assert_eq!(pad_status(launch_options_hide_pad(opts), Some("11.0-100")), PadStatus::LaunchOptions);
        assert!(launch_options_hide_pad(&format!("{} %command%", pad_launch_option_vars())));
    }

    #[test]
    fn known_list_and_ids() {
        assert!(is_known_vr("438100"));
        assert!(!is_known_vr("730"));
        assert!(is_id("3968468407"));
        assert!(!is_id("../evil"));
        assert!(!is_id(""));
    }

    /// The file we drop into protonfixes' folder must be valid Python and do
    /// what it says when protonfixes calls `main()`: run it against a stub
    /// `protonfixes.util` (skipped when python3 isn't around).
    #[test]
    fn generated_fix_runs_and_merges_existing_values() {
        let dir = std::env::temp_dir().join(format!("monadeck-fix-test-{}", std::process::id()));
        let pkg = dir.join("protonfixes");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("__init__.py"), "").unwrap();
        fs::write(pkg.join("util.py"), "import os\ndef set_environment(k, v):\n    os.environ[k] = v\n").unwrap();
        fs::write(dir.join("fix438100.py"), fix_source("VRChat")).unwrap();
        let script = "import os, fix438100\nfix438100.main()\nprint(os.environ['SDL_GAMECONTROLLER_IGNORE_DEVICES'] + '|' + os.environ['SDL_JOYSTICK_BLACKLIST_DEVICES'])";
        let out = std::process::Command::new("python3")
            .args(["-c", script])
            .current_dir(&dir)
            .env("PYTHONPATH", &dir)
            .env("SDL_GAMECONTROLLER_IGNORE_DEVICES", "0x1234/0x5678")
            .env_remove("SDL_JOYSTICK_BLACKLIST_DEVICES")
            .output();
        let _ = fs::remove_dir_all(&dir);
        let Ok(out) = out else { return }; // no python3 here
        assert!(out.status.success(), "python failed: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0x1234/0x5678,0x045e/0x028e|0x045e/0x028e");
    }

    #[test]
    fn fix_source_is_marked_python_with_a_clean_name() {
        let src = fix_source("Va\"M \\ VR\n");
        assert!(src.contains(MARKER));
        assert!(src.contains("def main() -> None:"));
        assert!(src.contains(PAD_DEVICE));
        assert!(src.starts_with("\"\"\"VaM  VR:"));
    }
}
