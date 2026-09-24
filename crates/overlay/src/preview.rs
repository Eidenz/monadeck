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
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
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
    use crate::monado::{BatteryInfo, BatteryKind, DevState};
    let bat = |kind, charge| BatteryInfo { kind, charge, charging: false, state: DevState::Live };
    let with = |b: BatteryInfo, state| BatteryInfo { state, ..b };
    // A controller switched off (grey "off"), a tracker out of sight (faded).
    let states = vec![
        bat(BatteryKind::Controller, 0.86),
        with(bat(BatteryKind::Controller, 0.40), DevState::Off),
        with(bat(BatteryKind::Tracker, 0.67), DevState::Lost),
        bat(BatteryKind::Glove, 0.80),
        bat(BatteryKind::Glove, 0.60),
    ];
    let rigs: [(&str, Vec<BatteryInfo>); 4] = [
        ("bottom-bar-states", states.clone()),
        ("bottom-bar-controllers", vec![bat(BatteryKind::Controller, 0.86), bat(BatteryKind::Controller, 0.83)]),
        (
            "bottom-bar-five-devices",
            vec![
                bat(BatteryKind::Controller, 0.86),
                bat(BatteryKind::Controller, 0.83),
                bat(BatteryKind::Tracker, 0.67),
                bat(BatteryKind::Glove, 0.72),
                bat(BatteryKind::Other, 0.65),
            ],
        ),
        (
            "bottom-bar-full-body",
            vec![
                bat(BatteryKind::Controller, 0.86),
                bat(BatteryKind::Controller, 0.83),
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
    Ok(())
}

/// Dashboard pages: each at the panel's real size (what you see first) and on
/// a tall canvas cropped to the content (the whole scrolling page).
fn pages(ctx: &egui::Context, textures: &mut HashMap<egui::TextureId, Tex>, dir: &Path) -> Result<()> {
    use crate::ui::settings_page_tabs as tabs;
    use crate::ui::{Nav, SystemTab};
    type Setup = Box<dyn Fn(&mut crate::ui::LibState)>;
    let mut shots: Vec<(String, Setup)> = Vec::new();
    for tab in tabs() {
        let name = format!("page-settings-{}", format!("{tab:?}").to_lowercase());
        shots.push((name, Box::new(move |st| {
            st.nav = Nav::Settings;
            st.settings_tab = tab;
        })));
    }
    shots.push(("page-settings-classic".into(), Box::new(|st| {
        st.nav = Nav::Settings;
        st.settings_classic = true;
    })));
    shots.push(("page-desktop".into(), Box::new(|st| st.nav = Nav::Desktop)));
    shots.push(("page-system-monado".into(), Box::new(|st| {
        st.nav = Nav::System;
        st.system_tab = SystemTab::Monado;
    })));
    for (name, setup) in shots {
        for (suffix, px) in [("", MAIN_PX), ("-full", TALL_PX)] {
            let file = format!("{name}{suffix}");
            if !wanted(&file) {
                continue;
            }
            let mut st = sample_state();
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
    rasterise(&prims, textures, out.pixels_per_point, px)
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
    let (w, h) = px;
    // Backdrop: a soft vertical gradient standing in for whatever's behind the card.
    let mut buf: Vec<[f32; 4]> = (0..w * h)
        .map(|i| {
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
        image::Rgba([(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8, 255])
    })
}
