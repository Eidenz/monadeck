//! `--toast-preview [dir]`: renders the toast cards to PNGs with a tiny software
//! rasteriser — real egui layout and tessellation, no GPU or headset — so the
//! design can be checked (and screenshotted) outside VR.
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::gfx::{apply_style, PPP};
use crate::toast::{Kind, Source, Toast, Toasts};

/// The toast panel's pixel size (see `make_panel` in main).
const PX: (usize, usize) = (960, 280);

/// (name, toast, extra queued, frames as (suffix, seconds since shown)).
type Case = (&'static str, Toast, usize, Vec<(&'static str, f32)>);

struct Tex {
    size: [usize; 2],
    px: Vec<egui::Color32>,
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
    let out = ctx.run(screen_input(0.0), |_| {});
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
            Toast::new(Kind::Welcome, "Monadeck is ready", "Left system button opens the dashboard · Settings › Controllers › Help lists every gesture"),
            0,
            vec![("open", 1.6)],
        ),
    ];

    let pose = openxr::Posef::IDENTITY;
    for (name, toast, extra, frames) in cases {
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
            let queued = toasts.update(&ctx, None).map_or(0, |(_, q)| q);
            let out = ctx.run(screen_input(secs as f64), |ctx| toasts.draw(ctx, now, queued));
            for (id, delta) in &out.textures_delta.set {
                apply_delta(&mut textures, *id, delta);
            }
            let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
            let img = rasterise(&prims, &textures, out.pixels_per_point);
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
    let out = ctx.run(screen_input(0.5), |ctx| toasts.draw(ctx, start + Duration::from_millis(500), 0));
    for (id, delta) in &out.textures_delta.set {
        apply_delta(&mut textures, *id, delta);
    }
    let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
    let path = dir.join("readout-open.png");
    rasterise(&prims, &textures, out.pixels_per_point).save(&path)?;
    println!("{}", path.display());
    Ok(())
}

fn screen_input(time: f64) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(PX.0 as f32 / PPP, PX.1 as f32 / PPP))),
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
fn rasterise(prims: &[egui::ClippedPrimitive], textures: &HashMap<egui::TextureId, Tex>, ppp: f32) -> image::RgbaImage {
    let (w, h) = PX;
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
            // f64 edge functions: f32 error at panel-sized coordinates leaves
            // gaps along shared edges.
            let mut p: Vec<(f64, f64)> = v.iter().map(|v| ((v.pos.x * ppp) as f64, (v.pos.y * ppp) as f64)).collect();
            let mut area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
            if area.abs() < 1e-9 {
                continue;
            }
            if area < 0.0 {
                v.swap(1, 2);
                p.swap(1, 2);
                area = -area;
            }
            // Top-left fill rule, so shared edges are drawn exactly once.
            let top_left = |a: (f64, f64), b: (f64, f64)| (a.1 == b.1 && b.0 < a.0) || b.1 < a.1;
            let tl = [top_left(p[1], p[2]), top_left(p[2], p[0]), top_left(p[0], p[1])];
            let xmin = p.iter().map(|q| q.0).fold(f64::MAX, f64::min).floor().max(cx0 as f64) as usize;
            let xmax = p.iter().map(|q| q.0).fold(f64::MIN, f64::max).ceil().min(cx1 as f64) as usize;
            let ymin = p.iter().map(|q| q.1).fold(f64::MAX, f64::min).floor().max(cy0 as f64) as usize;
            let ymax = p.iter().map(|q| q.1).fold(f64::MIN, f64::max).ceil().min(cy1 as f64) as usize;
            for y in ymin..ymax {
                for x in xmin..xmax {
                    let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                    let w0 = ((p[1].0 - px) * (p[2].1 - py) - (p[2].0 - px) * (p[1].1 - py)) / area;
                    let w1 = ((p[2].0 - px) * (p[0].1 - py) - (p[0].0 - px) * (p[2].1 - py)) / area;
                    let w2 = 1.0 - w0 - w1;
                    let inside = |w: f64, tl: bool| w > 0.0 || (w == 0.0 && tl);
                    if !inside(w0, tl[0]) || !inside(w1, tl[1]) || !inside(w2, tl[2]) {
                        continue;
                    }
                    let (w0, w1, w2) = (w0 as f32, w1 as f32, w2 as f32);
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
