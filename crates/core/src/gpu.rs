//! AMD GPU "VR" power-profile control.
//!
//! Writing `4` to a card's `pp_power_profile_mode` selects the VR profile, which
//! pins clocks for VR and cuts frame-time jitter (Envision exposes the same
//! thing). Needs root, so we apply it via `pkexec`; it resets on reboot.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// AMD VR profile is index 4 in `pp_power_profile_mode`.
const VR_INDEX: &str = "4";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmdGpu {
    /// e.g. `card1`.
    pub card: String,
    /// Absolute path to `pp_power_profile_mode`.
    pub profile_path: String,
    /// Currently active mode name (e.g. `BOOTUP_DEFAULT`, `VR`).
    pub current_mode: String,
    /// Whether the VR profile is the active one.
    pub vr_active: bool,
}

fn is_amd(device_dir: &Path) -> bool {
    fs::read_to_string(device_dir.join("vendor"))
        .map(|v| v.trim() == "0x1002")
        .unwrap_or(false)
}

/// Parse the active mode from `pp_power_profile_mode`. The active row carries a
/// `*` right after its name; the row's first token is the index.
fn parse_active(content: &str) -> (String, bool) {
    for line in content.lines() {
        if !line.contains('*') {
            continue;
        }
        let mut tokens = line.split_whitespace();
        let index = tokens.next().unwrap_or("").trim_end_matches('*');
        let name = tokens
            .next()
            .unwrap_or("")
            .trim_end_matches([':', '*'])
            .to_string();
        let vr = index == VR_INDEX || name.eq_ignore_ascii_case("VR");
        return (name, vr);
    }
    (String::new(), false)
}

/// The first AMD card exposing a power-profile control, if any.
pub fn find_amd_gpu() -> Option<AmdGpu> {
    let mut cards: Vec<PathBuf> = fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("card") && n[4..].chars().all(|c| c.is_ascii_digit()))
                .unwrap_or(false)
        })
        .collect();
    cards.sort();

    for card in cards {
        let device = card.join("device");
        let profile = device.join("pp_power_profile_mode");
        if !is_amd(&device) || !profile.is_file() {
            continue;
        }
        let content = fs::read_to_string(&profile).unwrap_or_default();
        let (current_mode, vr_active) = parse_active(&content);
        return Some(AmdGpu {
            card: card
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("card")
                .to_string(),
            profile_path: profile.to_string_lossy().to_string(),
            current_mode,
            vr_active,
        });
    }
    None
}

/// Set the VR power profile on the detected AMD GPU via `pkexec`. Blocking.
pub fn set_vr_profile() -> Result<()> {
    let gpu = find_amd_gpu().context("no AMD GPU with a power-profile control found")?;
    let status = std::process::Command::new("pkexec")
        .args(["sh", "-c", &format!("echo {VR_INDEX} > {}", gpu.profile_path)])
        .status()
        .context("failed to launch pkexec")?;
    if !status.success() {
        bail!("pkexec exited with {status}");
    }
    Ok(())
}
