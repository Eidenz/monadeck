//! Following a game we asked Steam to launch: did it start, did it make it,
//! did it die on the way — and was that death a Proton prefix upgrade (the
//! classic "first launch after a Proton update fails, the second works")?
//!
//! Steam spawns every game under `reaper SteamLaunch AppId=<id> -- …`, and that
//! process lives exactly as long as the game. So a scan of `/proc` for that
//! argument pair tells us the game is up, with no dependency on Steam's log
//! format. The scan runs on its own thread (a few hundred small reads a second
//! don't belong on the render thread).
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A flat game whose process has lived this long counts as launched.
const FLAT_SETTLE: Duration = Duration::from_secs(12);
/// A VR game normally counts once it shows up as an XR client; if that never
/// matches (names differ), a process this old is clearly up too.
const VR_SETTLE: Duration = Duration::from_secs(45);
/// An exit later than this after launch isn't a launch failure any more.
const LAUNCH_WINDOW: Duration = Duration::from_secs(180);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcState {
    /// Not scanned yet for this target.
    Unknown,
    Alive,
    /// Missing on two scans in a row.
    Gone,
}

/// Watches `/proc` for one Steam AppId's reaper process.
pub struct ProcWatch {
    target: Arc<Mutex<Option<String>>>,
    state: Arc<AtomicU8>,
    stop: Arc<AtomicBool>,
}

fn cmdline_is_launch(cmdline: &[u8], id: &str) -> bool {
    let want = format!("AppId={id}");
    let mut prev_was_steamlaunch = false;
    for arg in cmdline.split(|b| *b == 0) {
        if prev_was_steamlaunch && arg == want.as_bytes() {
            return true;
        }
        prev_was_steamlaunch = arg == b"SteamLaunch";
    }
    false
}

fn scan(id: &str) -> bool {
    let Ok(dir) = std::fs::read_dir("/proc") else { return false };
    for e in dir.flatten() {
        let name = e.file_name();
        if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let Ok(cmd) = std::fs::read(e.path().join("cmdline")) {
            if cmd.len() > 24 && cmdline_is_launch(&cmd, id) {
                return true;
            }
        }
    }
    false
}

impl ProcWatch {
    pub fn new() -> Self {
        let target: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let state = Arc::new(AtomicU8::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (t, s, st) = (target.clone(), state.clone(), stop.clone());
        std::thread::Builder::new()
            .name("launch-watch".into())
            .spawn(move || {
                let mut current: Option<String> = None;
                let mut misses = 0u8;
                while !st.load(Ordering::Relaxed) {
                    let want = t.lock().map(|g| g.clone()).unwrap_or(None);
                    if want != current {
                        current = want;
                        misses = 0;
                        s.store(0, Ordering::Relaxed);
                    }
                    match &current {
                        Some(id) => {
                            if scan(id) {
                                misses = 0;
                                s.store(1, Ordering::Relaxed);
                            } else {
                                misses = misses.saturating_add(1);
                                if misses >= 2 {
                                    s.store(2, Ordering::Relaxed);
                                }
                            }
                            std::thread::sleep(Duration::from_millis(1000));
                        }
                        None => std::thread::sleep(Duration::from_millis(400)),
                    }
                }
            })
            .ok();
        Self { target, state, stop }
    }

    /// Start (or stop, with `None`) watching an AppId.
    pub fn watch(&self, id: Option<String>) {
        if let Ok(mut t) = self.target.lock() {
            if *t != id {
                *t = id;
                self.state.store(0, Ordering::Relaxed);
            }
        }
    }

    pub fn state(&self) -> ProcState {
        match self.state.load(Ordering::Relaxed) {
            1 => ProcState::Alive,
            2 => ProcState::Gone,
            _ => ProcState::Unknown,
        }
    }
}

impl Drop for ProcWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Pending,
    /// It's up: an XR client appeared, or the process has settled.
    Launched,
    /// The process came and went before it counted as launched.
    /// `upgraded`: the Proton prefix version changed during that run.
    Exited { upgraded: bool, after: Duration },
}

/// One launch being followed.
pub struct Tracked {
    pub name: String,
    /// Steam AppId (app or non-Steam shortcut): reaper's `AppId=`, the
    /// compatdata folder, the protonfixes id.
    pub id: String,
    /// The `steam://rungameid/` value, to launch it again.
    pub game_id: String,
    pub vr: bool,
    pub uevr: bool,
    pub retried: bool,
    started: Instant,
    seen: Option<Instant>,
    prefix_before: Option<String>,
}

impl Tracked {
    pub fn new(name: String, id: String, game_id: String, vr: bool, uevr: bool, prefix_before: Option<String>) -> Self {
        Self { name, id, game_id, vr, uevr, retried: false, started: Instant::now(), seen: None, prefix_before }
    }

    /// The process has been seen at least once.
    pub fn process_seen(&self) -> bool {
        self.seen.is_some()
    }

    /// Too old to still be "launching" (drop the tracker).
    pub fn expired(&self, now: Instant) -> bool {
        now.duration_since(self.started) > LAUNCH_WINDOW
    }

    /// We launched it a second time: start over, once.
    pub fn restart(&mut self, prefix_now: Option<String>) {
        self.retried = true;
        self.started = Instant::now();
        self.seen = None;
        self.prefix_before = prefix_now;
    }

    /// `prefix_now` is only asked for when the game has exited.
    pub fn poll(&mut self, proc_state: ProcState, xr_appeared: bool, now: Instant, prefix_now: impl FnOnce() -> Option<String>) -> Outcome {
        if xr_appeared {
            return Outcome::Launched;
        }
        match proc_state {
            ProcState::Alive => {
                let seen = *self.seen.get_or_insert(now);
                let settle = if self.vr { VR_SETTLE } else { FLAT_SETTLE };
                if now.duration_since(seen) >= settle {
                    Outcome::Launched
                } else {
                    Outcome::Pending
                }
            }
            ProcState::Gone if self.seen.is_some() => {
                let after = now.duration_since(self.started);
                let upgraded = prefix_now() != self.prefix_before;
                Outcome::Exited { upgraded, after }
            }
            _ => Outcome::Pending,
        }
    }

    /// An exit worth one automatic relaunch: Proton rewrote the prefix during
    /// the run (or created it), it died inside the launch window, and we
    /// haven't retried yet. UEVR launches go through the injector: not retried.
    pub fn should_retry(&self, outcome: &Outcome) -> bool {
        matches!(outcome, Outcome::Exited { upgraded: true, after } if *after <= LAUNCH_WINDOW) && !self.retried && !self.uevr
    }
}

/// `monadeck-overlay --launch-selftest`: read-only report of what 1.6's launch
/// handling knows — which library games count as VR and why, whether their pad
/// fix is in place, whether a protonfixes-capable Proton exists — plus a live
/// check of the process watch against a stand-in reaper process.
pub fn selftest() -> anyhow::Result<()> {
    use monadeck_core::{steam, vr_games};
    println!("protonfixes-capable Proton installed: {}", vr_games::protonfixes_available());
    let learned = vr_games::load_learned();
    println!("learned VR games (seen as XR clients): {}", learned.len());
    let games = steam::scan_library();
    let mut n = 0;
    let mut tally: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for g in &games {
        let Some(id) = g.app_id.clone().or_else(|| g.shortcut_id.clone()) else { continue };
        let known = vr_games::is_known_vr(&id);
        let learnt = learned.contains(&id);
        if !(g.vr_hint || learnt) {
            continue;
        }
        n += 1;
        let why = if known { "known title" } else if learnt && !g.vr_hint { "seen as XR client" } else { "launch options / VR flag" };
        let prefix = steam::prefix_version(&id);
        let status = match vr_games::pad_status(g.pad_hidden_by_options, prefix.as_deref()) {
            vr_games::PadStatus::LaunchOptions => "launch options hide it",
            vr_games::PadStatus::LocalFix if vr_games::pad_fix_installed(&id) => "local fix installed",
            vr_games::PadStatus::LocalFix => "local fix (not written yet)",
            vr_games::PadStatus::NeedsLaunchOption => "NEEDS LAUNCH OPTION (Valve Proton)",
            vr_games::PadStatus::Unknown => "never launched",
        };
        *tally.entry(status).or_insert(0usize) += 1;
        println!(
            "  VR  {:<34} {:>11}  {:<24} {:<18} pad: {status}",
            g.name.chars().take(34).collect::<String>(),
            id,
            why,
            prefix.unwrap_or_else(|| "-".into()),
        );
    }
    println!("{n} of {} library games detected as VR", games.len());
    for (status, count) in &tally {
        println!("  {count:>3}  {status}");
    }

    // Live watch: a stand-in for `reaper SteamLaunch AppId=<id> -- …`.
    let id = "4242424242";
    let mut child = std::process::Command::new("sh").args(["-c", "sleep 3; true", "sh", "SteamLaunch", &format!("AppId={id}")]).spawn()?;
    let watch = ProcWatch::new();
    watch.watch(Some(id.into()));
    let t0 = Instant::now();
    let mut saw_alive = None;
    let mut saw_gone = None;
    while t0.elapsed() < Duration::from_secs(9) && saw_gone.is_none() {
        match watch.state() {
            ProcState::Alive if saw_alive.is_none() => saw_alive = Some(t0.elapsed()),
            ProcState::Gone if saw_alive.is_some() => saw_gone = Some(t0.elapsed()),
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.wait();
    println!("process watch: alive after {:?}, gone after {:?} (the stand-in lived 3 s)", saw_alive, saw_gone);
    if saw_alive.is_none() || saw_gone.is_none() {
        anyhow::bail!("the /proc watch did not follow the stand-in process");
    }
    println!("launch selftest OK");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracked(vr: bool) -> Tracked {
        Tracked::new("VaM VR".into(), "3968468407".into(), "17044442023307771904".into(), vr, false, Some("GE-Proton11-1".into()))
    }

    #[test]
    fn reaper_cmdline_matches_only_its_own_appid() {
        let cmd = b"/home/u/.local/share/Steam/ubuntu12_32/reaper\0SteamLaunch\0AppId=3968468407\0--\0/x/_v2-entry-point\0";
        assert!(cmdline_is_launch(cmd, "3968468407"));
        assert!(!cmdline_is_launch(cmd, "396846840"));
        assert!(!cmdline_is_launch(cmd, "438100"));
        // The id elsewhere on a command line isn't a launch.
        assert!(!cmdline_is_launch(b"cat\0AppId=438100\0", "438100"));
    }

    #[test]
    fn prefix_upgrade_death_retries_once() {
        let mut t = tracked(true);
        let t0 = Instant::now();
        assert_eq!(t.poll(ProcState::Unknown, false, t0, || None), Outcome::Pending);
        assert_eq!(t.poll(ProcState::Alive, false, t0 + Duration::from_secs(2), || None), Outcome::Pending);
        let out = t.poll(ProcState::Gone, false, t0 + Duration::from_secs(51), || Some("GE-Proton11-7".into()));
        assert!(matches!(out, Outcome::Exited { upgraded: true, .. }));
        assert!(t.should_retry(&out));
        t.restart(Some("GE-Proton11-7".into()));
        // Dies again on the same prefix: no second retry, and not "upgraded".
        let t1 = Instant::now();
        t.poll(ProcState::Alive, false, t1, || None);
        let out = t.poll(ProcState::Gone, false, t1 + Duration::from_secs(20), || Some("GE-Proton11-7".into()));
        assert!(matches!(out, Outcome::Exited { upgraded: false, .. }));
        assert!(!t.should_retry(&out));
    }

    #[test]
    fn gone_before_ever_seen_stays_pending() {
        let mut t = tracked(false);
        assert_eq!(t.poll(ProcState::Gone, false, Instant::now(), || None), Outcome::Pending);
        assert!(!t.process_seen());
    }

    #[test]
    fn flat_games_settle_and_vr_games_wait_for_the_xr_client() {
        let t0 = Instant::now();
        let mut flat = tracked(false);
        flat.poll(ProcState::Alive, false, t0, || None);
        assert_eq!(flat.poll(ProcState::Alive, false, t0 + Duration::from_secs(13), || None), Outcome::Launched);
        let mut vr = tracked(true);
        vr.poll(ProcState::Alive, false, t0, || None);
        assert_eq!(vr.poll(ProcState::Alive, false, t0 + Duration::from_secs(13), || None), Outcome::Pending);
        assert_eq!(vr.poll(ProcState::Alive, true, t0 + Duration::from_secs(14), || None), Outcome::Launched);
    }
}
