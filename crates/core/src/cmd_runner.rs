//! Spawns a child process and streams its stdout/stderr into a shared, capped
//! line buffer. Ported and trimmed from Envision's `cmd_runner.rs`: the original
//! used a channel + `nix`; here we accumulate directly under a mutex and shut the
//! process down with SIGTERM (then SIGKILL) via `libc`, which is all Monadeck
//! needs to run `monado-service` and show its logs live.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, sleep, JoinHandle};
use std::time::Duration;

/// Keep memory bounded for a long-running session; monado is chatty.
const MAX_LINES: usize = 8000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerStatus {
    Running,
    /// Stopped, with the process exit code if one was reported.
    Stopped(Option<i32>),
    NeverStarted,
}

#[derive(Default)]
struct LogBuf {
    lines: Vec<String>,
    /// Total lines ever pushed, so a reader can ask "what's new since N?" even
    /// after old lines were trimmed off the front.
    total: u64,
}

pub struct CmdRunner {
    process: Option<Child>,
    log: Arc<Mutex<LogBuf>>,
    threads: Vec<JoinHandle<()>>,
    last_exit: Option<i32>,
}

impl Default for CmdRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl CmdRunner {
    pub fn new() -> Self {
        Self {
            process: None,
            log: Arc::new(Mutex::new(LogBuf::default())),
            threads: Vec::new(),
            last_exit: None,
        }
    }

    /// Spawn `command args...` with `env` overlaid on the inherited environment.
    /// Replaces any previously tracked process (caller should stop it first).
    pub fn start(
        &mut self,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> std::io::Result<()> {
        self.threads.clear();
        // stdin MUST be a pipe: monado-service adds its stdin to an epoll set
        // (to detect the launcher going away), and epoll rejects the /dev/null
        // or regular-file stdin a GUI/dev-launched process otherwise inherits —
        // which makes the service abort with `epoll_ctl(stdin) failed`. We hold
        // the write end open (never take `child.stdin`) so it keeps running;
        // closing it (on drop) signals the service to exit cleanly.
        let mut child = Command::new(command)
            .args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");
        self.threads.push(Self::spawn_reader(stdout, self.log.clone()));
        self.threads.push(Self::spawn_reader(stderr, self.log.clone()));

        self.process = Some(child);
        self.last_exit = None;
        Ok(())
    }

    fn spawn_reader<R: std::io::Read + Send + 'static>(
        src: R,
        log: Arc<Mutex<LogBuf>>,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            let mut reader = BufReader::new(src);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => return, // EOF or read error: reader done
                    Ok(_) => {
                        let mut buf = log.lock().expect("log mutex poisoned");
                        buf.lines.push(line.trim_end_matches('\n').to_string());
                        buf.total += 1;
                        let overflow = buf.lines.len().saturating_sub(MAX_LINES);
                        if overflow > 0 {
                            buf.lines.drain(0..overflow);
                        }
                    }
                }
            }
        })
    }

    /// Current run status, reaping the child if it has exited.
    pub fn status(&mut self) -> RunnerStatus {
        match self.process.as_mut() {
            None => match self.last_exit {
                Some(code) => RunnerStatus::Stopped(Some(code)),
                None => RunnerStatus::NeverStarted,
            },
            Some(proc) => match proc.try_wait() {
                Ok(Some(status)) => {
                    self.last_exit = status.code();
                    self.process = None;
                    RunnerStatus::Stopped(status.code())
                }
                Ok(None) => RunnerStatus::Running,
                Err(_) => RunnerStatus::Running,
            },
        }
    }

    pub fn is_running(&mut self) -> bool {
        self.status() == RunnerStatus::Running
    }

    /// SIGTERM the process, then SIGKILL after a short grace period, so monado
    /// gets a chance to release its IPC socket and let us restore runtime files.
    pub fn terminate(&mut self) {
        let Some(mut proc) = self.process.take() else {
            return;
        };
        let pid = proc.id() as libc::pid_t;
        unsafe { libc::kill(pid, libc::SIGTERM) };

        // Give it two seconds, then force-kill if still alive.
        for _ in 0..20 {
            match proc.try_wait() {
                Ok(Some(status)) => {
                    self.last_exit = status.code();
                    self.join_readers();
                    return;
                }
                _ => sleep(Duration::from_millis(100)),
            }
        }
        unsafe { libc::kill(pid, libc::SIGKILL) };
        if let Ok(status) = proc.wait() {
            self.last_exit = status.code();
        }
        self.join_readers();
    }

    fn join_readers(&mut self) {
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }

    /// Snapshot of the buffered log lines.
    pub fn lines(&self) -> Vec<String> {
        self.log.lock().expect("log mutex poisoned").lines.clone()
    }

    /// Total lines emitted since start (including trimmed ones), for cursoring.
    pub fn total_lines(&self) -> u64 {
        self.log.lock().expect("log mutex poisoned").total
    }

    /// Lines whose absolute index is `>= since`. Returns `(next_cursor, lines)`.
    /// Lines trimmed off the front are silently skipped.
    pub fn lines_since(&self, since: u64) -> (u64, Vec<String>) {
        let buf = self.log.lock().expect("log mutex poisoned");
        let first_index = buf.total.saturating_sub(buf.lines.len() as u64);
        let start = since.max(first_index);
        let offset = (start - first_index) as usize;
        let slice = buf.lines.get(offset..).unwrap_or(&[]).to_vec();
        (buf.total, slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_output_and_reports_exit() {
        let mut r = CmdRunner::new();
        r.start(
            "bash",
            &["-c".into(), "echo hello; echo world 1>&2".into()],
            &HashMap::new(),
        )
        .unwrap();
        // Wait for completion.
        for _ in 0..50 {
            if r.status() != RunnerStatus::Running {
                break;
            }
            sleep(Duration::from_millis(20));
        }
        let lines = r.lines();
        assert!(lines.iter().any(|l| l == "hello"));
        assert!(lines.iter().any(|l| l == "world"));
    }
}
