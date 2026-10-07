//! Change notifications pushed by the kernel (inotify).
//!
//! One thread sleeps on an inotify descriptor watching each folder (the folder,
//! not its files: editors often save by writing a new file and renaming it
//! over the old one). It reacts to finished writes, renames and deletions,
//! lets a save's burst of events settle, then tells: [`DirWatch`] raises a
//! folder's flag for its `.json` files ([`DirWatch::changed`] takes it),
//! [`FileWatch`] calls back for one file. Nothing runs between changes.

use std::ffi::{CString, OsString};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
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

/// One file, watched through its folder: `on_change` runs (on the watch's
/// thread) once each write, replacement or removal of it settles. Dropping
/// the watch ends the thread.
pub struct FileWatch {
    stop: std::sync::Arc<OwnedFd>,
}

impl FileWatch {
    /// Fails when inotify can't be set up (the per-user limit used up).
    pub fn new(path: PathBuf, on_change: impl FnMut() + Send + 'static) -> std::io::Result<Self> {
        let invalid = |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("{what}: {}", path.display()));
        let dir = path.parent().ok_or_else(|| invalid("no folder"))?.to_path_buf();
        let name = path.file_name().ok_or_else(|| invalid("no file name"))?.to_os_string();
        // SAFETY: plain syscalls; each descriptor is owned right away.
        let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let wd = add_watch(fd.as_raw_fd(), &dir)?;
        let stop = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC) };
        if stop < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let stop = std::sync::Arc::new(unsafe { OwnedFd::from_raw_fd(stop) });
        let stopped = std::sync::Arc::clone(&stop);
        std::thread::Builder::new().name("file-watch".into()).spawn(move || run_file(fd, &stopped, dir, wd, name, on_change))?;
        Ok(Self { stop })
    }
}

impl Drop for FileWatch {
    fn drop(&mut self) {
        let one: u64 = 1;
        // SAFETY: an 8-byte write to our eventfd; the thread wakes and leaves.
        unsafe { libc::write(self.stop.as_raw_fd(), (&one as *const u64).cast(), 8) };
    }
}

fn run_file(fd: OwnedFd, stop: &OwnedFd, dir: PathBuf, mut wd: i32, name: OsString, mut on_change: impl FnMut()) {
    let mut buf = vec![0u8; 4096];
    loop {
        let mut fds = [
            libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: stop.as_raw_fd(), events: libc::POLLIN, revents: 0 },
        ];
        // SAFETY: two valid pollfds. Sleeps until the folder changes or the watch is dropped.
        if unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) } < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        if fds[1].revents != 0 {
            return;
        }
        let mut hit = false;
        let mut more = true;
        while more {
            let ok = read_events(fd.as_raw_fd(), &mut buf, |ev_wd, mask, ev_name| {
                if ev_wd != wd {
                    return;
                }
                if mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_IGNORED) != 0 {
                    hit = true;
                    match add_watch(fd.as_raw_fd(), &dir) {
                        Ok(w) => wd = w,
                        Err(e) => log::warn!("watch: can't watch {} again: {e}", dir.display()),
                    }
                } else if ev_name == name.as_bytes() {
                    hit = true;
                }
            });
            if !ok {
                return;
            }
            more = wait(fd.as_raw_fd(), Some(SETTLE));
        }
        if hit {
            on_change();
        }
    }
}

/// Read what the descriptor has and hand each event over as (watch, mask,
/// file name). False when reading failed for good.
fn read_events(fd: i32, buf: &mut [u8], mut each: impl FnMut(i32, u32, &[u8])) -> bool {
    // SAFETY: reading into our own buffer.
    let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
    if n <= 0 {
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::Interrupted {
            return true;
        }
        log::warn!("watch: inotify read failed ({err}), no more change notifications");
        return false;
    }
    let mut off = 0usize;
    while off + std::mem::size_of::<libc::inotify_event>() <= n as usize {
        // SAFETY: the kernel writes whole events; unaligned read of the header.
        let ev: libc::inotify_event = unsafe { std::ptr::read_unaligned(buf.as_ptr().add(off).cast()) };
        let name_start = off + std::mem::size_of::<libc::inotify_event>();
        let name = &buf[name_start..name_start + ev.len as usize];
        let name = name.split(|&b| b == 0).next().unwrap_or(&[]);
        off = name_start + ev.len as usize;
        each(ev.wd, ev.mask, name);
    }
    true
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
            let ok = read_events(fd, &mut buf, |wd, mask, name| {
                let Some(i) = wds.iter().position(|&w| w == wd) else { return };
                if mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_IGNORED) != 0 {
                    // The folder itself went away: bring it back and watch it again.
                    pending[i] = true;
                    match add_watch(fd, &dirs[i]) {
                        Ok(wd) => wds[i] = wd,
                        Err(e) => log::warn!("watch: can't watch {} again: {e}", dirs[i].display()),
                    }
                } else if name.ends_with(b".json") {
                    pending[i] = true;
                }
            });
            if !ok {
                unsafe { libc::close(fd) };
                return;
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

    #[test]
    fn file_watch_calls_back_for_its_file_until_dropped() {
        use std::sync::atomic::AtomicUsize;
        let root = std::env::temp_dir().join(format!("monadeck-filewatch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("openvrpaths.vrpath");
        let hits = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&hits);
        let w = FileWatch::new(file.clone(), move || {
            seen.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        let settled = |n: usize| (0..60).any(|_| {
            std::thread::sleep(Duration::from_millis(10));
            hits.load(Ordering::SeqCst) == n
        });

        std::fs::write(&file, "{}").unwrap();
        assert!(settled(1), "a write");
        // A new file renamed over it.
        std::fs::write(root.join(".tmp"), "{}").unwrap();
        std::fs::rename(root.join(".tmp"), &file).unwrap();
        assert!(settled(2), "a replacement");
        // Its neighbours don't count.
        std::fs::write(root.join("other.vrpath"), "{}").unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(hits.load(Ordering::SeqCst), 2);

        drop(w);
        std::thread::sleep(Duration::from_millis(50));
        std::fs::write(&file, "{}").unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(hits.load(Ordering::SeqCst), 2, "no calls once dropped");
        let _ = std::fs::remove_dir_all(&root);
    }
}
