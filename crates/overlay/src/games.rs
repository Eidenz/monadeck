// Game catalogue for the launcher. The catalogue itself is metadata only (no
// images) so it stays cheap regardless of library size — cover/hero/logo art is
// decoded lazily, on a background thread pool, only when a tile is on-screen or
// a game is selected. This is what lets a friend's 2000-game library load fast.
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use monadeck_core::steam;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ArtKind {
    Cover,
    Hero,
    Logo,
}

/// The three art kinds, for iterating slots (LRU sweep, etc.).
pub const ART_KINDS: [ArtKind; 3] = [ArtKind::Cover, ArtKind::Hero, ArtKind::Logo];

/// Load state of one art slot for one game.
pub enum ArtState {
    /// Not requested yet.
    Idle,
    /// Requested; decoding on a worker.
    Pending,
    /// Decoded + uploaded.
    Ready(egui::TextureHandle),
    /// No such art exists for this game.
    Missing,
}

impl ArtState {
    pub fn is_idle(&self) -> bool {
        matches!(self, ArtState::Idle)
    }
}

pub struct LibGame {
    pub name: String,
    pub app_id: Option<String>,
    pub shortcut_id: Option<String>,
    /// The id art is keyed on (Steam appid or non-Steam shortcut appid).
    pub cover_id: Option<String>,
    pub source: String,
    pub last_played: Option<u64>,
    pub size_on_disk: Option<u64>,
    pub playtime_minutes: Option<u32>,
    pub is_favorite: bool,
    pub cover: ArtState,
    pub hero: ArtState,
    pub logo: ArtState,
}

impl LibGame {
    pub fn art(&self, kind: ArtKind) -> &ArtState {
        match kind {
            ArtKind::Cover => &self.cover,
            ArtKind::Hero => &self.hero,
            ArtKind::Logo => &self.logo,
        }
    }
    pub fn art_mut(&mut self, kind: ArtKind) -> &mut ArtState {
        match kind {
            ArtKind::Cover => &mut self.cover,
            ArtKind::Hero => &mut self.hero,
            ArtKind::Logo => &mut self.logo,
        }
    }
}

/// Scan the launchable catalogue on a background thread, returning a channel that
/// yields the (recency-sorted) rows once. Fast: only reads appmanifests +
/// shortcuts.vdf — no image decoding — so it returns near-instantly. Rows are
/// `core::LibraryGame` (Send; no GPU handles) and become [`LibGame`] via
/// [`to_games`] on the render thread.
pub fn spawn_scan() -> Receiver<Vec<steam::LibraryGame>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut v = steam::scan_library();
        v.sort_by(|a, b| {
            b.last_played
                .unwrap_or(0)
                .cmp(&a.last_played.unwrap_or(0))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let _ = tx.send(v);
    });
    rx
}

/// Build the render-thread catalogue (all art Idle) from the scanned rows.
pub fn to_games(rows: Vec<steam::LibraryGame>) -> Vec<LibGame> {
    rows.into_iter()
        .map(|g| {
            let cover_id = g.app_id.clone().or_else(|| g.shortcut_id.clone());
            LibGame {
                name: g.name,
                app_id: g.app_id,
                shortcut_id: g.shortcut_id,
                cover_id,
                source: g.source,
                last_played: g.last_played,
                size_on_disk: g.size_on_disk,
                playtime_minutes: g.playtime_minutes,
                is_favorite: false,
                cover: ArtState::Idle,
                hero: ArtState::Idle,
                logo: ArtState::Idle,
            }
        })
        .collect()
}

// --- lazy art loading -------------------------------------------------------

struct ArtRequest {
    index: usize,
    kind: ArtKind,
    cover_id: String,
}

pub struct ArtResult {
    pub index: usize,
    pub kind: ArtKind,
    pub image: Option<egui::ColorImage>,
}

/// Background image-decoder pool. Decodes art only when asked (a tile became
/// visible / a game got selected), so libraries of any size stay cheap.
pub struct ArtLoader {
    req_tx: Sender<ArtRequest>,
    res_rx: Receiver<ArtResult>,
}

impl ArtLoader {
    pub fn new() -> Self {
        let (req_tx, req_rx) = mpsc::channel::<ArtRequest>();
        let (res_tx, res_rx) = mpsc::channel::<ArtResult>();
        let req_rx = Arc::new(Mutex::new(req_rx));
        for _ in 0..3 {
            let rx = Arc::clone(&req_rx);
            let tx = res_tx.clone();
            thread::spawn(move || loop {
                // Hold the lock only across recv, then decode unlocked.
                let req = {
                    let guard = rx.lock().unwrap();
                    guard.recv()
                };
                let Ok(req) = req else { break };
                let image = decode(&req.cover_id, req.kind);
                if tx.send(ArtResult { index: req.index, kind: req.kind, image }).is_err() {
                    break;
                }
            });
        }
        Self { req_tx, res_rx }
    }

    pub fn request(&self, index: usize, kind: ArtKind, cover_id: String) {
        let _ = self.req_tx.send(ArtRequest { index, kind, cover_id });
    }

    pub fn try_recv(&self) -> Option<ArtResult> {
        self.res_rx.try_recv().ok()
    }
}

fn decode(cover_id: &str, kind: ArtKind) -> Option<egui::ColorImage> {
    let (bytes, (max_w, max_h)) = match kind {
        ArtKind::Cover => (steam::game_cover_bytes(cover_id, None)?.0, (300, 450)),
        ArtKind::Hero => (steam::game_hero_bytes(cover_id)?.0, (900, 360)),
        ArtKind::Logo => (steam::game_logo_bytes(cover_id)?.0, (520, 320)),
    };
    let img = image::load_from_memory(&bytes).ok()?;
    let img = img.thumbnail(max_w, max_h);
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, &rgba.into_raw()))
}
