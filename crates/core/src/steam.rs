//! Steam/VR game discovery for the binding editor.
//!
//! Scans Steam libraries (incl. Proton compatdata) and xrizer config dirs for
//! games that ship SteamVR/xrizer input bindings — pairing each game's
//! `actions.json` with its `bindings_<controller>.json` files. Plus Steam cover
//! art and a small persisted list of user-added scan paths.
//!
//! Ported from xrbind's `steam_scanner.rs`; `dirs::*` swapped for [`crate::paths`].

use crate::paths::{config_home, home, monadeck_config_dir};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedGame {
    pub name: String,
    pub app_id: Option<String>,
    pub game_path: String,
    pub actions_path: String,
    pub binding_files: Vec<BindingFile>,
    pub source: String,
    /// For non-Steam games (added to Steam as a shortcut): the unsigned 32-bit
    /// shortcut appid, resolved from `shortcuts.vdf`. Names the compatdata folder
    /// and the custom `grid/<id>p.*` artwork, and derives the launch game-id.
    /// `None` for real Steam apps (use `app_id`) and unmatched custom entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut_id: Option<String>,
    /// Unix timestamp the game was last played (Steam `LastPlayed` / non-Steam
    /// `LastPlayTime`), for recency ordering. `None`/0 if never played or unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingFile {
    pub controller_type: String,
    pub file_path: String,
    pub file_name: String,
}

#[derive(Deserialize)]
struct ActionsManifest {
    #[serde(default)]
    default_bindings: Vec<DefaultBinding>,
}

#[derive(Deserialize)]
struct DefaultBinding {
    controller_type: String,
    binding_url: String,
}

// --- config (custom scan paths + custom covers) under ~/.config/monadeck ------

fn covers_dir() -> PathBuf {
    monadeck_config_dir().join("covers")
}

fn custom_paths_file() -> PathBuf {
    monadeck_config_dir().join("binding_custom_paths.json")
}

pub fn get_custom_paths() -> Vec<String> {
    fs::read_to_string(custom_paths_file())
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

pub fn set_custom_paths(paths: &[String]) -> std::io::Result<()> {
    let file = custom_paths_file();
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(paths)?;
    fs::write(file, json)
}

// --- top-level API ------------------------------------------------------------

/// All detected games (steam common, Proton prefixes, xrizer configs/overrides,
/// and user-added paths), sorted by name.
pub fn scan_games() -> Vec<DetectedGame> {
    let custom_paths = get_custom_paths();
    let mut games: Vec<DetectedGame> = Vec::new();

    let steam_roots = find_steam_roots();
    let library_paths = find_library_folders(&steam_roots);

    let mut app_name_map: HashMap<String, String> = HashMap::new();
    let mut last_played_map: HashMap<String, u64> = HashMap::new();
    for lib_path in &library_paths {
        let steamapps = lib_path.join("steamapps");
        if steamapps.is_dir() {
            collect_app_names(&steamapps, &mut app_name_map, &mut last_played_map);
        }
    }

    for lib_path in &library_paths {
        let common = lib_path.join("steamapps").join("common");
        if common.is_dir() {
            scan_directory_for_bindings(&common, &app_name_map, "steam", &mut games);
        }
        let compatdata = lib_path.join("steamapps").join("compatdata");
        if compatdata.is_dir() {
            scan_compatdata(&compatdata, &app_name_map, &mut games);
        }
    }

    scan_xrizer_configs(&mut games);

    for custom_path in &custom_paths {
        let p = Path::new(custom_path);
        if p.is_dir() {
            scan_directory_for_bindings(p, &app_name_map, "custom", &mut games);
        }
    }

    // Collapse a game that has both a default binding and an xrizer override into
    // ONE entry (the override is what xrizer loads), but MERGE the binding files
    // so every controller stays editable: the override's file wins per
    // controller_type, and the default supplies the controllers the override
    // lacks. Keyed by game path, which both entries share.
    let override_paths: HashSet<String> = games
        .iter()
        .filter(|g| g.source.contains("xrizer (game override)"))
        .map(|g| g.game_path.clone())
        .collect();

    let mut extras: HashMap<String, Vec<BindingFile>> = HashMap::new();
    for g in &games {
        if !g.source.contains("xrizer (game override)") && override_paths.contains(&g.game_path) {
            extras
                .entry(g.game_path.clone())
                .or_default()
                .extend(g.binding_files.iter().cloned());
        }
    }
    for g in &mut games {
        if g.source.contains("xrizer (game override)") {
            if let Some(extra) = extras.get(&g.game_path) {
                let mut have: HashSet<String> =
                    g.binding_files.iter().map(|b| b.controller_type.clone()).collect();
                for bf in extra {
                    if have.insert(bf.controller_type.clone()) {
                        g.binding_files.push(bf.clone());
                    }
                }
                g.binding_files
                    .sort_by(|a, b| a.controller_type.cmp(&b.controller_type));
            }
        }
    }
    games.retain(|g| {
        g.source.contains("xrizer (game override)") || !override_paths.contains(&g.game_path)
    });

    // Steam games: stamp last-played from the appmanifest map.
    for g in games.iter_mut() {
        if let Some(id) = &g.app_id {
            if let Some(&ts) = last_played_map.get(id) {
                g.last_played = Some(ts);
            }
        }
    }

    // Resolve non-Steam shortcuts for custom entries that have no Steam appid —
    // gives them their `grid/<id>p.*` artwork, a launchable id, and last-played.
    // Parse shortcuts.vdf only if some game actually needs it.
    if games.iter().any(|g| g.app_id.is_none() && g.shortcut_id.is_none()) {
        let shortcuts = parse_shortcuts();
        if !shortcuts.is_empty() {
            for g in games.iter_mut().filter(|g| g.app_id.is_none() && g.shortcut_id.is_none()) {
                if let Some(s) = match_shortcut(g, &shortcuts) {
                    g.shortcut_id = Some(s.app_id.to_string());
                    if s.last_play > 0 {
                        g.last_played = Some(s.last_play as u64);
                    }
                }
            }
        }
    }

    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    games
}

/// Read a game's `actions.json` + a chosen binding file, as `(actions, binding)`.
pub fn load_game_bindings(actions_path: &str, binding_path: &str) -> std::io::Result<(String, String)> {
    Ok((
        fs::read_to_string(actions_path)?,
        fs::read_to_string(binding_path)?,
    ))
}

/// Raw cover-art image bytes for a game — a user-set custom cover (keyed by
/// `game_key`) first, then Steam's library cache. Returns `(bytes, is_png)`.
///
/// For consumers that decode images themselves (the in-headset overlay builds
/// GPU textures) rather than embedding a `data:` URL.
pub fn game_cover_bytes(cover_id: &str, game_key: Option<&str>) -> Option<(Vec<u8>, bool)> {
    // 1. A monadeck-set custom cover (keyed by game_key).
    if let Some(key) = game_key {
        for ext in ["jpg", "png"] {
            let custom = covers_dir().join(format!("{key}.{ext}"));
            if let Ok(data) = fs::read(&custom) {
                return Some((data, ext == "png"));
            }
        }
    }
    // 2. Steam user-set custom artwork. The portrait capsule lives at
    //    userdata/<id>/config/grid/<id>p.{png,jpg} — the ONLY place non-Steam
    //    shortcuts keep covers (keyed by the unsigned shortcut appid), and it
    //    overrides the official library cache when present.
    if !cover_id.is_empty() {
        for grid in steam_grid_dirs() {
            for ext in ["png", "jpg", "jpeg"] {
                let p = grid.join(format!("{cover_id}p.{ext}"));
                if let Ok(data) = fs::read(&p) {
                    return Some((data, ext == "png"));
                }
            }
        }
    }
    // 3. Official Steam library cache (real Steam apps only).
    let h = home();
    let candidates = [
        h.join(format!(".steam/steam/appcache/librarycache/{cover_id}/library_600x900.jpg")),
        h.join(format!(".local/share/Steam/appcache/librarycache/{cover_id}/library_600x900.jpg")),
    ];
    for path in &candidates {
        if let Ok(data) = fs::read(path) {
            return Some((data, false));
        }
    }
    None
}

/// Cover art for a game as a `data:` URL — a user-set custom cover (keyed by
/// `game_key`) first, then Steam's library cache. `Err` if none found.
pub fn game_cover(app_id: &str, game_key: Option<&str>) -> Result<String, String> {
    match game_cover_bytes(app_id, game_key) {
        Some((data, is_png)) => Ok(data_url(&data, if is_png { "png" } else { "jpeg" })),
        None => Err("cover not found".into()),
    }
}

/// Landscape "hero"/banner art for a game. Non-Steam: `grid/<id>_hero.*` (the
/// user-set wide banner) then `grid/<id>.*` (the header capsule). Steam:
/// `librarycache/<id>/library_hero.jpg` then `header.jpg`. Returns
/// `(bytes, is_png)`. Used for the launcher's hero banner.
pub fn game_hero_bytes(cover_id: &str) -> Option<(Vec<u8>, bool)> {
    if cover_id.is_empty() {
        return None;
    }
    // Steam user-set / non-Steam art (wide hero first, header capsule second).
    for grid in steam_grid_dirs() {
        for (name, is_png) in [
            (format!("{cover_id}_hero.png"), true),
            (format!("{cover_id}_hero.jpg"), false),
            (format!("{cover_id}.png"), true),
            (format!("{cover_id}.jpg"), false),
        ] {
            if let Ok(data) = fs::read(grid.join(&name)) {
                return Some((data, is_png));
            }
        }
    }
    // Official Steam library cache (real Steam apps).
    let h = home();
    for rel in [
        format!(".steam/steam/appcache/librarycache/{cover_id}/library_hero.jpg"),
        format!(".local/share/Steam/appcache/librarycache/{cover_id}/library_hero.jpg"),
        format!(".steam/steam/appcache/librarycache/{cover_id}/header.jpg"),
        format!(".local/share/Steam/appcache/librarycache/{cover_id}/header.jpg"),
    ] {
        if let Ok(data) = fs::read(h.join(&rel)) {
            return Some((data, false));
        }
    }
    None
}

/// The transparent logo/wordmark art for a game, to overlay on the hero banner
/// (SteamVR-style). Non-Steam: `grid/<id>_logo.png`. Steam: `librarycache/<id>/logo.png`.
pub fn game_logo_bytes(cover_id: &str) -> Option<(Vec<u8>, bool)> {
    if cover_id.is_empty() {
        return None;
    }
    for grid in steam_grid_dirs() {
        for (name, is_png) in [
            (format!("{cover_id}_logo.png"), true),
            (format!("{cover_id}_logo.jpg"), false),
        ] {
            if let Ok(data) = fs::read(grid.join(&name)) {
                return Some((data, is_png));
            }
        }
    }
    let h = home();
    for rel in [
        format!(".steam/steam/appcache/librarycache/{cover_id}/logo.png"),
        format!(".local/share/Steam/appcache/librarycache/{cover_id}/logo.png"),
    ] {
        if let Ok(data) = fs::read(h.join(&rel)) {
            return Some((data, true));
        }
    }
    None
}

pub fn set_custom_cover(game_key: &str, image_path: &str) -> std::io::Result<()> {
    let covers = covers_dir();
    fs::create_dir_all(&covers)?;
    let ext = Path::new(image_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .filter(|e| e == "png")
        .map(|_| "png")
        .unwrap_or("jpg");
    fs::copy(image_path, covers.join(format!("{game_key}.{ext}")))?;
    Ok(())
}

pub fn remove_custom_cover(game_key: &str) {
    for ext in ["jpg", "png"] {
        let _ = fs::remove_file(covers_dir().join(format!("{game_key}.{ext}")));
    }
}

fn data_url(data: &[u8], mime: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(data);
    format!("data:image/{mime};base64,{b64}")
}

// --- non-Steam shortcuts (artwork + launch) -----------------------------------

/// All Steam `userdata/<id>/config/grid` dirs — where Steam keeps user-set
/// custom artwork (and the only place a non-Steam shortcut's cover lives).
fn steam_grid_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root in find_steam_roots() {
        if let Ok(users) = fs::read_dir(root.join("userdata")) {
            for u in users.flatten() {
                let grid = u.path().join("config").join("grid");
                if grid.is_dir() {
                    dirs.push(grid);
                }
            }
        }
    }
    dirs
}

/// A non-Steam game the user added to Steam, from `shortcuts.vdf`.
#[derive(Debug, Clone)]
pub struct Shortcut {
    /// Unsigned 32-bit appid — names the compatdata folder and `grid/<id>p.*`.
    pub app_id: u32,
    pub app_name: String,
    pub exe: String,
    pub start_dir: String,
    /// Unix timestamp last played (`LastPlayTime`), 0 if never.
    pub last_play: u32,
}

/// Parse every user's `shortcuts.vdf` (binary VDF). Forgiving: we scan for the
/// `appid` (int32) markers and read the string fields that follow each, which is
/// all we need to map a game to its artwork + a launch id. Steam writes the
/// fields in a stable order (appid first), so per-entry association is reliable.
pub fn parse_shortcuts() -> Vec<Shortcut> {
    let mut out = Vec::new();
    for root in find_steam_roots() {
        if let Ok(users) = fs::read_dir(root.join("userdata")) {
            for u in users.flatten() {
                let vdf = u.path().join("config").join("shortcuts.vdf");
                if let Ok(data) = fs::read(&vdf) {
                    out.extend(parse_shortcuts_bytes(&data));
                }
            }
        }
    }
    out
}

fn parse_shortcuts_bytes(data: &[u8]) -> Vec<Shortcut> {
    const APPID: &[u8] = b"\x02appid\x00";
    let mut starts = Vec::new();
    let mut i = 0;
    while let Some(p) = find_bytes(&data[i..], APPID) {
        starts.push(i + p);
        i += p + APPID.len();
    }
    let mut shortcuts = Vec::new();
    for (n, &s) in starts.iter().enumerate() {
        let val = s + APPID.len();
        if val + 4 > data.len() {
            continue;
        }
        let app_id = u32::from_le_bytes([data[val], data[val + 1], data[val + 2], data[val + 3]]);
        let end = starts.get(n + 1).copied().unwrap_or(data.len());
        let seg = &data[val + 4..end];
        shortcuts.push(Shortcut {
            app_id,
            app_name: vdf_str(seg, b"\x01AppName\x00").unwrap_or_default(),
            exe: vdf_str(seg, b"\x01Exe\x00").unwrap_or_default(),
            start_dir: vdf_str(seg, b"\x01StartDir\x00").unwrap_or_default(),
            last_play: vdf_i32(seg, b"\x02LastPlayTime\x00").unwrap_or(0),
        });
    }
    shortcuts
}

/// Read a null-terminated VDF string field value (the bytes after `key`).
fn vdf_str(seg: &[u8], key: &[u8]) -> Option<String> {
    let k = find_bytes(seg, key)? + key.len();
    let end = seg[k..].iter().position(|&b| b == 0)? + k;
    Some(String::from_utf8_lossy(&seg[k..end]).into_owned())
}

/// Read a little-endian int32 VDF field value (the 4 bytes after `key`).
fn vdf_i32(seg: &[u8], key: &[u8]) -> Option<u32> {
    let k = find_bytes(seg, key)? + key.len();
    let b = seg.get(k..k + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

fn norm_path(p: &str) -> String {
    p.trim().trim_matches('"').trim_end_matches('/').to_string()
}

/// Resolve a custom (non-Steam) game's shortcut by matching its install dir or
/// folder name against `shortcuts.vdf`.
fn match_shortcut<'a>(game: &DetectedGame, shortcuts: &'a [Shortcut]) -> Option<&'a Shortcut> {
    let gp = norm_path(&game.game_path);
    // Primary: the shortcut's start dir / exe sits at or under the game dir.
    if !gp.is_empty() {
        if let Some(s) = shortcuts.iter().find(|s| {
            let sd = norm_path(&s.start_dir);
            let exe = norm_path(&s.exe);
            (!sd.is_empty() && (sd == gp || sd.starts_with(&gp) || gp.starts_with(&sd)))
                || (!exe.is_empty() && exe.starts_with(&format!("{gp}/")))
        }) {
            return Some(s);
        }
    }
    // Fallback: folder name matches the shortcut name / exe basename (".exe" off).
    let name = game.name.to_lowercase();
    shortcuts.iter().find(|s| {
        let an = s.app_name.trim_end_matches(".exe").to_lowercase();
        let base = Path::new(s.exe.trim_matches('"'))
            .file_stem()
            .map(|b| b.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        (!an.is_empty() && an == name) || (!base.is_empty() && base == name)
    })
}

// --- discovery internals (verbatim from xrbind, dirs -> paths) ----------------

fn find_steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let h = home();
    let candidates = [
        h.join(".steam/steam"),
        h.join(".local/share/Steam"),
        h.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];
    for c in candidates {
        let resolved = fs::canonicalize(&c).unwrap_or(c.clone());
        if resolved.is_dir() && !roots.contains(&resolved) {
            roots.push(resolved);
        }
    }
    roots
}

fn find_library_folders(steam_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut library_paths: Vec<PathBuf> = Vec::new();
    for root in steam_roots {
        if !library_paths.contains(root) {
            library_paths.push(root.clone());
        }
        let vdf_path = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(content) = fs::read_to_string(&vdf_path) {
            if let Some(paths) = parse_library_folders_vdf(&content) {
                for p in paths {
                    let pb = PathBuf::from(&p);
                    let resolved = fs::canonicalize(&pb).unwrap_or(pb);
                    if resolved.is_dir() && !library_paths.contains(&resolved) {
                        library_paths.push(resolved);
                    }
                }
            }
        }
    }
    library_paths
}

fn parse_library_folders_vdf(content: &str) -> Option<Vec<String>> {
    // Each library is a `"path"  "<dir>"` line. Split on quotes so a line like
    // `\t\t"path"\t\t"/run/media/.../SteamLibrary"` yields ["", "path", "\t\t",
    // "<dir>", ""] — parts[1]=="path", parts[3]==the directory. (Robust, and
    // doesn't depend on a VDF deserializer choking on the nested `apps` blocks.)
    let mut paths = Vec::new();
    for line in content.lines() {
        let parts: Vec<&str> = line.split('"').collect();
        if parts.len() >= 4 && parts[1] == "path" {
            paths.push(parts[3].to_string());
        }
    }
    (!paths.is_empty()).then_some(paths)
}

fn collect_app_names(
    steamapps: &Path,
    map: &mut HashMap<String, String>,
    last: &mut HashMap<String, u64>,
) {
    if let Ok(entries) = fs::read_dir(steamapps) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("appmanifest_") && name.ends_with(".acf") {
                let app_id = name
                    .strip_prefix("appmanifest_")
                    .and_then(|s| s.strip_suffix(".acf"))
                    .unwrap_or("")
                    .to_string();
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Some(app_name) = extract_vdf_value(&content, "name") {
                        map.insert(app_id.clone(), app_name);
                    }
                    if let Some(ts) = extract_vdf_value(&content, "LastPlayed").and_then(|s| s.parse().ok()) {
                        last.insert(app_id, ts);
                    }
                }
            }
        }
    }
}

fn extract_vdf_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let parts: Vec<&str> = line.trim().split('"').collect();
        if parts.len() >= 4 && parts[1] == key {
            return Some(parts[3].to_string());
        }
    }
    None
}

/// Find a game's `actions.json` files by probing the well-known SteamVR/Unity
/// locations instead of deep-walking the whole game tree. This keeps the scan
/// fast and avoids hanging on slow/FUSE filesystems (e.g. NTFS Steam libraries),
/// where stat-ing every file in `*_Data/StreamingAssets` is catastrophic.
fn find_game_actions(game_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    // Directly in the game folder.
    let direct = game_dir.join("actions.json");
    if direct.is_file() {
        found.push(direct);
    }
    // One level down (incl. the Unity `<*_Data>/StreamingAssets/...` layout).
    if let Ok(entries) = fs::read_dir(game_dir) {
        for entry in entries.flatten() {
            let sub = entry.path();
            let sa = sub.join("StreamingAssets");
            let candidates = [
                sub.join("actions.json"),
                sa.join("actions.json"),
                sa.join("SteamVR").join("actions.json"),
                sa.join("input").join("actions.json"),
                sa.join("bindings").join("actions.json"),
            ];
            for cand in candidates {
                if cand.is_file() {
                    found.push(cand);
                }
            }
        }
    }
    found
}

fn scan_directory_for_bindings(
    dir: &Path,
    app_name_map: &HashMap<String, String>,
    source: &str,
    games: &mut Vec<DetectedGame>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let game_dir = entry.path();
        let game_folder_name = entry.file_name().to_string_lossy().to_string();

        let found_actions = find_game_actions(&game_dir);
        for actions_path in &found_actions {
            if let Some(game) = try_load_actions_manifest(
                actions_path,
                &game_folder_name,
                app_name_map,
                source,
                Some(&game_dir),
            ) {
                games.push(game);
            }
        }

        let xrizer_dir = game_dir.join("xrizer");
        if xrizer_dir.is_dir() {
            let has_xrizer_actions = found_actions.iter().any(|p| p.starts_with(&xrizer_dir));
            if !has_xrizer_actions {
                let main_actions = found_actions.iter().find(|p| !p.starts_with(&xrizer_dir)).cloned();
                if let Some(actions_path) = main_actions {
                    if let Some(game) =
                        try_load_xrizer_override(&xrizer_dir, &actions_path, &game_folder_name, app_name_map)
                    {
                        games.push(game);
                    }
                }
            }
        }
    }
}

fn try_load_xrizer_override(
    xrizer_dir: &Path,
    actions_path: &Path,
    fallback_name: &str,
    app_name_map: &HashMap<String, String>,
) -> Option<DetectedGame> {
    let mut binding_files = Vec::new();
    if let Ok(entries) = fs::read_dir(xrizer_dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.ends_with(".json") && fname != "actions.json" {
                let without_prefix = fname
                    .strip_prefix("bindings_")
                    .or_else(|| fname.strip_prefix("binding_"))
                    .unwrap_or(&fname);
                let controller_type = without_prefix.strip_suffix(".json").unwrap_or(without_prefix).to_string();
                binding_files.push(BindingFile {
                    controller_type,
                    file_path: entry.path().to_string_lossy().to_string(),
                    file_name: fname,
                });
            }
        }
    }
    if binding_files.is_empty() {
        return None;
    }
    let app_id = extract_app_id_from_path(actions_path);
    let display_name = app_id
        .as_ref()
        .and_then(|id| app_name_map.get(id))
        .cloned()
        .unwrap_or_else(|| fallback_name.to_string());
    let game_path = xrizer_dir
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| xrizer_dir.to_string_lossy().to_string());
    Some(DetectedGame {
        name: display_name,
        app_id,
        game_path,
        actions_path: actions_path.to_string_lossy().to_string(),
        binding_files,
        source: "xrizer (game override)".to_string(),
        shortcut_id: None,
        last_played: None,
    })
}

fn scan_compatdata(compatdata: &Path, app_name_map: &HashMap<String, String>, games: &mut Vec<DetectedGame>) {
    let Ok(entries) = fs::read_dir(compatdata) else {
        return;
    };
    for entry in entries.flatten() {
        let app_id = entry.file_name().to_string_lossy().to_string();
        let pfx = entry.path().join("pfx").join("drive_c");
        if !pfx.is_dir() {
            continue;
        }
        let game_name = app_name_map.get(&app_id).cloned().unwrap_or_else(|| format!("App {app_id}"));
        for walker_entry in WalkDir::new(&pfx).max_depth(6).into_iter().filter_map(|e| e.ok()) {
            if walker_entry.file_name() == "actions.json" {
                if let Some(mut game) =
                    try_load_actions_manifest(walker_entry.path(), &game_name, app_name_map, "steam (proton)", None)
                {
                    game.app_id = Some(app_id.clone());
                    games.push(game);
                }
            }
        }
    }
}

fn scan_xrizer_configs(games: &mut Vec<DetectedGame>) {
    let xrizer_paths = [
        config_home().join("openxr").join("1").join("xrizer"),
        config_home().join("xrizer"),
    ];
    for xrizer_dir in &xrizer_paths {
        if !xrizer_dir.is_dir() {
            continue;
        }
        for walker_entry in WalkDir::new(xrizer_dir).max_depth(3).into_iter().filter_map(|e| e.ok()) {
            if walker_entry.file_name() == "actions.json" {
                let parent_name = walker_entry
                    .path()
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "xrizer config".to_string());
                if let Some(game) =
                    try_load_actions_manifest(walker_entry.path(), &parent_name, &HashMap::new(), "xrizer", None)
                {
                    games.push(game);
                }
            }
        }
    }
}

fn try_load_actions_manifest(
    actions_path: &Path,
    fallback_name: &str,
    app_name_map: &HashMap<String, String>,
    source: &str,
    game_root: Option<&Path>,
) -> Option<DetectedGame> {
    let content = fs::read_to_string(actions_path).ok()?;
    let manifest: ActionsManifest = serde_json::from_str(&content).ok()?;
    let actions_dir = actions_path.parent()?;

    let mut binding_files = Vec::new();
    for db in &manifest.default_bindings {
        let binding_path = actions_dir.join(&db.binding_url);
        if binding_path.exists() {
            binding_files.push(BindingFile {
                controller_type: db.controller_type.clone(),
                file_path: binding_path.to_string_lossy().to_string(),
                file_name: db.binding_url.clone(),
            });
        }
    }
    if binding_files.is_empty() {
        return None;
    }

    let app_id = extract_app_id_from_path(actions_path);
    let display_name = app_id
        .as_ref()
        .and_then(|id| app_name_map.get(id))
        .cloned()
        .unwrap_or_else(|| fallback_name.to_string());
    let game_path = game_root.unwrap_or(actions_dir).to_string_lossy().to_string();

    Some(DetectedGame {
        name: display_name,
        app_id,
        game_path,
        actions_path: actions_path.to_string_lossy().to_string(),
        binding_files,
        source: source.to_string(),
        shortcut_id: None,
        last_played: None,
    })
}

fn extract_app_id_from_path(path: &Path) -> Option<String> {
    let path_str = path.to_string_lossy();
    if let Some(idx) = path_str.find("compatdata/") {
        let after = &path_str[idx + 11..];
        let id: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !id.is_empty() {
            return Some(id);
        }
    }
    if let Some(idx) = path_str.find("steamapps/common/") {
        let after = &path_str[idx + 17..];
        let folder_name: String = after.chars().take_while(|&c| c != '/').collect();
        if !folder_name.is_empty() {
            let steamapps = Path::new(&path_str[..idx + 9]);
            if let Ok(entries) = fs::read_dir(steamapps) {
                for entry in entries.flatten() {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if fname.starts_with("appmanifest_") && fname.ends_with(".acf") {
                        if let Ok(content) = fs::read_to_string(entry.path()) {
                            if let Some(installdir) = extract_vdf_value(&content, "installdir") {
                                if installdir == folder_name {
                                    let app_id = fname
                                        .strip_prefix("appmanifest_")
                                        .and_then(|s| s.strip_suffix(".acf"))
                                        .unwrap_or("")
                                        .to_string();
                                    if !app_id.is_empty() {
                                        return Some(app_id);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}
