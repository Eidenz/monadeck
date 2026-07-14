//! Detect and stop a running SteamVR before starting monado.
//!
//! SteamVR and monado both want exclusive control of the HMD's display and
//! tracking; with SteamVR up, monado typically fails to grab the headset (or
//! crashes outright). There is no workflow where you'd run both at once — the
//! one time SteamVR is needed is floor calibration, during which monado isn't
//! running anyway. Worse, powering on controllers/trackers can silently
//! auto-launch SteamVR in the background, so a start can fail for reasons the
//! user can't see. So, on by default, Monadeck stops SteamVR first.
//!
//! We target only `vrserver` (the SteamVR core process; `vrserver.exe` when it
//! runs under Proton/Wine). SteamVR is multi-process — vrcompositor, vrmonitor,
//! … — but taking down vrserver brings the rest down with it, so that single
//! target is enough and keeps us from over-matching unrelated processes.

use std::fs;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// The SteamVR core process, as it appears natively and under Proton/Wine. The
/// kernel `comm` name is capped at 15 chars, but both fit comfortably.
const TARGET_NAMES: &[&str] = &["vrserver", "vrserver.exe"];

fn is_target(name: &str) -> bool {
    TARGET_NAMES.contains(&name)
}

/// True if a process's name (`/proc/<pid>/comm`) or the basename of its argv[0]
/// (`/proc/<pid>/cmdline`) is exactly `vrserver` / `vrserver.exe`. Matching both
/// covers native SteamVR (comm = `vrserver`) and the Proton build, where `comm`
/// can be the Wine loader while the command line still names `vrserver.exe`.
fn process_matches(pid: libc::pid_t) -> bool {
    if let Ok(comm) = fs::read_to_string(format!("/proc/{pid}/comm")) {
        if is_target(comm.trim()) {
            return true;
        }
    }
    if let Ok(cmdline) = fs::read(format!("/proc/{pid}/cmdline")) {
        // cmdline is NUL-separated; argv[0] is the program path. Take its
        // basename on both separators since Wine paths use backslashes.
        if let Some(arg0) = cmdline.split(|b| *b == 0).next() {
            let arg0 = String::from_utf8_lossy(arg0);
            let base = arg0
                .rsplit(|c| c == '/' || c == '\\')
                .next()
                .unwrap_or_default();
            if is_target(base) {
                return true;
            }
        }
    }
    false
}

/// PIDs of every running SteamVR core process.
fn vrserver_pids() -> Vec<libc::pid_t> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<libc::pid_t>().ok())
        .filter(|&pid| process_matches(pid))
        .collect()
}

/// Whether the process is still alive (`kill(pid, 0)` succeeds only for a live,
/// signalable process; it returns ESRCH once the process is gone).
fn pid_alive(pid: libc::pid_t) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Whether SteamVR is currently running.
pub fn steamvr_running() -> bool {
    !vrserver_pids().is_empty()
}

/// Stop SteamVR if it's running and return how many core processes we signalled
/// (0 when SteamVR wasn't up — a cheap no-op). SIGTERM lets SteamVR shut its
/// child processes down cleanly; anything still alive after a short grace period
/// is force-killed.
pub fn kill_steamvr() -> usize {
    let pids = vrserver_pids();
    if pids.is_empty() {
        return 0;
    }
    let count = pids.len();
    for &pid in &pids {
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }

    // Give SteamVR up to ~3s to tear itself (and vrcompositor/vrmonitor/…) down.
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if pids.iter().all(|&p| !pid_alive(p)) {
            return count;
        }
        sleep(Duration::from_millis(100));
    }

    // Still here after the grace period → force it.
    for &pid in &pids {
        if pid_alive(pid) {
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_exact_vrserver_names() {
        assert!(is_target("vrserver"));
        assert!(is_target("vrserver.exe"));
        assert!(!is_target("vrserver.exe.old"));
        assert!(!is_target("vrserverd"));
        assert!(!is_target("myvrserver"));
        assert!(!is_target("vrcompositor"));
        assert!(!is_target(""));
    }

    #[test]
    fn does_not_match_this_test_process() {
        // The test binary isn't SteamVR, so scanning must never flag ourselves.
        let me = unsafe { libc::getpid() };
        assert!(!process_matches(me));
    }

    #[test]
    fn scan_runs_without_error() {
        // Whatever is on the box, enumerating /proc must not panic.
        let _ = steamvr_running();
    }
}
