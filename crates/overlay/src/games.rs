// Game catalogue for the launcher. Reuses monadeck-core's Steam scanner and
// cover-art lookup, decoding covers into `egui::ColorImage` off the render
// thread so the panel never hitches while libraries are read.
use monadeck_core::steam;

/// A scanned game with its cover already decoded to CPU pixels (no GPU handle
/// yet — `ColorImage` is `Send`, so this is what crosses the scan->render
/// channel; the render thread turns it into a texture via [`load_into`]).
pub struct GameEntry {
    pub name: String,
    /// Steam appid (real Steam apps).
    pub app_id: Option<String>,
    /// Non-Steam shortcut appid (custom-library games), for artwork + launch.
    pub shortcut_id: Option<String>,
    pub source: String,
    pub game_path: String,
    pub last_played: Option<u64>,
    pub cover: Option<egui::ColorImage>,
    /// Landscape banner art for the hero, when available.
    pub hero: Option<egui::ColorImage>,
}

/// A game ready to draw: cover uploaded as a texture in the panel's egui context.
pub struct LoadedGame {
    pub name: String,
    pub app_id: Option<String>,
    pub shortcut_id: Option<String>,
    pub source: String,
    // Used by launching for non-Steam entries (exe path) once direct-exe launch
    // lands; kept now so the catalogue already carries it.
    #[allow(dead_code)]
    pub game_path: String,
    pub last_played: Option<u64>,
    pub texture: Option<egui::TextureHandle>,
    pub hero: Option<egui::TextureHandle>,
}

/// Scan all libraries and pre-decode cover art, ordered most-recently-played
/// first. Call on a background thread.
pub fn scan() -> Vec<GameEntry> {
    let mut entries: Vec<GameEntry> = steam::scan_games()
        .into_iter()
        .map(|g| {
            // Steam appid first, else the non-Steam shortcut appid (custom art).
            let cover_id = g.app_id.clone().or_else(|| g.shortcut_id.clone());
            let cover = cover_id.as_deref().and_then(decode_cover);
            let hero = cover_id.as_deref().and_then(decode_hero);
            GameEntry {
                name: g.name,
                app_id: g.app_id,
                shortcut_id: g.shortcut_id,
                source: g.source,
                game_path: g.game_path,
                last_played: g.last_played,
                cover,
                hero,
            }
        })
        .collect();
    // Most-recently-played first; never-played (None -> 0) fall to the bottom,
    // tie-broken by name. The first entry becomes the default selection.
    entries.sort_by(|a, b| {
        b.last_played
            .unwrap_or(0)
            .cmp(&a.last_played.unwrap_or(0))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

/// Decode a game's cover to RGBA pixels, downscaled to keep VRAM modest
/// (covers are 600×900; ~300×450 is plenty at panel resolution).
fn decode_cover(cover_id: &str) -> Option<egui::ColorImage> {
    let (bytes, _is_png) = steam::game_cover_bytes(cover_id, None)?;
    decode_scaled(&bytes, 300, 450)
}

/// Decode the landscape hero/banner art (wider, lower).
fn decode_hero(cover_id: &str) -> Option<egui::ColorImage> {
    let (bytes, _is_png) = steam::game_hero_bytes(cover_id)?;
    decode_scaled(&bytes, 900, 360)
}

fn decode_scaled(bytes: &[u8], max_w: u32, max_h: u32) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = img.thumbnail(max_w, max_h);
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();
    Some(egui::ColorImage::from_rgba_unmultiplied(size, &pixels))
}

/// Upload decoded covers into the panel's egui context (render thread).
pub fn load_into(ctx: &egui::Context, entries: Vec<GameEntry>) -> Vec<LoadedGame> {
    entries
        .into_iter()
        .map(|e| {
            let key = e
                .app_id
                .clone()
                .or_else(|| e.shortcut_id.clone())
                .unwrap_or_else(|| e.name.clone());
            let texture = e.cover.map(|img| {
                ctx.load_texture(format!("cover-{key}"), img, egui::TextureOptions::LINEAR)
            });
            let hero = e.hero.map(|img| {
                ctx.load_texture(format!("hero-{key}"), img, egui::TextureOptions::LINEAR)
            });
            LoadedGame {
                name: e.name,
                app_id: e.app_id,
                shortcut_id: e.shortcut_id,
                source: e.source,
                game_path: e.game_path,
                last_played: e.last_played,
                texture,
                hero,
            }
        })
        .collect()
}
