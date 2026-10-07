//! The desktop's audio devices, and the headset's taking over while VR runs.
//!
//! Everything goes through `pactl` (PipeWire's pulse server, or PulseAudio):
//! its JSON listing for the device names, `get-default-*` / `set-default-*`
//! for the switch. Streams that follow the default (most of them) move along.
//!
//! A switch remembers what the default was, and [`restore`] puts it back when
//! VR stops, unless you picked another device yourself in the meantime.

use serde::{Deserialize, Serialize};

/// The output and microphone WiVRn adds while a headset is connected.
pub const WIVRN_OUTPUT: &str = "wivrn.sink";
pub const WIVRN_INPUT: &str = "wivrn.source";

/// An output (sink) or a microphone (source), by its stable node name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDevice {
    /// `alsa_output.usb-…`: survives reboots and replugging.
    pub name: String,
    /// What the desktop calls it ("Beyond Digital Stereo (IEC958)").
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AudioDevices {
    /// `pactl` answered: the switch can work.
    pub available: bool,
    pub outputs: Vec<AudioDevice>,
    pub inputs: Vec<AudioDevice>,
    pub default_output: Option<String>,
    pub default_input: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Output,
    Input,
}

impl Kind {
    fn noun(self) -> &'static str {
        match self {
            Kind::Output => "sink",
            Kind::Input => "source",
        }
    }
}

fn pactl(args: &[&str]) -> Option<String> {
    let out = crate::host::command("pactl").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The devices of one kind, from `pactl --format=json list sinks|sources`.
/// Monitors (an output's loopback, listed as a source) aren't microphones.
fn parse_list(json: &str) -> Vec<AudioDevice> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|d| {
            let name = d.get("name")?.as_str()?.to_string();
            let class = d.pointer("/properties/device.class").and_then(|v| v.as_str());
            if class == Some("monitor") || name.ends_with(".monitor") {
                return None;
            }
            let description = d.get("description").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or(&name).to_string();
            Some(AudioDevice { name, description })
        })
        .collect()
}

fn devices(kind: Kind) -> Option<Vec<AudioDevice>> {
    pactl(&["--format=json", "list", &format!("{}s", kind.noun())]).map(|j| parse_list(&j))
}

/// The current default device of a kind.
pub fn default(kind: Kind) -> Option<String> {
    pactl(&[&format!("get-default-{}", kind.noun())]).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn set_default(kind: Kind, name: &str) -> bool {
    pactl(&[&format!("set-default-{}", kind.noun()), name]).is_some()
}

/// Whether device `name` is plugged in (or, for WiVRn's, connected).
pub fn present(kind: Kind, name: &str) -> bool {
    devices(kind).is_some_and(|d| d.iter().any(|x| x.name == name))
}

/// Every output and microphone, and the current defaults.
pub fn list() -> AudioDevices {
    let (Some(outputs), Some(inputs)) = (devices(Kind::Output), devices(Kind::Input)) else {
        return AudioDevices::default();
    };
    AudioDevices {
        available: true,
        outputs,
        inputs,
        default_output: default(Kind::Output),
        default_input: default(Kind::Input),
    }
}

/// One switch made for VR: what was the default before, and ours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Swap {
    pub previous: String,
    pub ours: String,
}

/// What VR switched, to hand back when it stops.
#[derive(Debug, Clone, Default)]
pub struct Switched {
    pub output: Option<Swap>,
    pub input: Option<Swap>,
}

impl Switched {
    fn slot(&mut self, kind: Kind) -> &mut Option<Swap> {
        match kind {
            Kind::Output => &mut self.output,
            Kind::Input => &mut self.input,
        }
    }
}

/// Make `want` the default `kind` if it's plugged in. Returns whether that's
/// settled (switched now, already the default, or nothing wanted); false
/// while the device hasn't shown up yet (a headset brings its own a moment
/// after VR starts it).
pub fn switch(kind: Kind, want: Option<&str>, done: &mut Switched) -> bool {
    let Some(want) = want else { return true };
    if !present(kind, want) {
        return false;
    }
    let current = default(kind);
    if current.as_deref() == Some(want) {
        return true;
    }
    if !set_default(kind, want) {
        log::warn!("audio: couldn't make {want} the default {}", kind.noun());
        return false;
    }
    log::info!("audio: default {} is now {want} (was {})", kind.noun(), current.as_deref().unwrap_or("none"));
    // A restart in between keeps the first "before": that's what to go back to.
    let slot = done.slot(kind);
    match (slot.as_mut(), current) {
        (Some(swap), _) => swap.ours = want.to_string(),
        (None, Some(previous)) => *slot = Some(Swap { previous, ours: want.to_string() }),
        (None, None) => {}
    }
    true
}

/// Put back what [`switch`] replaced. Left alone where you picked another
/// device yourself while VR ran; a headset device that went away with VR
/// counts as still ours.
pub fn restore(done: &mut Switched) {
    for kind in [Kind::Output, Kind::Input] {
        let Some(swap) = done.slot(kind).take() else { continue };
        let current = default(kind);
        let still_ours = current.as_deref() == Some(swap.ours.as_str()) || !present(kind, &swap.ours);
        if !still_ours {
            log::info!("audio: default {} changed during VR ({}), leaving it", kind.noun(), current.unwrap_or_default());
            continue;
        }
        if !present(kind, &swap.previous) {
            log::info!("audio: {} is gone, nothing to go back to", swap.previous);
            continue;
        }
        if set_default(kind, &swap.previous) {
            log::info!("audio: default {} back to {}", kind.noun(), swap.previous);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitors_are_not_microphones() {
        let json = r#"[
            {"name":"alsa_output.usb-X.analog-stereo.monitor","description":"Monitor of X","properties":{"device.class":"monitor"}},
            {"name":"alsa_input.usb-Bigscreen_Beyond-00.iec958-stereo","description":"Beyond Digital Stereo (IEC958)","properties":{"device.class":"sound"}},
            {"name":"odd","description":"","properties":{}}
        ]"#;
        let d = parse_list(json);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].description, "Beyond Digital Stereo (IEC958)");
        // No description: the name stands in.
        assert_eq!(d[1].description, "odd");
        assert!(parse_list("not json").is_empty());
    }
}
