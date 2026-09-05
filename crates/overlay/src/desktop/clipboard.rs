//! Clipboard text preview for the keyboard's top bar. Reading the Wayland
//! clipboard needs focus (or a data-control protocol), so we lean on
//! `wl-paste` from wl-clipboard, polled on a thread only while the keyboard is
//! up. Absent tool → no preview, nothing else breaks.
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct ClipboardWatcher {
    active: Arc<AtomicBool>,
    latest: Arc<Mutex<Option<String>>>,
    available: bool,
}

impl ClipboardWatcher {
    pub fn new() -> Self {
        let available = Command::new("wl-paste").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok();
        let active = Arc::new(AtomicBool::new(false));
        let latest: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        if available {
            let (a, l) = (active.clone(), latest.clone());
            std::thread::Builder::new()
                .name("clipboard-watch".into())
                .spawn(move || loop {
                    if a.load(Ordering::Relaxed) {
                        let out = Command::new("wl-paste")
                            .args(["--no-newline", "-t", "text/plain"])
                            .stderr(Stdio::null())
                            .output();
                        let text = match out {
                            Ok(o) if o.status.success() => {
                                let s = String::from_utf8_lossy(&o.stdout).replace(['\n', '\r'], " ");
                                (!s.trim().is_empty()).then(|| s.trim().to_string())
                            }
                            _ => None,
                        };
                        if let Ok(mut g) = l.lock() {
                            *g = text;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(1500));
                })
                .expect("spawn clipboard thread");
        } else {
            log::info!("desktop: wl-paste not found; no clipboard preview");
        }
        Self { active, latest, available }
    }

    pub fn set_active(&self, on: bool) {
        self.active.store(on && self.available, Ordering::Relaxed);
    }

    pub fn latest(&self) -> Option<String> {
        self.latest.lock().ok().and_then(|g| g.clone())
    }
}
