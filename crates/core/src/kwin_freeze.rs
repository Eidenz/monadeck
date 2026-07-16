//! Recover the desktop from the kwin "cold-start HMD adoption" freeze.
//!
//! Some headsets (seen on the Bigscreen Beyond) serve a corrupted EDID when
//! their display is woken from a cold start. The corrupt read loses the
//! DisplayID "non-desktop / VR HMD" flag, so kwin adopts the headset as a
//! regular monitor, withdraws it from monado's DRM lease, and then retries a
//! failing atomic modeset ~56×/s forever — which blocks presentation on every
//! output: the whole desktop visually freezes until the headset is unplugged.
//!
//! The failure is only reachable in the seconds after monado powers the panel,
//! so this watchdog runs for a short window around service start: it snapshots
//! kwin's desktop outputs, follows kwin's journal for the commit-failure spam,
//! and on detection disables whichever output(s) newly appeared (the adopted
//! HMD) via `kscreen-doctor`, unfreezing the desktop without a replug.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Serialize;

/// How long after service start the failure window stays open. The observed
/// freeze hits ~4s after launch; a slow lighthouse/device init can stretch the
/// display power-on, so keep a generous margin — the watcher is just two
/// blocked threads and exits on its own.
const WATCH_WINDOW: Duration = Duration::from_secs(60);

/// Trigger threshold: this many kwin commit-failure lines inside
/// [`SPAM_WINDOW`]. The freeze spams ~56/s, so this trips in well under a
/// second; an isolated failed commit (which can happen on a legit mode change)
/// never accumulates enough.
const SPAM_LINES: usize = 25;
const SPAM_WINDOW: Duration = Duration::from_secs(2);

const KWIN_SPAM_MARKER: &str = "Atomic modeset commit failed";

/// What the watchdog did, for surfacing in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct FreezeRecovery {
    /// Outputs we told kwin to drop (normally one: the adopted HMD connector,
    /// e.g. "DP-1"). Empty if the spam was detected but no new output could be
    /// identified — the desktop is likely still frozen and needs a replug.
    pub disabled_outputs: Vec<String>,
}

/// Watches for the freeze during one service launch. Owned by the app state;
/// `spawn` replaces any previous watch, `stop` tears it down early.
#[derive(Default)]
pub struct KwinFreezeWatch {
    stop: Arc<AtomicBool>,
    journal: Arc<Mutex<Option<Child>>>,
    result: Arc<Mutex<Option<FreezeRecovery>>>,
    threads: Vec<JoinHandle<()>>,
}

impl KwinFreezeWatch {
    /// Whether this session can hit (and we can fix) the kwin freeze at all:
    /// the detection string and the recovery tool are both kwin-specific.
    pub fn session_applicable() -> bool {
        let kde = std::env::var("XDG_CURRENT_DESKTOP")
            .map(|d| d.to_ascii_uppercase().contains("KDE"))
            .unwrap_or(false);
        let wayland = std::env::var("XDG_SESSION_TYPE")
            .map(|t| t.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false);
        kde && wayland
    }

    /// Start watching. Call right before spawning monado-service so the output
    /// baseline predates the headset display powering on. No-op outside a KDE
    /// Wayland session.
    pub fn spawn(&mut self) {
        self.stop_watch();
        if !Self::session_applicable() {
            return;
        }
        let baseline = match desktop_output_names() {
            Some(names) => names,
            None => {
                log::debug!("kwin freeze watch: kscreen-doctor unavailable, not watching");
                return;
            }
        };

        // Follow only new kwin lines; `-o cat` drops metadata so line matching
        // is trivial.
        let child = Command::new("journalctl")
            .args(["-f", "-n", "0", "-o", "cat", "-t", "kwin_wayland"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                log::debug!("kwin freeze watch: journalctl unavailable ({e}), not watching");
                return;
            }
        };
        let stdout = child.stdout.take().expect("stdout was piped");

        self.stop = Arc::new(AtomicBool::new(false));
        self.result = Arc::new(Mutex::new(None));
        *self.journal.lock().unwrap() = Some(child);

        // Reader: counts spam lines in a sliding window, fires recovery once.
        let result = self.result.clone();
        let stop = self.stop.clone();
        self.threads.push(std::thread::spawn(move || {
            let mut hits: Vec<Instant> = Vec::new();
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                if !line.contains(KWIN_SPAM_MARKER) {
                    continue;
                }
                let now = Instant::now();
                hits.push(now);
                hits.retain(|t| now.duration_since(*t) <= SPAM_WINDOW);
                if hits.len() >= SPAM_LINES {
                    log::warn!(
                        "kwin freeze watch: modeset-failure spam detected, \
                         disabling newly adopted output(s)"
                    );
                    *result.lock().unwrap() = Some(recover(&baseline));
                    break; // one-shot: job done for this launch
                }
            }
        }));

        // Deadline: close the window by killing journalctl, which EOFs the
        // reader. Also reacts to stop() within half a second.
        let journal = self.journal.clone();
        let stop = self.stop.clone();
        self.threads.push(std::thread::spawn(move || {
            let deadline = Instant::now() + WATCH_WINDOW;
            while Instant::now() < deadline && !stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(500));
            }
            if let Some(mut c) = journal.lock().unwrap().take() {
                let _ = c.kill();
                let _ = c.wait();
            }
        }));
    }

    /// Stop watching (service stopped, or a new watch replaces this one).
    pub fn stop_watch(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut c) = self.journal.lock().unwrap().take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }

    /// One-shot: returns the recovery event once, then clears it, so a status
    /// poll can hand it to the UI exactly one time.
    pub fn take_result(&self) -> Option<FreezeRecovery> {
        self.result.lock().unwrap().take()
    }
}

impl Drop for KwinFreezeWatch {
    fn drop(&mut self) {
        self.stop_watch();
    }
}

/// The desktop outputs kwin currently manages (a non-desktop HMD is correctly
/// absent from this list — it only appears when kwin wrongly adopts it).
/// `None` when kscreen-doctor is missing or its output can't be parsed.
fn desktop_output_names() -> Option<Vec<String>> {
    let out = Command::new("kscreen-doctor")
        .args(["-o", "--json"])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    // kscreen-doctor appends its human-readable listing after the JSON
    // document; parse just the first value.
    let raw = String::from_utf8_lossy(&out.stdout);
    let doc = serde_json::Deserializer::from_str(&raw)
        .into_iter::<serde_json::Value>()
        .next()?
        .ok()?;
    let names = doc
        .get("outputs")?
        .as_array()?
        .iter()
        .filter_map(|o| o.get("name")?.as_str().map(str::to_string))
        .collect();
    Some(names)
}

/// Disable every output kwin picked up since the baseline — during the freeze
/// that's the wrongly adopted HMD. kwin's commit loop stays live enough to
/// service the request, which is what unfreezes the desktop.
fn recover(baseline: &[String]) -> FreezeRecovery {
    let current = desktop_output_names().unwrap_or_default();
    let mut disabled = Vec::new();
    for name in current {
        if baseline.iter().any(|b| b == &name) {
            continue;
        }
        let ok = Command::new("kscreen-doctor")
            .arg(format!("output.{name}.disable"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            log::warn!("kwin freeze watch: disabled adopted output {name}");
            disabled.push(name);
        } else {
            log::warn!("kwin freeze watch: failed to disable output {name}");
        }
    }
    if disabled.is_empty() {
        log::warn!(
            "kwin freeze watch: spam detected but no new output found; \
             desktop may still be frozen (unplug the headset to recover)"
        );
    }
    FreezeRecovery {
        disabled_outputs: disabled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end detection: emit the real kwin spam line into the journal
    /// under kwin_wayland's tag and check the watch trips. Recovery is a no-op
    /// here (no output appears between baseline and trigger), so nothing on
    /// the desktop is touched. Skips silently outside a KDE Wayland session
    /// (CI) where the watch intentionally never arms.
    #[test]
    fn detects_journal_spam() {
        if !KwinFreezeWatch::session_applicable() || desktop_output_names().is_none() {
            return;
        }
        let mut watch = KwinFreezeWatch::default();
        watch.spawn();
        // Give journalctl -f a moment to start following before we emit.
        std::thread::sleep(Duration::from_millis(800));
        for _ in 0..SPAM_LINES + 5 {
            let _ = Command::new("logger")
                .args(["-t", "kwin_wayland", "Atomic modeset commit failed! Invalid argument"])
                .status();
        }
        // Journald delivery + the follow pipe aren't instant; poll briefly.
        let mut result = None;
        for _ in 0..40 {
            result = watch.take_result();
            if result.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        watch.stop_watch();
        let recovery = result.expect("spam should have triggered the watch");
        assert!(
            recovery.disabled_outputs.is_empty(),
            "no output changed during the test, none should be disabled"
        );
    }

    #[test]
    fn output_names_parse_when_available() {
        // On a machine with kscreen-doctor this exercises the trailing-garbage
        // JSON parse; elsewhere it just confirms the graceful None.
        if let Some(names) = desktop_output_names() {
            assert!(!names.is_empty());
        }
    }
}
