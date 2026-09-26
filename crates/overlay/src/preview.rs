//! `--toast-preview [dir]`: renders the toast cards, the watch, the bottom bar
//! and dashboard pages to PNGs with a tiny software rasteriser — real egui
//! layout and tessellation, no GPU or headset — so the design can be checked
//! (and screenshotted) outside VR. `MONADECK_PREVIEW_ONLY=<text>` keeps only the
//! shots whose name contains it.
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::gfx::{apply_style, theme, PPP};
use crate::toast::{Kind, Source, Toast, Toasts};

/// The toast panel's pixel size (see `make_panel` in main).
const PX: (usize, usize) = (960, 280);
/// The minimal watch's.
const MINI_PX: (usize, usize) = (420, 160);
/// The dashboard's bottom bar.
const BOTTOM_PX: (usize, usize) = (1640, 151);
/// The launch popup.
const LAUNCH_PX: (usize, usize) = (840, 480);
/// The dashboard's main panel, and a tall canvas for whole scrolling pages.
const MAIN_PX: (usize, usize) = (2000, 1250);
const TALL_PX: (usize, usize) = (2000, 4600);
/// The wrist watch.
const WATCH_PX: (usize, usize) = (600, 404);
/// The nav rail.
const RAIL_PX: (usize, usize) = (162, 1140);

/// (name, toast, extra queued, frames as (suffix, seconds since shown)).
type Case = (&'static str, Toast, usize, Vec<(&'static str, f32)>);

struct Tex {
    size: [usize; 2],
    px: Vec<egui::Color32>,
}

fn wanted(name: &str) -> bool {
    std::env::var("MONADECK_PREVIEW_ONLY").map_or(true, |f| name.contains(&f))
}

pub fn run(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let ctx = egui::Context::default();
    crate::gfx::install_fonts(&ctx);
    apply_style(&ctx);
    ctx.set_pixels_per_point(PPP);
    if std::env::var_os("MONADECK_PREVIEW_NOFEATHER").is_some() {
        ctx.tessellation_options_mut(|o| o.feathering = false);
    }
    let mut textures: HashMap<egui::TextureId, Tex> = HashMap::new();
    // egui applies the pixel scale on its first pass (that frame lays out on a
    // default 10000-point screen): warm the context up like the overlay does.
    let out = ctx.run(screen_input(PX, 0.0), |_| {});
    for (id, delta) in &out.textures_delta.set {
        apply_delta(&mut textures, *id, delta);
    }

    // A real app icon if the system has one (exercises the lookup), else a
    // brand-coloured stand-in.
    let icon = ["discord", "org.telegram.desktop", "firefox", "fluxer", "heroic", "kate"]
        .iter()
        .find_map(|n| crate::notifications::icon_by_name(n))
        .unwrap_or_else(sample_icon);

    let full = |t: &Toast| crate::toast::ENTER + t.secs + crate::toast::EXIT;
    let cases: Vec<Case> = vec![
        (
            "notification-desktop",
            Toast::new(Kind::Notification, "Eidenz", "hey, are you coming tonight? we're starting the raid at 9 and could really use a healer")
                .app("Discord")
                .source(Source::Desktop)
                .icon(Some(icon.clone())),
            0,
            vec![("line", 0.07), ("opening", 0.2), ("open", 1.6), ("late", 4.9), ("folding", 5.42)],
        ),
        (
            "notification-queued",
            Toast::new(Kind::Notification, "Eidenz", "and bring snacks").app("Discord").source(Source::Desktop).icon(Some(icon.clone())),
            2,
            vec![("open", 1.0)],
        ),
        (
            "notification-xso",
            Toast::new(Kind::Notification, "Friend online", "Nyx is now online on VRChat").app("VRCX").source(Source::XsOverlay),
            0,
            vec![("open", 1.6)],
        ),
        ("timer", Toast::new(Kind::Timer, "Timer finished", "The countdown is over"), 0, vec![("open", 1.6)]),
        ("battery", Toast::new(Kind::Battery, "Low battery", "Controller at 12%"), 0, vec![("open", 1.6)]),
        (
            "warning",
            Toast::new(
                Kind::Warning,
                "Launch it in Steam first",
                "Stray has no Proton prefix yet. Force Proton in its Steam properties and run it once, then VR Mod will work.",
            ),
            0,
            vec![("open", 1.6)],
        ),
        ("info", Toast::new(Kind::Info, "No screen selected", "Show screens from the watch or the bottom bar"), 0, vec![("open", 1.6)]),
        ("confirm", Toast::new(Kind::Confirm, "Layout “Standing” saved", ""), 0, vec![("open", 1.0)]),
        (
            "welcome",
            Toast::new(Kind::Welcome, "Monadeck is ready", "Left system button opens the dashboard · Settings › Controllers lists every gesture"),
            0,
            vec![("open", 1.6)],
        ),
    ];

    let pose = openxr::Posef::IDENTITY;
    for (name, toast, extra, frames) in cases {
        if !wanted(name) {
            continue;
        }
        let mut toasts = Toasts::new();
        let lifetime = full(&toast);
        toasts.push(toast);
        for i in 0..extra {
            toasts.push(Toast::new(Kind::Notification, format!("queued {i}"), ""));
        }
        let start = Instant::now();
        toasts.tick(start, Some(&pose));
        for (suffix, secs) in frames {
            let secs = secs.min(lifetime - 0.001);
            let now = start + Duration::from_secs_f32(secs);
            let queued = toasts.update(&ctx, now, None).map_or(0, |(_, q)| q);
            let out = ctx.run(screen_input(PX, secs as f64), |ctx| toasts.draw(ctx, now, queued));
            for (id, delta) in &out.textures_delta.set {
                apply_delta(&mut textures, *id, delta);
            }
            let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
            let img = rasterise(&prims, &textures, out.pixels_per_point, PX);
            let path = dir.join(format!("{name}-{suffix}.png"));
            img.save(&path)?;
            println!("{}", path.display());
            for id in &out.textures_delta.free {
                textures.remove(id);
            }
        }
    }
    // The readout: updated in place, so drawn from its own manager.
    let mut toasts = Toasts::new();
    let start = Instant::now();
    toasts.readout("Screen 1", "1.20 m  ·  2.1 m away  ·  curve 30°", pose);
    let out = ctx.run(screen_input(PX, 0.5), |ctx| toasts.draw(ctx, start + Duration::from_millis(500), 0));
    for (id, delta) in &out.textures_delta.set {
        apply_delta(&mut textures, *id, delta);
    }
    let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
    let path = dir.join("readout-open.png");
    rasterise(&prims, &textures, out.pixels_per_point, PX).save(&path)?;
    println!("{}", path.display());
    // The minimal watch (its own, smaller panel), idle and pointed at.
    let mut st = crate::ui::LibState::new();
    st.clock = "9:41 PM".into();
    st.notif_unseen = 2;
    st.watch_date = "Thursday, September 24".into();
    st.batteries = sample_batteries();
    for (name, hot) in [("watch-mini", false), ("watch-mini-hot", true)] {
        let out = ctx.run(screen_input(MINI_PX, 1.0), |ctx| crate::ui::build_watch_mini(ctx, &st, hot));
        for (id, delta) in &out.textures_delta.set {
            apply_delta(&mut textures, *id, delta);
        }
        let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
        let path = dir.join(format!("{name}.png"));
        rasterise(&prims, &textures, out.pixels_per_point, MINI_PX).save(&path)?;
        println!("{}", path.display());
    }
    // The launch popup: loading, then the outcome it shows before closing.
    for (name, status, glyph) in [
        ("launch-loading", "Loading…", crate::ui::LaunchGlyph::Spinner),
        ("launch-done", "", crate::ui::LaunchGlyph::Done),
        ("launch-failed", "Failed to launch", crate::ui::LaunchGlyph::Failed),
    ] {
        let hero = crate::games::ArtState::Missing;
        let out = ctx.run(screen_input(LAUNCH_PX, 1.0), |ctx| crate::ui::build_launch_popup(ctx, "VaM VR", status, &hero, glyph));
        for (id, delta) in &out.textures_delta.set {
            apply_delta(&mut textures, *id, delta);
        }
        let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
        let path = dir.join(format!("{name}.png"));
        rasterise(&prims, &textures, out.pixels_per_point, LAUNCH_PX).save(&path)?;
        println!("{}", path.display());
    }
    // The dashboard's bottom bar with three screens up, under a growing pile of
    // devices: the batteries must fold into per-kind chips before they reach
    // the screen pills.
    use crate::monado::{BatteryInfo, BatteryKind, DevState, Hand};
    let bat = |kind, charge| BatteryInfo { kind, charge, charging: false, state: DevState::Live, hand: None };
    let with = |b: BatteryInfo, state| BatteryInfo { state, ..b };
    let hand = |b: BatteryInfo, hand| BatteryInfo { hand: Some(hand), ..b };
    // A controller switched off (grey "off"), a tracker out of sight (faded).
    let states = vec![
        hand(bat(BatteryKind::Controller, 0.86), Hand::Left),
        hand(with(bat(BatteryKind::Controller, 0.40), DevState::Off), Hand::Right),
        with(bat(BatteryKind::Tracker, 0.67), DevState::Lost),
        bat(BatteryKind::Glove, 0.80),
        bat(BatteryKind::Glove, 0.60),
    ];
    let rigs: [(&str, Vec<BatteryInfo>); 4] = [
        ("bottom-bar-states", states.clone()),
        ("bottom-bar-controllers", vec![hand(bat(BatteryKind::Controller, 0.86), Hand::Left), hand(bat(BatteryKind::Controller, 0.83), Hand::Right)]),
        (
            "bottom-bar-five-devices",
            vec![
                hand(bat(BatteryKind::Controller, 0.86), Hand::Left),
                hand(bat(BatteryKind::Controller, 0.83), Hand::Right),
                bat(BatteryKind::Tracker, 0.67),
                bat(BatteryKind::Glove, 0.72),
                bat(BatteryKind::Other, 0.65),
            ],
        ),
        (
            "bottom-bar-full-body",
            vec![
                hand(bat(BatteryKind::Controller, 0.86), Hand::Left),
                hand(bat(BatteryKind::Controller, 0.83), Hand::Right),
                bat(BatteryKind::Tracker, 0.67),
                bat(BatteryKind::Tracker, 0.41),
                bat(BatteryKind::Tracker, 0.12),
                bat(BatteryKind::Glove, 0.72),
                bat(BatteryKind::Glove, 0.70),
                bat(BatteryKind::Other, 0.65),
            ],
        ),
    ];
    for (name, batteries) in rigs {
        if !wanted(name) {
            continue;
        }
        let mut st = crate::ui::LibState::new();
        st.clock = "10:45 AM".into();
        st.desktop_bar = vec![("DP-1".into(), true), ("DP-2".into(), false), ("HDMI-A-1".into(), false)];
        st.batteries = batteries;
        let out = ctx.run(screen_input(BOTTOM_PX, 1.0), |ctx| crate::ui::build_bottom(ctx, &mut st));
        for (id, delta) in &out.textures_delta.set {
            apply_delta(&mut textures, *id, delta);
        }
        let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
        let path = dir.join(format!("{name}.png"));
        rasterise(&prims, &textures, out.pixels_per_point, BOTTOM_PX).save(&path)?;
        println!("{}", path.display());
    }
    // The wrist watch with the same devices: live, off and out of sight.
    if wanted("watch-states") {
        let mut st = sample_state();
        st.batteries = states;
        let img = shoot(&ctx, &mut textures, WATCH_PX, 3, |ctx| crate::ui::build_watch(ctx, &mut st));
        let path = dir.join("watch-states.png");
        img.save(&path)?;
        println!("{}", path.display());
    }
    pages(&ctx, &mut textures, dir)?;
    dashboard(&ctx, &mut textures, dir)?;
    // Only on request: reads your Steam library's art.
    if std::env::var("MONADECK_PREVIEW_ONLY").is_ok_and(|f| f.contains("readme")) {
        readme(&ctx, &mut textures, dir)?;
    }
    Ok(())
}

/// `MONADECK_PREVIEW_ONLY=readme`: the README's shots from real renders, with
/// a headset and two controllers and nothing running: the watch, the Home
/// dashboard with your Steam library's art, the playspace tools, and the
/// layers of the screenshots shot (background, photo window, watch with a
/// new-screenshot card) for compositing.
fn readme(ctx: &egui::Context, textures: &mut HashMap<egui::TextureId, Tex>, dir: &Path) -> Result<()> {
    use crate::ui::{Nav, SystemTab};
    let games = readme_games(ctx);
    let state = |nav: Nav| {
        let mut st = sample_state();
        st.games = games.iter().map(clone_game).collect();
        st.collections = vec!["Chill".into(), "With friends".into()];
        st.games[0].collections = vec![0, 1];
        st.selected = Some(0);
        st.running_index = None;
        st.nav = nav;
        use crate::monado::{BatteryInfo, BatteryKind, DevState, Hand};
        st.batteries = [(0.86, Hand::Left), (0.83, Hand::Right)]
            .map(|(charge, hand)| BatteryInfo { kind: BatteryKind::Controller, charge, charging: false, state: DevState::Live, hand: Some(hand) })
            .to_vec();
        for r in &mut st.desktop_rows {
            r.shown = false;
        }
        st.desktop_bar = st.desktop_rows.iter().map(|r| (r.name.clone(), false)).collect();
        st.desktop_shown = 0;
        st.watch_buttons = ["keyboard", "recenter", "letgo", "screenshot"].map(String::from).to_vec();
        st.watch_date = "Thursday, September 24".into();
        st.clock = "10:45 PM".into();
        st
    };
    let save = |img: &image::RgbaImage, name: &str| -> Result<()> {
        let path = dir.join(name);
        img.save(&path)?;
        println!("{}", path.display());
        Ok(())
    };

    // The watch, floating (transparent around it).
    let mut st = state(Nav::Home);
    save(&shoot_over(ctx, textures, WATCH_PX, 6, true, |ctx| crate::ui::build_watch(ctx, &mut st)), "readme-watch.png")?;

    // Home, and Home with the rail and the bottom bar as they hang in VR.
    let mut st = state(Nav::Home);
    let main = shoot(ctx, textures, MAIN_PX, 12, |ctx| crate::ui::build_main(ctx, &mut st));
    save(&main, "readme-dash-home.png")?;
    let mut st = state(Nav::Home);
    let rail = shoot_over(ctx, textures, RAIL_PX, 6, true, |ctx| crate::ui::build_rail(ctx, &mut st));
    let mut st = state(Nav::Home);
    let bottom = shoot_over(ctx, textures, BOTTOM_PX, 6, true, |ctx| crate::ui::build_bottom(ctx, &mut st));
    let dash = composite(&rail, &main, &bottom, None);
    save(&dash, "readme-dashboard.png")?;

    // The playspace tools, flat and with the rail and bar.
    let playspace = |ctx: &egui::Context, st: &mut crate::ui::LibState| {
        st.system_tab = SystemTab::Playspace;
        crate::ui::build_main(ctx, st)
    };
    let mut st = state(Nav::System);
    let ps_main = shoot(ctx, textures, MAIN_PX, 12, |ctx| playspace(ctx, &mut st));
    save(&ps_main, "readme-playspace.png")?;
    let mut st = state(Nav::System);
    let ps_rail = shoot_over(ctx, textures, RAIL_PX, 6, true, |ctx| crate::ui::build_rail(ctx, &mut st));
    let ps_dash = composite(&ps_rail, &ps_main, &bottom, None);

    // The screenshots shot's layers. The background is a view of the built-in
    // 360° panorama; the "screenshot" is that view with the dashboard in it.
    let sky = image::open(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/sky/table_mountain_2.jpg"))?.to_rgba8();
    let view = |yaw: f32, fov: f32, w: u32, h: u32| -> image::RgbaImage {
        // A rectilinear view out of the equirect panorama (yaw in turns).
        let (sw, sh) = (sky.width() as f32, sky.height() as f32);
        let f = (w as f32 / 2.0) / (fov.to_radians() / 2.0).tan();
        image::RgbaImage::from_fn(w, h, |x, y| {
            let (dx, dy) = (x as f32 - w as f32 / 2.0, y as f32 - h as f32 / 2.0 - h as f32 * 0.08);
            let lon = yaw * std::f32::consts::TAU + dx.atan2(f);
            let lat = -dy.atan2((dx * dx + f * f).sqrt());
            let u = (lon / std::f32::consts::TAU).rem_euclid(1.0) * (sw - 1.0);
            let v = (0.5 - lat / std::f32::consts::PI).clamp(0.0, 1.0) * (sh - 1.0);
            *sky.get_pixel(u as u32, v as u32)
        })
    };
    save(&view(0.50, 88.0, 1800, 1150), "readme-layer-bg.png")?;
    // Panels over the panorama as they'd hang in front of you; the README's
    // crop of it is 1200×805 at (204, 160).
    let in_vr = |panels: &image::RgbaImage| {
        let mut shot = view(0.47, 80.0, 1600, 1000);
        let small = image::imageops::resize(panels, 1120, (1120.0 * panels.height() as f32 / panels.width() as f32) as u32, image::imageops::FilterType::Triangle);
        image::imageops::overlay(&mut shot, &small, ((1600 - small.width()) / 2) as i64, 190);
        shot
    };
    let readme_crop = |shot: &image::RgbaImage| image::DynamicImage::ImageRgba8(image::imageops::crop_imm(shot, 204, 160, 1200, 805).to_image()).to_rgb8();
    let shot = in_vr(&dash);
    save(&shot, "readme-layer-shot.png")?;
    let jpg = |img: &image::RgbImage, name: &str| -> Result<()> {
        let path = dir.join(name);
        let mut out = std::fs::File::create(&path)?;
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92).encode_image(img)?;
        println!("{}", path.display());
        Ok(())
    };
    jpg(&readme_crop(&shot), "readme-dashboard-vr.jpg")?;
    jpg(&readme_crop(&in_vr(&ps_dash)), "readme-playspace-vr.jpg")?;
    let shot_tex = ctx.load_texture(
        "readme-shot",
        egui::ColorImage::from_rgba_unmultiplied([shot.width() as usize, shot.height() as usize], shot.as_raw()),
        egui::TextureOptions::LINEAR,
    );
    let photo_px = (crate::photos::WINDOW_PX.0 as usize, crate::photos::WINDOW_PX.1 as usize);
    let window = shoot_over(ctx, textures, photo_px, 6, true, |ctx| crate::photos::preview_window(ctx, shot_tex.clone(), "2026/09/24  22:49:35", true, true));
    save(&window, "readme-layer-window.png")?;
    let mut st = state(Nav::Home);
    st.wrist_shot = Some(crate::ui::WristShot { thumb: Some(shot_tex.clone()), qr: None, when: "2026/09/24 22:49:51".into(), idx: 0, total: 1 });
    save(&shoot_over(ctx, textures, WATCH_PX, 6, true, |ctx| crate::ui::build_watch(ctx, &mut st)), "readme-layer-watch.png")?;
    Ok(())
}

/// Rail + main panel + bottom bar at one physical scale (the bar's 1640 px
/// span the main panel's 2000 in VR), on `bg` or transparent.
fn composite(rail: &image::RgbaImage, main: &image::RgbaImage, bottom: &image::RgbaImage, bg: Option<image::Rgba<u8>>) -> image::RgbaImage {
    let bw = MAIN_PX.0 as u32;
    let bh = (BOTTOM_PX.1 as f32 * MAIN_PX.0 as f32 / BOTTOM_PX.0 as f32) as u32;
    let bottom = image::imageops::resize(bottom, bw, bh, image::imageops::FilterType::Triangle);
    let gap = 36u32;
    let (w, h) = (RAIL_PX.0 as u32 + gap + MAIN_PX.0 as u32, MAIN_PX.1 as u32 + gap + bh);
    let mut canvas = image::RgbaImage::from_pixel(w, h, bg.unwrap_or(image::Rgba([0, 0, 0, 0])));
    image::imageops::overlay(&mut canvas, rail, 0, ((MAIN_PX.1 - RAIL_PX.1) / 2) as i64);
    image::imageops::overlay(&mut canvas, main, (RAIL_PX.0 as u32 + gap) as i64, 0);
    image::imageops::overlay(&mut canvas, &bottom, (RAIL_PX.0 as u32 + gap) as i64, (MAIN_PX.1 as u32 + gap) as i64);
    canvas
}

/// Games from your Steam library with their real art (Steam's own cache has
/// it for owned games, installed or not); ones without art are skipped.
fn readme_games(ctx: &egui::Context) -> Vec<crate::games::LibGame> {
    use crate::games::{ArtState, LibGame};
    use monadeck_core::steam;
    const PICKS: [(&str, &str); 10] = [
        ("VRChat", "438100"),
        ("Half-Life: Alyx", "546560"),
        ("Beat Saber", "620980"),
        ("Pistol Whip", "1079800"),
        ("BONELAB", "1592190"),
        ("The Midnight Walk", "2863640"),
        ("Blade & Sorcery", "629730"),
        ("Phasmophobia", "739630"),
        ("Resonite", "2519830"),
        ("BONEWORKS", "823500"),
    ];
    let load = |bytes: Option<(Vec<u8>, bool)>, key: String, max_w: u32| -> ArtState {
        let Some(img) = bytes.and_then(|(b, _)| image::load_from_memory(&b).ok()) else {
            return ArtState::Missing;
        };
        let img = if img.width() > max_w { img.resize(max_w, u32::MAX, image::imageops::FilterType::Triangle) } else { img };
        let rgba = img.to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
        ArtState::Ready(ctx.load_texture(key, ci, egui::TextureOptions::LINEAR))
    };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    PICKS
        .iter()
        .enumerate()
        .filter_map(|(i, (name, id))| {
            let cover = load(steam::game_cover_bytes(id, None), format!("{id}-cover"), 480);
            if matches!(cover, ArtState::Missing) {
                return None;
            }
            Some(LibGame {
                name: name.to_string(),
                app_id: Some(id.to_string()),
                shortcut_id: None,
                cover_id: Some(id.to_string()),
                source: "Steam".into(),
                exe: None,
                start_dir: None,
                uevr_capable: false,
                vr: true,
                pad_hidden_by_options: false,
                last_played: Some(now - (i as u64 * 7 + 1) * 3600 * 5),
                size_on_disk: Some((i as u64 + 2) * 2_900_000_000),
                playtime_minutes: Some((12 - i as u32) * 131),
                tracked_minutes: None,
                is_favorite: matches!(i, 0 | 2 | 6),
                uevr: false,
                collections: Vec::new(),
                cover,
                hero: load(steam::game_hero_bytes(id), format!("{id}-hero"), 1600),
                logo: load(steam::game_logo_bytes(id), format!("{id}-logo"), 700),
            })
        })
        .collect()
}

/// The game side of the dashboard (Home, Library, Collections, Favorites, the
/// running-game splash, the search keyboard), the rail, the watch in its
/// states, and a flat composite of rail + main + bottom bar as they hang in VR.
fn dashboard(ctx: &egui::Context, textures: &mut HashMap<egui::TextureId, Tex>, dir: &Path) -> Result<()> {
    use crate::ui::Nav;
    let games = sample_games(ctx);
    let dressed = |nav: Nav| {
        let mut st = sample_state();
        st.games = games.iter().map(clone_game).collect();
        st.collections = vec!["Chill".into(), "With friends".into()];
        for (i, g) in st.games.iter_mut().enumerate() {
            if i % 3 == 0 {
                g.collections.push(0);
            }
            if i % 4 == 1 {
                g.collections.push(1);
            }
        }
        st.selected = Some(0);
        st.running_index = Some(0);
        st.nav = nav;
        st.session_minutes = Some(42);
        st
    };
    type Setup = Box<dyn Fn(&mut crate::ui::LibState)>;
    let shots: Vec<(&str, Setup)> = vec![
        ("dash-home", Box::new(|_| {})),
        ("dash-home-idle", Box::new(|st| {
            st.running_index = None;
            st.selected = Some(2);
        })),
        ("dash-home-jp", Box::new(|st| {
            st.running_index = None;
            st.selected = Some(13);
        })),
        ("dash-library", Box::new(|st| st.nav = Nav::Library)),
        ("dash-collections", Box::new(|st| {
            st.nav = Nav::Library;
            st.library_grouped = true;
        })),
        ("dash-favorites", Box::new(|st| st.nav = Nav::Favorites)),
        ("dash-splash", Box::new(|st| st.show_splash = true)),
        ("dash-search", Box::new(|st| {
            st.search = "ha".into();
            st.keyboard_open = true;
        })),
    ];
    let mut home_main = None;
    for (name, setup) in shots {
        if !wanted(name) && !wanted("dashboard-composite") {
            continue;
        }
        let mut st = dressed(Nav::Home);
        setup(&mut st);
        let img = shoot(ctx, textures, MAIN_PX, 12, |ctx| crate::ui::build_main(ctx, &mut st));
        if name == "dash-home" {
            home_main = Some(img.clone());
        }
        if wanted(name) {
            let path = dir.join(format!("{name}.png"));
            img.save(&path)?;
            println!("{}", path.display());
        }
    }
    // The rail and the bottom bar, alone and in the composite.
    let mut st = dressed(Nav::Home);
    let rail = shoot(ctx, textures, RAIL_PX, 6, |ctx| crate::ui::build_rail(ctx, &mut st));
    let mut st = dressed(Nav::Home);
    st.batteries = sample_batteries();
    let bottom = shoot(ctx, textures, BOTTOM_PX, 6, |ctx| crate::ui::build_bottom(ctx, &mut st));
    if wanted("dash-rail") {
        let path = dir.join("dash-rail.png");
        rail.save(&path)?;
        println!("{}", path.display());
    }
    if wanted("dashboard-composite") {
        if let Some(main) = home_main {
            // Same physical scale for all three: the bar's 1640 px span the
            // main panel's 2000 px width in VR.
            let bw = MAIN_PX.0 as u32;
            let bh = (BOTTOM_PX.1 as f32 * MAIN_PX.0 as f32 / BOTTOM_PX.0 as f32) as u32;
            let bottom = image::imageops::resize(&bottom, bw, bh, image::imageops::FilterType::Triangle);
            let gap = 36u32;
            let (w, h) = (RAIL_PX.0 as u32 + gap + MAIN_PX.0 as u32, MAIN_PX.1 as u32 + gap + bh);
            let mut canvas = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 12, 15, 255]));
            image::imageops::overlay(&mut canvas, &rail, 0, ((MAIN_PX.1 - RAIL_PX.1) / 2) as i64);
            image::imageops::overlay(&mut canvas, &main, (RAIL_PX.0 as u32 + gap) as i64, 0);
            image::imageops::overlay(&mut canvas, &bottom, (RAIL_PX.0 as u32 + gap) as i64, (MAIN_PX.1 as u32 + gap) as i64);
            let path = dir.join("dashboard-composite.png");
            canvas.save(&path)?;
            println!("{}", path.display());
        }
    }
    // The watch: clock, the media card, notification history, gaming mode.
    let watch_shots: Vec<(&str, Setup)> = vec![
        ("watch-clock", Box::new(|_| {})),
        ("watch-media", Box::new(|st| {
            st.media = Some(crate::media::MediaState { player: "Spotify".into(), title: "Lofi hip hop radio".into(), artist: "Lofi Girl".into(), playing: true });
            st.watch_media_menu = true;
        })),
        ("watch-history", Box::new(|st| {
            st.notif_history = vec![
                ("Eidenz".into(), "are you coming tonight?".into(), "2m".into()),
                ("VRCX".into(), "Nyx is now online".into(), "14m".into()),
            ];
            st.watch_history_menu = true;
        })),
        ("watch-gaming", Box::new(|st| st.game_mode = true)),
        ("watch-unlocked", Box::new(|st| st.watch_locked = false)),
    ];
    for (name, setup) in watch_shots {
        if !wanted(name) {
            continue;
        }
        let mut st = dressed(Nav::Home);
        st.batteries = sample_batteries();
        st.watch_date = "Thursday, September 24".into();
        st.notif_unseen = 1;
        st.notif_history = vec![("Eidenz".into(), "are you coming tonight?".into(), "2m".into())];
        st.timer_running = true;
        st.timer_remaining = 272;
        st.timer_total = 300;
        setup(&mut st);
        let img = shoot(ctx, textures, WATCH_PX, 6, |ctx| crate::ui::build_watch(ctx, &mut st));
        let path = dir.join(format!("{name}.png"));
        img.save(&path)?;
        println!("{}", path.display());
    }
    Ok(())
}

fn sample_batteries() -> Vec<crate::monado::BatteryInfo> {
    use crate::monado::{BatteryInfo, BatteryKind, DevState, Hand};
    let b = |kind, charge, state| BatteryInfo { kind, charge, charging: false, state, hand: None };
    let c = |charge, hand| BatteryInfo { hand: Some(hand), ..b(BatteryKind::Controller, charge, DevState::Live) };
    vec![
        c(0.86, Hand::Left),
        c(0.64, Hand::Right),
        b(BatteryKind::Tracker, 0.41, DevState::Lost),
        b(BatteryKind::Glove, 0.80, DevState::Live),
        b(BatteryKind::Glove, 0.60, DevState::Off),
    ]
}

fn clone_game(g: &crate::games::LibGame) -> crate::games::LibGame {
    use crate::games::ArtState;
    let art = |a: &ArtState| match a {
        ArtState::Ready(t) => ArtState::Ready(t.clone()),
        ArtState::Missing => ArtState::Missing,
        ArtState::Pending => ArtState::Pending,
        ArtState::Idle => ArtState::Idle,
    };
    crate::games::LibGame {
        name: g.name.clone(),
        app_id: g.app_id.clone(),
        shortcut_id: g.shortcut_id.clone(),
        cover_id: g.cover_id.clone(),
        source: g.source.clone(),
        exe: g.exe.clone(),
        start_dir: g.start_dir.clone(),
        uevr_capable: g.uevr_capable,
        vr: g.vr,
        pad_hidden_by_options: g.pad_hidden_by_options,
        last_played: g.last_played,
        size_on_disk: g.size_on_disk,
        playtime_minutes: g.playtime_minutes,
        tracked_minutes: g.tracked_minutes,
        is_favorite: g.is_favorite,
        uevr: g.uevr,
        collections: g.collections.clone(),
        cover: art(&g.cover),
        hero: art(&g.hero),
        logo: art(&g.logo),
    }
}

/// A library of stand-in games: gradient covers and hero art (a few missing,
/// to show the placeholders), favourites, playtime, sizes.
fn sample_games(ctx: &egui::Context) -> Vec<crate::games::LibGame> {
    use crate::games::{ArtState, LibGame};
    let names = [
        "VRChat", "Half-Life: Alyx", "Beat Saber", "Hollow Knight", "Pistol Whip", "Bonelab", "The Midnight Walk",
        "Hades II", "Blade & Sorcery", "Stray", "Outer Wilds", "Ramage", "GOAT", "東方紅魔郷 〜 the Embodiment of Scarlet Devil",
    ];
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let tex = |name: &str, w: usize, h: usize, seed: usize| {
        let hue = (seed as f32 * 0.137) % 1.0;
        let px = (0..w * h)
            .map(|k| {
                let (x, y) = ((k % w) as f32 / w as f32, (k / w) as f32 / h as f32);
                let c = |o: f32| (((hue + o + x * 0.15) * std::f32::consts::TAU).sin() * 0.5 + 0.5) * (170.0 - 90.0 * y) + 30.0;
                egui::Color32::from_rgb(c(0.0) as u8, c(0.33) as u8, c(0.66) as u8)
            })
            .collect();
        ArtState::Ready(ctx.load_texture(format!("{name}-{w}x{h}"), egui::ColorImage { size: [w, h], pixels: px }, egui::TextureOptions::LINEAR))
    };
    names
        .iter()
        .enumerate()
        .map(|(i, name)| LibGame {
            name: name.to_string(),
            app_id: Some(format!("{}", 1000 + i)),
            shortcut_id: None,
            cover_id: Some(format!("{}", 1000 + i)),
            source: if i % 5 == 4 { "Non-Steam".into() } else { "Steam".into() },
            exe: None,
            start_dir: None,
            uevr_capable: i == 9,
            vr: i < 6,
            pad_hidden_by_options: false,
            last_played: Some(now - (i as u64 * 7 + 1) * 3600 * 5),
            size_on_disk: Some((i as u64 + 1) * 3_700_000_000),
            playtime_minutes: Some((14 - i as u32) * 97),
            tracked_minutes: None,
            is_favorite: i % 4 == 0,
            uevr: false,
            collections: Vec::new(),
            cover: if i % 6 == 5 { ArtState::Missing } else { tex(name, 60, 90, i) },
            hero: if i % 6 == 5 { ArtState::Missing } else { tex(name, 192, 62, i + 3) },
            logo: ArtState::Missing,
        })
        .collect()
}

/// Dashboard pages: each at the panel's real size (what you see first) and on
/// a tall canvas cropped to the content (the whole scrolling page).
fn pages(ctx: &egui::Context, textures: &mut HashMap<egui::TextureId, Tex>, dir: &Path) -> Result<()> {
    use crate::ui::{DesktopTab, Nav, PhotosTab, SettingsTab, SystemTab};
    type Setup = Box<dyn Fn(&mut crate::ui::LibState)>;
    let name = |page: &str, tab: &dyn std::fmt::Debug| format!("page-{page}-{}", format!("{tab:?}").to_lowercase());
    let mut shots: Vec<(String, Setup)> = Vec::new();
    for tab in SettingsTab::ALL {
        shots.push((name("settings", &tab), Box::new(move |st| {
            st.nav = Nav::Settings;
            st.settings_tab = tab;
        })));
    }
    for tab in [SystemTab::Timer, SystemTab::Playspace, SystemTab::Monado] {
        shots.push((name("system", &tab), Box::new(move |st| {
            st.nav = Nav::System;
            st.system_tab = tab;
        })));
    }
    for tab in DesktopTab::ALL {
        shots.push((name("desktop", &tab), Box::new(move |st| {
            st.nav = Nav::Desktop;
            st.desktop_tab = tab;
        })));
    }
    // The share dialog in flight: the first setup, then adding one more screen.
    shots.push(("page-desktop-screens-waiting".into(), Box::new(|st| {
        st.nav = Nav::Desktop;
        st.desktop_tab = DesktopTab::Screens;
        st.desktop_ready = false;
        st.desktop_pending = true;
        st.desktop_status = "Waiting for the screen-share dialog on your desktop…".into();
        for r in &mut st.desktop_rows {
            r.approved = false;
            r.shown = false;
            r.hint = Some("Set up screens to approve it".into());
        }
    })));
    shots.push(("page-desktop-screens-adding".into(), Box::new(|st| {
        st.nav = Nav::Desktop;
        st.desktop_tab = DesktopTab::Screens;
        st.desktop_pending = true;
    })));
    for tab in PhotosTab::ALL {
        shots.push((name("photos", &tab), Box::new(move |st| {
            st.nav = Nav::Photos;
            st.photos_tab = tab;
        })));
    }
    // Stand-in screenshots for the gallery: soft gradients, some landscape,
    // one portrait (the tiles centre-crop).
    let shots_tex: Vec<(egui::TextureHandle, String)> = (0..8)
        .map(|i| {
            let (w, h) = if i == 3 { (90, 160) } else { (160, 90) };
            let hue = i as f32 / 8.0;
            let px = (0..w * h)
                .map(|k| {
                    let (x, y) = ((k % w) as f32 / w as f32, (k / w) as f32 / h as f32);
                    let c = |o: f32| (((hue + o) * std::f32::consts::TAU).sin() * 0.5 + 0.5) * 180.0 + 40.0 * (1.0 - y);
                    egui::Color32::from_rgb(c(0.0) as u8, c(0.33 + x * 0.2) as u8, c(0.66) as u8)
                })
                .collect();
            let img = egui::ColorImage { size: [w, h], pixels: px };
            (ctx.load_texture(format!("shot-{i}"), img, egui::TextureOptions::LINEAR), format!("Sep {}, 10:{:02} PM", 20 + i / 3, 5 * i))
        })
        .collect();
    for (name, setup) in shots {
        for (suffix, px) in [("", MAIN_PX), ("-full", TALL_PX)] {
            let file = format!("{name}{suffix}");
            if !wanted(&file) {
                continue;
            }
            let mut st = sample_state();
            st.gallery_items = shots_tex.clone();
            (st.gallery_total, st.gallery_pages) = (8, 1);
            setup(&mut st);
            let mut img = shoot(ctx, textures, px, 12, |ctx| crate::ui::build_main(ctx, &mut st));
            if px == TALL_PX {
                img = crop_to_content(img);
            }
            let path = dir.join(format!("{file}.png"));
            img.save(&path)?;
            println!("{}", path.display());
        }
    }
    Ok(())
}

/// Run `frames` egui passes 50 ms apart (fades and eased widgets settle), then
/// rasterise the last one.
fn shoot(
    ctx: &egui::Context,
    textures: &mut HashMap<egui::TextureId, Tex>,
    px: (usize, usize),
    frames: usize,
    build: impl FnMut(&egui::Context),
) -> image::RgbaImage {
    shoot_over(ctx, textures, px, frames, false, build)
}

/// `shoot`, optionally without a backdrop (straight alpha, for compositing).
fn shoot_over(
    ctx: &egui::Context,
    textures: &mut HashMap<egui::TextureId, Tex>,
    px: (usize, usize),
    frames: usize,
    transparent: bool,
    mut build: impl FnMut(&egui::Context),
) -> image::RgbaImage {
    let mut last = None;
    for f in 0..frames {
        let out = ctx.run(screen_input(px, 10.0 + f as f64 * 0.05), |ctx| build(ctx));
        for (id, delta) in &out.textures_delta.set {
            apply_delta(textures, *id, delta);
        }
        for id in &out.textures_delta.free {
            textures.remove(id);
        }
        last = Some(out);
    }
    let out = last.expect("at least one frame");
    let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
    rasterise_over(&prims, textures, out.pixels_per_point, px, transparent)
}

/// Cut a tall page shot just below its last content (rows that are all panel
/// background are empty).
fn crop_to_content(img: image::RgbaImage) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    let bg = theme::SURFACE;
    let empty = |y: u32| {
        (w / 6..w * 5 / 6).step_by(3).all(|x| {
            let p = img.get_pixel(x, y);
            (p[0] as i32 - bg.r() as i32).abs() <= 2 && (p[1] as i32 - bg.g() as i32).abs() <= 2 && (p[2] as i32 - bg.b() as i32).abs() <= 2
        })
    };
    let last = (0..h).rev().find(|&y| !empty(y)).unwrap_or(h - 1);
    let keep = (last + 60).min(h);
    image::imageops::crop_imm(&img, 0, 0, w, keep).to_image()
}

/// A LibState dressed like a real session: three screens, two layouts, time
/// zones, a running game, the fork's hold switch.
fn sample_state() -> crate::ui::LibState {
    use crate::desktop::ScreenRow;
    let mut st = crate::ui::LibState::new();
    st.scanning = false;
    st.clock = "10:45 PM".into();
    st.hold_pose = Some(true);
    st.monado_freeze_supported = true;
    st.monado_clients = vec![
        crate::monado::ClientInfo { id: 3, name: "VRChat".into(), is_app: true, is_primary: true, frozen: false },
        crate::monado::ClientInfo { id: 5, name: "NemuriXR".into(), is_app: false, is_primary: false, frozen: false },
    ];
    st.watch_buttons = vec!["keyboard".into(), "recenter".into(), "letgo".into(), "freeze".into()];
    st.watch_zone_ids = vec!["America/New_York".into(), "Asia/Tokyo".into()];
    st.watch_times = vec![("New York".into(), "4:45 PM".into()), ("Tokyo".into(), "5:45 AM".into())];
    st.skybox_source = "built-in (Table Mountain 2, CC0)".into();
    st.skybox_custom_hint = "~/.config/monadeck/skybox.jpg".into();
    st.notif_dbus_ok = true;
    st.notif_udp_ok = true;
    st.game_profiles = vec!["Xbox".into(), "Keyboard + mouse".into(), "Hollow Knight".into()];
    st.game_profile = "Xbox".into();
    st.game_protonfixes_ok = true;
    st.desktop_ready = true;
    st.desktop_dmabuf = true;
    st.desktop_color_scale = true;
    st.desktop_status = "3 screens approved".into();
    st.desktop_shown = 2;
    st.keyboard_layout = "English (US)".into();
    let row = |name: &str, detail: &str, shown: bool| ScreenRow {
        name: name.into(),
        detail: detail.into(),
        hint: None,
        shown,
        approved: true,
        opacity: 1.0,
    };
    st.desktop_rows = vec![
        row("DP-3", "Samsung Odyssey · 2560×1440", true),
        row("DP-1", "Dell U2720Q · 1920×1080", true),
        row("HDMI-A-1", "LG TV · 1920×1080", false),
    ];
    st.desktop_bar = st.desktop_rows.iter().map(|r| (r.name.clone(), r.shown)).collect();
    st.layouts = vec![("Standing".into(), 2), ("Lying down".into(), 1)];
    st.layout_follow = vec![false, true];
    st.layout_active = Some("Standing".into());
    st
}

fn screen_input(px: (usize, usize), time: f64) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(px.0 as f32 / PPP, px.1 as f32 / PPP))),
        time: Some(time),
        ..Default::default()
    }
}

/// A blurple rounded square with a white dot: a stand-in app icon whose
/// dominant colour should win the accent.
fn sample_icon() -> egui::ColorImage {
    let n = 64usize;
    let mut px = vec![egui::Color32::TRANSPARENT; n * n];
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32 - 31.5, y as f32 - 31.5);
            let inside = fx.abs().max(fy.abs()) < 30.0 && (fx * fx + fy * fy).sqrt() < 40.0;
            let dot = (fx * fx + (fy + 2.0) * (fy + 2.0)).sqrt() < 12.0;
            px[y * n + x] = if dot { egui::Color32::WHITE } else if inside { egui::Color32::from_rgb(88, 101, 242) } else { egui::Color32::TRANSPARENT };
        }
    }
    egui::ColorImage { size: [n, n], pixels: px }
}

fn apply_delta(textures: &mut HashMap<egui::TextureId, Tex>, id: egui::TextureId, delta: &egui::epaint::ImageDelta) {
    let (size, px): ([usize; 2], Vec<egui::Color32>) = match &delta.image {
        egui::ImageData::Color(c) => (c.size, c.pixels.clone()),
        egui::ImageData::Font(f) => (f.size, f.srgba_pixels(None).collect()),
    };
    match delta.pos {
        None => {
            textures.insert(id, Tex { size, px });
        }
        Some([x0, y0]) => {
            if let Some(t) = textures.get_mut(&id) {
                for y in 0..size[1] {
                    for x in 0..size[0] {
                        if let Some(dst) = t.px.get_mut((y0 + y) * t.size[0] + x0 + x) {
                            *dst = px[y * size[0] + x];
                        }
                    }
                }
            }
        }
    }
}

/// Premultiplied-alpha triangle rasteriser over a dim scene-like backdrop.
fn rasterise(prims: &[egui::ClippedPrimitive], textures: &HashMap<egui::TextureId, Tex>, ppp: f32, px: (usize, usize)) -> image::RgbaImage {
    rasterise_over(prims, textures, ppp, px, false)
}

/// `transparent`: no backdrop, a straight-alpha image to composite elsewhere.
fn rasterise_over(prims: &[egui::ClippedPrimitive], textures: &HashMap<egui::TextureId, Tex>, ppp: f32, px: (usize, usize), transparent: bool) -> image::RgbaImage {
    let (w, h) = px;
    // Backdrop: a soft vertical gradient standing in for whatever's behind the card.
    let mut buf: Vec<[f32; 4]> = (0..w * h)
        .map(|i| {
            if transparent {
                return [0.0; 4];
            }
            let t = (i / w) as f32 / h as f32;
            [0.13 + 0.03 * t, 0.15 + 0.03 * t, 0.20 + 0.04 * t, 1.0]
        })
        .collect();
    let sample = |tex: &Tex, u: f32, v: f32| -> [f32; 4] {
        let fx = (u * tex.size[0] as f32 - 0.5).max(0.0);
        let fy = (v * tex.size[1] as f32 - 0.5).max(0.0);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let at = |x: usize, y: usize| {
            let c = tex.px[y.min(tex.size[1] - 1) * tex.size[0] + x.min(tex.size[0] - 1)];
            [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, c.a() as f32 / 255.0]
        };
        let (a, b, c, d) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
        let mut out = [0.0; 4];
        for i in 0..4 {
            out[i] = (a[i] * (1.0 - tx) + b[i] * tx) * (1.0 - ty) + (c[i] * (1.0 - tx) + d[i] * tx) * ty;
        }
        out
    };
    for cp in prims {
        let egui::epaint::Primitive::Mesh(mesh) = &cp.primitive else { continue };
        let Some(tex) = textures.get(&mesh.texture_id) else { continue };
        let clip = cp.clip_rect;
        let (cx0, cy0) = ((clip.min.x * ppp).floor().max(0.0) as usize, (clip.min.y * ppp).floor().max(0.0) as usize);
        let (cx1, cy1) = ((clip.max.x * ppp).ceil().min(w as f32) as usize, (clip.max.y * ppp).ceil().min(h as f32) as usize);
        for tri in mesh.indices.chunks_exact(3) {
            let mut v: Vec<&egui::epaint::Vertex> = tri.iter().map(|&i| &mesh.vertices[i as usize]).collect();
            // Like a GPU: snap vertices to a 1/256 px grid and evaluate the edge
            // functions in exact integer arithmetic. With floats, a pixel centre
            // lying right on a shared edge ties differently per pixel, which
            // shows up as dashes along every 1 px rim. (From NemuriXR's rig.)
            const SUB: i64 = 256;
            let snap = |f: f32| (f as f64 * ppp as f64 * SUB as f64).round() as i64;
            let mut p: Vec<(i64, i64)> = v.iter().map(|v| (snap(v.pos.x), snap(v.pos.y))).collect();
            let edge = |a: (i64, i64), b: (i64, i64), c: (i64, i64)| (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            let mut area = edge(p[0], p[1], p[2]);
            if area == 0 {
                continue;
            }
            if area < 0 {
                v.swap(1, 2);
                p.swap(1, 2);
                area = -area;
            }
            // Top-left fill rule, so shared edges are drawn exactly once.
            let top_left = |a: (i64, i64), b: (i64, i64)| (a.1 == b.1 && b.0 < a.0) || b.1 < a.1;
            let tl = [top_left(p[1], p[2]), top_left(p[2], p[0]), top_left(p[0], p[1])];
            let px_of = |sub: i64| sub.div_euclid(SUB);
            let xmin = px_of(p.iter().map(|q| q.0).min().unwrap()).max(cx0 as i64) as usize;
            let xmax = (px_of(p.iter().map(|q| q.0).max().unwrap()) + 1).min(cx1 as i64).max(0) as usize;
            let ymin = px_of(p.iter().map(|q| q.1).min().unwrap()).max(cy0 as i64) as usize;
            let ymax = (px_of(p.iter().map(|q| q.1).max().unwrap()) + 1).min(cy1 as i64).max(0) as usize;
            for y in ymin..ymax {
                for x in xmin..xmax {
                    let c = (x as i64 * SUB + SUB / 2, y as i64 * SUB + SUB / 2);
                    let (e0, e1, e2) = (edge(p[1], p[2], c), edge(p[2], p[0], c), edge(p[0], p[1], c));
                    let inside = |e: i64, tl: bool| e > 0 || (e == 0 && tl);
                    if !inside(e0, tl[0]) || !inside(e1, tl[1]) || !inside(e2, tl[2]) {
                        continue;
                    }
                    let (w0, w1) = (e0 as f32 / area as f32, e1 as f32 / area as f32);
                    let w2 = 1.0 - w0 - w1;
                    let lerp = |f: &dyn Fn(&egui::epaint::Vertex) -> f32| f(v[0]) * w0 + f(v[1]) * w1 + f(v[2]) * w2;
                    let (u, vv) = (lerp(&|v| v.uv.x), lerp(&|v| v.uv.y));
                    let t = sample(tex, u, vv);
                    let col = [
                        lerp(&|v| v.color.r() as f32 / 255.0) * t[0],
                        lerp(&|v| v.color.g() as f32 / 255.0) * t[1],
                        lerp(&|v| v.color.b() as f32 / 255.0) * t[2],
                        lerp(&|v| v.color.a() as f32 / 255.0) * t[3],
                    ];
                    let dst = &mut buf[y * w + x];
                    for i in 0..4 {
                        dst[i] = col[i] + dst[i] * (1.0 - col[3]);
                    }
                }
            }
        }
    }
    image::RgbaImage::from_fn(w as u32, h as u32, |x, y| {
        let c = buf[y as usize * w + x as usize];
        if transparent {
            // Premultiplied → straight alpha.
            let a = c[3].clamp(0.0, 1.0);
            let un = |v: f32| if a > 1e-4 { (v / a).clamp(0.0, 1.0) } else { 0.0 };
            image::Rgba([(un(c[0]) * 255.0).round() as u8, (un(c[1]) * 255.0).round() as u8, (un(c[2]) * 255.0).round() as u8, (a * 255.0).round() as u8])
        } else {
            image::Rgba([(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8, 255])
        }
    })
}
