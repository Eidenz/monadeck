//! Change notifications for config folders, pushed by the kernel (inotify).
//!
//! One thread sleeps on an inotify descriptor watching each folder (the folder,
//! not its files: editors often save by writing a new file and renaming it
//! over the old one). It reacts to finished writes, renames and deletions of
//! `.json` files, lets a save's burst of events settle, then raises that
//! folder's flag; [`DirWatch::changed`] takes it. Nothing runs between changes.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How long to wait for more events once one arrived (a save is several).
const SETTLE: Duration = Duration::from_millis(60);

const MASK: u32 = libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_MOVED_FROM | libc::IN_DELETE | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF;

pub struct DirWatch {
    flags: Arc<Vec<AtomicBool>>,
}

impl DirWatch {
    /// Watch `dirs` (created if missing). Fails when inotify can't be set up,
    /// e.g. the per-user instance limit is used up: callers fall back to polling.
    pub fn new(dirs: Vec<PathBuf>) -> std::io::Result<Self> {
        // SAFETY: plain syscalls on a descriptor this function owns.
        let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut wds = Vec::with_capacity(dirs.len());
        for d in &dirs {
            match add_watch(fd, d) {
                Ok(wd) => wds.push(wd),
                Err(e) => {
                    unsafe { libc::close(fd) };
                    return Err(e);
                }
            }
        }
        let flags: Arc<Vec<AtomicBool>> = Arc::new(dirs.iter().map(|_| AtomicBool::new(false)).collect());
        let shared = Arc::clone(&flags);
        std::thread::Builder::new().name("dir-watch".into()).spawn(move || run(fd, dirs, wds, &shared))?;
        Ok(Self { flags })
    }

    /// Whether folder `i` (in the order given) changed since the last call.
    pub fn changed(&self, i: usize) -> bool {
        self.flags.get(i).is_some_and(|f| f.swap(false, Ordering::AcqRel))
    }
}

fn add_watch(fd: i32, dir: &PathBuf) -> std::io::Result<i32> {
    std::fs::create_dir_all(dir)?;
    let c = CString::new(dir.as_os_str().as_bytes()).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    // SAFETY: `c` is a valid NUL-terminated path for the duration of the call.
    let wd = unsafe { libc::inotify_add_watch(fd, c.as_ptr(), MASK) };
    if wd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(wd)
}

/// Wait until the descriptor has events, up to `timeout` (`None`: forever).
fn wait(fd: i32, timeout: Option<Duration>) -> bool {
    let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    let ms = timeout.map_or(-1, |t| t.as_millis() as i32);
    // SAFETY: one valid pollfd.
    unsafe { libc::poll(&mut p, 1, ms) > 0 }
}

fn run(fd: i32, dirs: Vec<PathBuf>, mut wds: Vec<i32>, flags: &[AtomicBool]) {
    let mut buf = vec![0u8; 16 * 1024];
    let mut pending = vec![false; dirs.len()];
    loop {
        // Sleep until something happens, then keep reading while it settles.
        let mut timeout = None;
        while wait(fd, timeout) {
            // SAFETY: reading into our own buffer.
            let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                log::warn!("watch: inotify read failed ({err}), no more change notifications");
                unsafe { libc::close(fd) };
                return;
            }
            let mut off = 0usize;
            while off + std::mem::size_of::<libc::inotify_event>() <= n as usize {
                // SAFETY: the kernel writes whole events; unaligned read of the header.
                let ev: libc::inotify_event = unsafe { std::ptr::read_unaligned(buf.as_ptr().add(off).cast()) };
                let name_start = off + std::mem::size_of::<libc::inotify_event>();
                let name = &buf[name_start..name_start + ev.len as usize];
                let name = name.split(|&b| b == 0).next().unwrap_or(&[]);
                off = name_start + ev.len as usize;
                let Some(i) = wds.iter().position(|&w| w == ev.wd) else { continue };
                if ev.mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_IGNORED) != 0 {
                    // The folder itself went away: bring it back and watch it again.
                    pending[i] = true;
                    match add_watch(fd, &dirs[i]) {
                        Ok(wd) => wds[i] = wd,
                        Err(e) => log::warn!("watch: can't watch {} again: {e}", dirs[i].display()),
                    }
                } else if name.ends_with(b".json") {
                    pending[i] = true;
                }
            }
            timeout = Some(SETTLE);
        }
        for (i, p) in pending.iter_mut().enumerate() {
            if std::mem::take(p) {
                flags[i].store(true, Ordering::Release);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for(w: &DirWatch, i: usize) -> bool {
        (0..60).any(|_| {
            std::thread::sleep(Duration::from_millis(10));
            w.changed(i)
        })
    }

    #[test]
    fn sees_saves_renames_and_deletions() {
        let root = std::env::temp_dir().join(format!("monadeck-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (a, b) = (root.join("a"), root.join("b"));
        let w = DirWatch::new(vec![a.clone(), b.clone()]).unwrap();
        assert!(a.is_dir() && b.is_dir(), "folders are created");

        std::fs::write(a.join("knuckles.json"), "{}").unwrap();
        assert!(wait_for(&w, 0));
        assert!(!w.changed(1), "only the folder that changed");

        // An editor's save: a temp file renamed over the real one.
        std::fs::write(b.join(".tmp"), "{}").unwrap();
        std::fs::rename(b.join(".tmp"), b.join("x.json")).unwrap();
        assert!(wait_for(&w, 1));

        // Reset renames the file aside: it's gone, that's a change.
        std::fs::rename(a.join("knuckles.json"), a.join("knuckles.json.bak")).unwrap();
        assert!(wait_for(&w, 0));

        // Other files don't count.
        std::fs::write(a.join("notes.txt"), "hi").unwrap();
        assert!(!wait_for(&w, 0));

        // The folder removed and made again: still watched.
        std::fs::remove_dir_all(&a).unwrap();
        assert!(wait_for(&w, 0));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join("knuckles.json"), "{}").unwrap();
        assert!(wait_for(&w, 0));
        let _ = std::fs::remove_dir_all(&root);
    }
}
