//! Path-based plugins — the whole point of Monadeck's launcher.
//!
//! Unlike Envision (which scans freedesktop `.desktop` files on `$PATH`/XDG dirs
//! and reads `X-XR-Plugin-Exec`), a Monadeck plugin is just an explicit
//! executable path plus args and a launch moment. No PATH, no desktop entries —
//! you point it at a binary and it runs.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// When a plugin should run relative to the monado service lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ExecWhen {
    /// Launch right after `monado-service` comes up (the common case).
    #[default]
    AfterStart,
    /// Run once the service has stopped (cleanup, restore, etc.).
    AfterStop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    /// Display name in the UI.
    pub name: String,
    /// Absolute path to the executable to run.
    pub path: PathBuf,
    /// Extra arguments passed verbatim.
    #[serde(default)]
    pub args: Vec<String>,
    /// When to launch it.
    #[serde(default)]
    pub when: ExecWhen,
    /// Disabled plugins stay in the list but don't launch.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Plugin {
    /// Whether this plugin points at an installed app (`.desktop`) rather than a
    /// raw executable.
    fn is_desktop_entry(&self) -> bool {
        self.path.extension().and_then(|e| e.to_str()) == Some("desktop")
    }

    /// Resolve the argv to spawn: for a `.desktop` plugin, the parsed `Exec`
    /// (field codes stripped) plus any extra args; otherwise the path + args.
    fn argv(&self) -> Result<Vec<String>> {
        if self.is_desktop_entry() {
            let exec = crate::desktop::entry_exec(&self.path).ok_or_else(|| {
                anyhow::anyhow!("no Exec in desktop entry {}", self.path.display())
            })?;
            let mut argv = crate::desktop::parse_exec(&exec);
            if argv.is_empty() {
                bail!("empty Exec in desktop entry {}", self.path.display());
            }
            argv.extend(self.args.iter().cloned());
            Ok(argv)
        } else {
            let mut argv = vec![self.path.to_string_lossy().to_string()];
            argv.extend(self.args.iter().cloned());
            Ok(argv)
        }
    }

    /// Spawn the plugin detached, with `env` overlaid on the inherited
    /// environment. Works for both raw executables and installed apps. Returns
    /// the spawned child — the caller keeps it so the plugin can be stopped and
    /// reaped when the service goes down (rather than lingering across restarts).
    pub fn launch(&self, env: &HashMap<String, String>) -> Result<Child> {
        if !self.enabled {
            bail!("plugin '{}' is disabled", self.name);
        }
        if !self.path.is_file() {
            bail!(
                "plugin '{}' path does not exist: {}",
                self.name,
                self.path.display()
            );
        }
        let argv = self.argv()?;
        let child = Command::new(&argv[0])
            .args(&argv[1..])
            .envs(env)
            // Detach stdio so a chatty plugin doesn't block on a full pipe; its
            // own logging is its concern.
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("launching plugin '{}'", self.name))?;
        Ok(child)
    }
}

/// Stop a plugin we launched: SIGTERM, then SIGKILL after a short grace, and reap
/// it. Without this, a plugin (e.g. WayVR) launched on service start outlives the
/// service stop, so the next start spawns a second instance that collides with the
/// lingering first one — and the un-reaped child becomes a zombie.
pub fn terminate(child: &mut Child) {
    // Already exited — just reap.
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    let pid = child.id() as libc::pid_t;
    unsafe { libc::kill(pid, libc::SIGTERM) };
    // Give it ~1.5 s to exit cleanly (so it can release sockets/locks), then force it.
    for _ in 0..15 {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
}
