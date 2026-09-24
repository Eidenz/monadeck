// The widget kit every tool page is built from (Settings, System, Desktop,
// Photos): a page shell with a category list on the left, and custom-painted
// controls sized for a laser (NemuriXR's vocabulary: switch rows, toggle tiles,
// segmented choices, steppers, cards). Multi-part controls paint into one
// allocated rect, so they read left to right even inside right-to-left rows.
use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Response, Sense, Stroke, StrokeKind};
use egui_phosphor::regular as icon;

use crate::gfx::theme;

const NAV_W: f32 = 236.0;
/// Right-hand slider width.
pub(super) const SLIDER_W: f32 = 340.0;
/// Toggle tiles: icon chip on top, title and one line at the bottom.
pub(super) const TILE_H: f32 = 132.0;
/// Seconds a category takes to fade in.
const TAB_FADE: f64 = 0.18;

const STOP_RED: Color32 = Color32::from_rgb(224, 78, 78);

// --- page shell ------------------------------------------------------------------------

/// One entry of a page's category list.
pub(super) struct Tab {
    pub glyph: &'static str,
    pub label: &'static str,
    /// Shown beside the page title while this category is open.
    pub blurb: &'static str,
}

/// A tool page: title + the open category's blurb on top, the category list
/// on the left, the category's content (scrolling) on the right. `salt`
/// keeps each page's scroll and fade state apart. Returns a clicked category.
pub(super) fn shell(ui: &mut egui::Ui, glyph: &str, title: &str, tabs: &[Tab], current: usize, salt: &str, body: impl FnOnce(&mut egui::Ui)) -> Option<usize> {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(glyph).size(28.0).color(theme::PRIMARY));
        ui.add_space(8.0);
        ui.label(egui::RichText::new(title).size(28.0).strong().color(Color32::WHITE));
        ui.add_space(14.0);
        if let Some(t) = tabs.get(current) {
            ui.label(egui::RichText::new(t.blurb).size(15.0).color(theme::ON_SURFACE_VAR));
        }
    });
    ui.add_space(14.0);
    let body_h = ui.available_height();
    let mut picked = None;
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(NAV_W, body_h), Layout::top_down(Align::Min), |ui| {
            ui.set_min_size(egui::vec2(NAV_W, body_h));
            ui.spacing_mut().item_spacing.y = 6.0;
            for (i, t) in tabs.iter().enumerate() {
                if nav_item(ui, t.glyph, t.label, i == current).clicked() && i != current {
                    picked = Some(i);
                }
            }
        });
        ui.add_space(26.0);
        let w = ui.available_width();
        ui.allocate_ui_with_layout(egui::vec2(w, body_h), Layout::top_down(Align::Min), |ui| {
            ui.set_min_size(egui::vec2(w, body_h));
            ui.set_opacity(tab_fade(ui, salt, current));
            egui::ScrollArea::vertical().id_salt((salt, current)).auto_shrink([false, false]).show(ui, |ui| {
                // Leave the scrollbar its own gutter.
                ui.set_width(w - 18.0);
                body(ui);
                ui.add_space(24.0);
            });
        });
    });
    picked
}

/// Opacity for the open category: fades in over `TAB_FADE` after a switch.
fn tab_fade(ui: &egui::Ui, salt: &str, current: usize) -> f32 {
    let now = ui.input(|i| i.time);
    let id = egui::Id::new(("page-fade", salt));
    let (shown, since) = ui.ctx().data_mut(|d| *d.get_temp_mut_or(id, (current, now)));
    if shown != current {
        ui.ctx().data_mut(|d| d.insert_temp(id, (current, now)));
        return 0.0;
    }
    (((now - since) / TAB_FADE) as f32).clamp(0.0, 1.0)
}

fn nav_item(ui: &mut egui::Ui, glyph: &str, label: &str, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(NAV_W, 54.0), Sense::click());
    let h = hover_t(ui, &resp);
    let on = ui.ctx().animate_bool_with_time(resp.id.with("on"), active, 0.16);
    let p = ui.painter();
    let fill = mix(alpha(Color32::WHITE, 0.05 * h), alpha(theme::PRIMARY, 0.16), on);
    p.rect_filled(rect, CornerRadius::same(14), fill);
    if on > 0.01 {
        // An accent tick on the left edge of the open category.
        let bar = Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - 13.0), egui::vec2(4.0, 26.0));
        p.rect_filled(bar, CornerRadius::same(2), alpha(theme::PRIMARY, on));
    }
    let fg = mix(mix(theme::ON_SURFACE_VAR, Color32::WHITE, h), Color32::WHITE, on);
    let glyph_fg = mix(mix(theme::ON_SURFACE_VAR, Color32::WHITE, h), theme::PRIMARY, on);
    p.text(Pos2::new(rect.left() + 28.0, rect.center().y), Align2::CENTER_CENTER, glyph, FontId::proportional(22.0), glyph_fg);
    p.text(Pos2::new(rect.left() + 52.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(17.0), fg);
    resp
}

// --- widgets ------------------------------------------------------------------------------

pub(super) fn alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}

/// Blend `a` toward `b` by `t` (0 keeps `a`).
pub(super) fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Eased 0..1 hover amount for a response. Kit widgets paint their own
/// hover, so a hovered one is also noted for `take_kit_hovered` (the panel's
/// generic hover glow leaves those alone).
pub(super) fn hover_t(ui: &egui::Ui, resp: &Response) -> f32 {
    if resp.hovered() {
        ui.ctx().data_mut(|d| {
            let v = d.get_temp_mut_or_default::<Vec<Rect>>(kit_hovered_id());
            // Panels that never take the list mustn't grow it forever.
            if v.len() > 64 {
                v.clear();
            }
            v.push(resp.rect);
        });
    }
    ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.12)
}

fn kit_hovered_id() -> egui::Id {
    egui::Id::new("kit-hovered")
}

/// Rects of kit widgets hovered this frame (cleared by the call).
pub(super) fn take_kit_hovered(ctx: &egui::Context) -> Vec<Rect> {
    ctx.data_mut(|d| d.remove_temp::<Vec<Rect>>(kit_hovered_id()).unwrap_or_default())
}

/// A rounded rect filled with an exact top→bottom gradient (a triangle fan
/// with per-vertex colours; a linear gradient survives any triangulation).
pub(super) fn gradient_rect(painter: &egui::Painter, rect: Rect, radius: f32, top: Color32, bottom: Color32) {
    let r = radius.min(rect.width() * 0.5).min(rect.height() * 0.5);
    let colour_at = |p: Pos2| mix(top, bottom, (p.y - rect.top()) / rect.height().max(1.0));
    let mut mesh = egui::Mesh::default();
    let centre = rect.center();
    mesh.colored_vertex(centre, colour_at(centre));
    let corners = [
        (Pos2::new(rect.right() - r, rect.top() + r), -90.0f32),
        (Pos2::new(rect.right() - r, rect.bottom() - r), 0.0),
        (Pos2::new(rect.left() + r, rect.bottom() - r), 90.0),
        (Pos2::new(rect.left() + r, rect.top() + r), 180.0),
    ];
    const STEPS: usize = 10;
    for (c, start) in corners {
        for i in 0..=STEPS {
            let a = (start + 90.0 * i as f32 / STEPS as f32).to_radians();
            let p = Pos2::new(c.x + r * a.cos(), c.y + r * a.sin());
            mesh.colored_vertex(p, colour_at(p));
        }
    }
    let n = (corners.len() * (STEPS + 1)) as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// A texture centre-cropped into `rect` (object-fit: cover), rounded.
pub(super) fn cover_image(ui: &egui::Ui, rect: Rect, tex: &egui::TextureHandle, radius: f32) {
    let [tw, th] = tex.size();
    let (ta, da) = (tw as f32 / th.max(1) as f32, rect.width() / rect.height().max(1.0));
    let uv = if ta > da {
        let k = da / ta;
        Rect::from_min_max(Pos2::new((1.0 - k) / 2.0, 0.0), Pos2::new((1.0 + k) / 2.0, 1.0))
    } else {
        let k = ta / da;
        Rect::from_min_max(Pos2::new(0.0, (1.0 - k) / 2.0), Pos2::new(1.0, (1.0 + k) / 2.0))
    };
    egui::Image::new(egui::load::SizedTexture::new(tex.id(), tex.size_vec2()))
        .uv(uv)
        .corner_radius(CornerRadius::same(radius as u8))
        .paint_at(ui, rect);
}

/// A dark translucent button for use over artwork: glyph (+ label), `lit`
/// tints it with `accent`. Returns the response.
pub(super) fn glass_button(ui: &mut egui::Ui, rect: Rect, id: egui::Id, glyph: &str, label: &str, lit: bool, accent: Color32) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let h = hover_t(ui, &resp);
    let on = ui.ctx().animate_bool_with_time(id.with("on"), lit, 0.16);
    let p = ui.painter();
    let radius = CornerRadius::same((rect.height() / 2.0) as u8);
    p.rect_filled(rect, radius, mix(Color32::from_black_alpha(130), Color32::from_black_alpha(170), h));
    p.rect_stroke(rect, radius, Stroke::new(1.0, mix(alpha(Color32::WHITE, 0.14 + 0.2 * h), alpha(accent, 0.8), on)), StrokeKind::Inside);
    let fg = mix(mix(theme::ON_SURFACE, Color32::WHITE, h), accent, on);
    let text = if label.is_empty() { glyph.to_string() } else { format!("{glyph}  {label}") };
    p.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(if label.is_empty() { 21.0 } else { 16.0 }), fg);
    resp
}

/// One line of text, cut with an ellipsis past `max_w`.
pub(super) fn fit_text(ui: &egui::Ui, text: &str, size: f32, color: Color32, max_w: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), FontId::proportional(size), color, max_w);
    job.wrap = egui::text::TextWrapping { max_width: max_w, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    ui.fonts(|f| f.layout_job(job))
}

/// A small uppercase label above a card.
pub(super) fn group(ui: &mut egui::Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text.to_uppercase()).size(12.5).strong().color(theme::ON_SURFACE_VAR));
    ui.add_space(-4.0);
}

/// A rounded card holding rows.
pub(super) fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::default()
        .fill(theme::SURFACE_CONTAINER)
        .stroke(Stroke::new(1.0, alpha(Color32::WHITE, 0.05)))
        .corner_radius(18)
        .inner_margin(egui::Margin::symmetric(20, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.vertical(contents);
        });
    ui.add_space(6.0);
}

pub(super) fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, alpha(Color32::WHITE, 0.07)));
}

/// Title (+ a quieter line, wrapped) on the left, a control of `ctrl_w` on the
/// right, vertically centred. The text never runs under the control.
pub(super) fn row(ui: &mut egui::Ui, title: &str, sub: &str, ctrl_w: f32, control: impl FnOnce(&mut egui::Ui)) {
    let w = ui.available_width();
    let text_w = (w - ctrl_w - 28.0).max(180.0);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(text_w, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(text_w);
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.add_space(13.0);
            ui.label(egui::RichText::new(title).size(16.5).color(theme::ON_SURFACE));
            if !sub.is_empty() {
                ui.add(egui::Label::new(egui::RichText::new(sub).size(13.0).color(theme::ON_SURFACE_VAR)).wrap());
            }
            ui.add_space(13.0);
        });
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
}

/// A quiet one-liner inside a card (status, caveats).
pub(super) fn note(ui: &mut egui::Ui, glyph: &str, text: &str) {
    ui.add_space(2.0);
    ui.horizontal_top(|ui| {
        ui.label(egui::RichText::new(glyph).size(14.0).color(theme::ON_SURFACE_VAR));
        ui.add(egui::Label::new(egui::RichText::new(text).size(13.0).color(theme::ON_SURFACE_VAR)).wrap());
    });
    ui.add_space(12.0);
}

/// An animated on/off switch centred at `c`. `t` is the eased 0..1 on-amount.
pub(super) fn switch(painter: &egui::Painter, c: Pos2, t: f32, hover: f32) {
    let (w, h) = (52.0, 30.0);
    let track = Rect::from_center_size(c, egui::vec2(w, h));
    let off = mix(theme::SURFACE_CONTAINER_HIGH, Color32::from_rgb(56, 66, 78), hover);
    painter.rect_filled(track, CornerRadius::same((h / 2.0) as u8), mix(off, theme::PRIMARY, t));
    let x = track.left() + h / 2.0 + (w - h) * t;
    painter.circle_filled(Pos2::new(x, c.y), h / 2.0 - 4.0, mix(theme::ON_SURFACE_VAR, Color32::WHITE, t));
}

/// Title + wrapped sub on the left, a switch on the right; the whole row is
/// the target. Returns true if the value changed.
pub(super) fn switch_row(ui: &mut egui::Ui, title: &str, sub: &str, value: &mut bool) -> bool {
    let w = ui.available_width();
    let text_w = w - 84.0;
    let title_g = fit_text(ui, title, 16.5, theme::ON_SURFACE, text_w);
    let sub_g = (!sub.is_empty()).then(|| {
        ui.fonts(|f| f.layout(sub.to_owned(), FontId::proportional(13.0), theme::ON_SURFACE_VAR, text_w))
    });
    let text_h = title_g.size().y + sub_g.as_ref().map_or(0.0, |g| g.size().y + 3.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, text_h + 26.0), Sense::click());
    let h = hover_t(ui, &resp);
    let on = ui.ctx().animate_bool_with_time(resp.id.with("on"), *value, 0.16);
    let p = ui.painter();
    if h > 0.001 {
        p.rect_filled(rect.expand2(egui::vec2(10.0, 0.0)), CornerRadius::same(12), alpha(Color32::WHITE, 0.035 * h));
    }
    let top = rect.center().y - text_h / 2.0;
    let title_h = title_g.size().y;
    p.galley(Pos2::new(rect.left(), top), title_g, Color32::WHITE);
    if let Some(g) = sub_g {
        p.galley(Pos2::new(rect.left(), top + title_h + 3.0), g, Color32::WHITE);
    }
    switch(p, Pos2::new(rect.right() - 28.0, rect.center().y), on, h);
    if resp.clicked() {
        *value = !*value;
        true
    } else {
        false
    }
}

/// A big quick-settings tile: icon chip, switch, title, one line of what it
/// does; lit with the accent while on. Returns true if the value changed.
pub(super) fn toggle_tile(ui: &mut egui::Ui, size: egui::Vec2, glyph: &str, title: &str, sub: &str, value: &mut bool, enabled: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let h = if enabled { hover_t(ui, &resp) } else { 0.0 };
    let on = ui.ctx().animate_bool_with_time(resp.id.with("on"), *value, 0.18);
    let dim = if enabled { 1.0 } else { 0.45 };
    let lit = on * dim;
    let rect = rect.expand(h * 2.0);
    let p = ui.painter();
    let radius = CornerRadius::same(18);
    let base = mix(theme::SURFACE_CONTAINER, Color32::from_rgb(40, 52, 60), h * 0.6);
    p.rect_filled(rect, radius, mix(base, Color32::from_rgb(20, 72, 70), 0.55 * lit));
    let rim = mix(alpha(Color32::WHITE, 0.05), alpha(theme::PRIMARY, 0.55), lit);
    p.rect_stroke(rect, radius, Stroke::new(1.0, rim), StrokeKind::Inside);
    let pad = 16.0;
    let chip = Rect::from_min_size(rect.min + egui::vec2(pad, pad), egui::vec2(40.0, 40.0));
    icon_chip(p, chip, glyph, lit);
    switch(p, Pos2::new(rect.right() - pad - 26.0, chip.center().y), lit, h);
    let text_w = rect.width() - pad * 2.0;
    p.galley(Pos2::new(rect.left() + pad, rect.bottom() - pad - 40.0), fit_text(ui, title, 17.0, alpha(Color32::WHITE, dim), text_w), Color32::WHITE);
    p.galley(Pos2::new(rect.left() + pad, rect.bottom() - pad - 16.0), fit_text(ui, sub, 13.0, alpha(theme::ON_SURFACE_VAR, dim), text_w), Color32::WHITE);
    if resp.clicked() {
        *value = !*value;
        true
    } else {
        false
    }
}

/// A tinted rounded square carrying a glyph.
pub(super) fn icon_chip(painter: &egui::Painter, rect: Rect, glyph: &str, lit: f32) {
    let fill = mix(alpha(Color32::WHITE, 0.06), alpha(theme::PRIMARY, 0.22), lit);
    painter.rect_filled(rect, CornerRadius::same((rect.height() * 0.3) as u8), fill);
    let fg = mix(theme::ON_SURFACE_VAR, theme::PRIMARY, lit);
    painter.text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(rect.height() * 0.52), fg);
}

/// A watch quick-button slot: its number, the action's glyph and name.
pub(super) fn quick_tile(ui: &mut egui::Ui, size: egui::Vec2, n: usize, glyph: &str, label: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let h = hover_t(ui, &resp);
    let rect = rect.expand(h * 2.0);
    let p = ui.painter();
    let radius = CornerRadius::same(18);
    p.rect_filled(rect, radius, mix(theme::SURFACE_CONTAINER, Color32::from_rgb(40, 52, 60), h * 0.6));
    p.rect_stroke(rect, radius, Stroke::new(1.0, mix(alpha(Color32::WHITE, 0.05), alpha(theme::PRIMARY, 0.5), h)), StrokeKind::Inside);
    p.text(rect.left_top() + egui::vec2(14.0, 12.0), Align2::LEFT_TOP, format!("{n}"), FontId::proportional(13.0), theme::ON_SURFACE_VAR);
    p.text(rect.right_top() + egui::vec2(-14.0, 12.0), Align2::RIGHT_TOP, icon::ARROWS_CLOCKWISE, FontId::proportional(14.0), alpha(theme::ON_SURFACE_VAR, 0.4 + 0.6 * h));
    p.text(Pos2::new(rect.center().x, rect.top() + 48.0), Align2::CENTER_CENTER, glyph, FontId::proportional(30.0), theme::PRIMARY);
    let label_g = fit_text(ui, label, 13.5, theme::ON_SURFACE, rect.width() - 20.0);
    p.galley(Pos2::new(rect.center().x - label_g.size().x / 2.0, rect.bottom() - 32.0), label_g, Color32::WHITE);
    resp.on_hover_text(label)
}

/// A pill of options, one selected. Returns the tapped option, if any.
/// Painted in one allocated rect, so it reads left to right in any layout
/// (a right-to-left row would otherwise reverse or scatter it).
pub(super) fn segmented(ui: &mut egui::Ui, options: &[&str], selected: usize) -> Option<usize> {
    let (pad, gap, seg_h) = (4.0, 3.0, 40.0);
    let widths: Vec<f32> = options
        .iter()
        .map(|l| (ui.fonts(|f| f.layout_no_wrap(l.to_string(), FontId::proportional(15.0), Color32::WHITE)).size().x + 32.0).max(84.0))
        .collect();
    let total = widths.iter().sum::<f32>() + gap * (options.len().saturating_sub(1)) as f32 + pad * 2.0;
    let (rect, base) = ui.allocate_exact_size(egui::vec2(total, seg_h + pad * 2.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(13), Color32::from_rgb(22, 26, 32));
    let mut picked = None;
    let mut x = rect.left() + pad;
    for (i, label) in options.iter().enumerate() {
        let seg = Rect::from_min_size(Pos2::new(x, rect.top() + pad), egui::vec2(widths[i], seg_h));
        x += widths[i] + gap;
        let resp = ui.interact(seg, base.id.with(i), Sense::click());
        let h = hover_t(ui, &resp);
        let on = ui.ctx().animate_bool_with_time(resp.id.with("on"), i == selected, 0.14);
        let p = ui.painter();
        p.rect_filled(seg, CornerRadius::same(10), mix(alpha(Color32::WHITE, 0.06 * h), theme::PRIMARY, on));
        let fg = mix(mix(theme::ON_SURFACE_VAR, Color32::WHITE, h), Color32::BLACK, on);
        p.text(seg.center(), Align2::CENTER_CENTER, *label, FontId::proportional(15.0), fg);
        if resp.clicked() && i != selected {
            picked = Some(i);
        }
    }
    picked
}

/// − value + in one allocated rect (reads the right way round in a
/// right-to-left row). Returns true if the value changed.
pub(super) fn stepper(ui: &mut egui::Ui, value: &mut f32, min: f32, max: f32, step: f32, fmt: impl Fn(f32) -> String) -> bool {
    stepper_with(ui, value, min, max, step, (icon::MINUS, icon::PLUS), 96.0, fmt)
}

/// `stepper` with its own glyphs (e.g. rotate arrows) and value width.
#[allow(clippy::too_many_arguments)]
pub(super) fn stepper_with(
    ui: &mut egui::Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    step: f32,
    glyphs: (&str, &str),
    mid: f32,
    fmt: impl Fn(f32) -> String,
) -> bool {
    let btn = 44.0;
    let (rect, base) = ui.allocate_exact_size(egui::vec2(btn * 2.0 + mid, btn), Sense::hover());
    let before = *value;
    for (i, glyph, x) in [(0, glyphs.0, rect.left()), (1, glyphs.1, rect.right() - btn)] {
        let r = Rect::from_min_size(Pos2::new(x, rect.top()), egui::vec2(btn, btn));
        let resp = ui.interact(r, base.id.with(i), Sense::click());
        let h = hover_t(ui, &resp);
        let can = if i == 0 { *value > min + 1e-4 } else { *value < max - 1e-4 };
        let p = ui.painter();
        p.rect_filled(r, CornerRadius::same(12), mix(theme::SURFACE_CONTAINER_HIGH, Color32::from_rgb(56, 66, 78), h));
        let fg = if can { mix(theme::ON_SURFACE, Color32::WHITE, h) } else { alpha(theme::ON_SURFACE_VAR, 0.4) };
        p.text(r.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(18.0), fg);
        if resp.clicked() && can {
            *value = if i == 0 { (*value - step).max(min) } else { (*value + step).min(max) };
        }
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, fmt(*value), FontId::proportional(16.0), Color32::WHITE);
    (*value - before).abs() > f32::EPSILON
}

/// A selectable pill (remap profiles).
pub(super) fn choice_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> Response {
    let g = fit_text(ui, label, 15.0, Color32::WHITE, 220.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(g.size().x + 36.0, 42.0), Sense::click());
    let h = hover_t(ui, &resp);
    let on = ui.ctx().animate_bool_with_time(resp.id.with("on"), selected, 0.16);
    let p = ui.painter();
    let radius = CornerRadius::same(21);
    p.rect_filled(rect, radius, mix(mix(theme::SURFACE_CONTAINER_HIGH, Color32::from_rgb(56, 66, 78), h), theme::PRIMARY, on));
    let fg = mix(mix(theme::ON_SURFACE, Color32::WHITE, h), Color32::BLACK, on);
    let g = fit_text(ui, label, 15.0, fg, 220.0);
    p.galley(Pos2::new(rect.center().x - g.size().x / 2.0, rect.center().y - g.size().y / 2.0), g, fg);
    resp
}

/// A compact square icon button for row actions.
pub(super) fn small_icon(ui: &mut egui::Ui, glyph: &str, tip: &str) -> Response {
    icon_btn(ui, glyph, tip, false, true)
}

/// A compact square icon button: `armed` = a destructive action waiting for
/// its second tap (red); disabled ones fade and don't take clicks.
pub(super) fn icon_btn(ui: &mut egui::Ui, glyph: &str, tip: &str, armed: bool, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), if enabled { Sense::click() } else { Sense::hover() });
    let h = if enabled { hover_t(ui, &resp) } else { 0.0 };
    let p = ui.painter();
    let fill = if armed { mix(STOP_RED, Color32::WHITE, h * 0.12) } else { mix(theme::SURFACE_CONTAINER_HIGH, Color32::from_rgb(56, 66, 78), h) };
    p.rect_filled(rect, CornerRadius::same(12), if enabled { fill } else { alpha(fill, 0.5) });
    let fg = if armed {
        Color32::WHITE
    } else if enabled {
        mix(theme::ON_SURFACE, Color32::WHITE, h)
    } else {
        alpha(theme::ON_SURFACE_VAR, 0.35)
    };
    p.text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(19.0), fg);
    resp.on_hover_text(tip)
}

/// A gesture: the keys on the left, what they do wrapped beside them.
pub(super) fn gesture_row(ui: &mut egui::Ui, keys: &str, what: &str) {
    let key_w = 270.0;
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(key_w, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(key_w);
            ui.add(egui::Label::new(egui::RichText::new(keys).size(14.5).color(theme::ON_SURFACE)).wrap());
        });
        ui.add(egui::Label::new(egui::RichText::new(what).size(14.0).color(theme::ON_SURFACE_VAR)).wrap());
    });
}

/// "Keys — what they do" line for a reference list.
pub(super) fn key_row(ui: &mut egui::Ui, keys: &str, what: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), Sense::hover());
    let p = ui.painter();
    let key_w = 300.0;
    p.galley(Pos2::new(rect.left(), rect.center().y - 10.0), fit_text(ui, keys, 14.5, theme::PRIMARY, key_w), Color32::WHITE);
    p.galley(
        Pos2::new(rect.left() + key_w + 12.0, rect.center().y - 9.0),
        fit_text(ui, what, 14.0, theme::ON_SURFACE_VAR, rect.width() - key_w - 12.0),
        Color32::WHITE,
    );
}

/// How a [`button`] is dressed.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Tone {
    /// A neutral surface button.
    Neutral,
    /// The page's main action, filled with the accent.
    Primary,
    /// Currently in effect: tinted with the accent.
    Active,
    /// Destructive, first tap: outlined red.
    Danger,
    /// Destructive, armed (the next tap confirms): filled red.
    DangerArmed,
}

/// A button with a glyph and a label, at least `min_w` wide.
pub(super) fn button(ui: &mut egui::Ui, glyph: &str, label: &str, tone: Tone, min_w: f32) -> Response {
    button_enabled(ui, glyph, label, tone, min_w, true)
}

pub(super) fn button_enabled(ui: &mut egui::Ui, glyph: &str, label: &str, tone: Tone, min_w: f32, enabled: bool) -> Response {
    let text = if glyph.is_empty() { label.to_string() } else if label.is_empty() { glyph.to_string() } else { format!("{glyph}  {label}") };
    let g = ui.fonts(|f| f.layout_no_wrap(text.clone(), FontId::proportional(16.0), Color32::WHITE));
    let size = egui::vec2((g.size().x + 40.0).max(min_w), 46.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let h = if enabled { hover_t(ui, &resp) } else { 0.0 };
    let p = ui.painter();
    let radius = CornerRadius::same(13);
    let hover_fill = Color32::from_rgb(56, 66, 78);
    let (fill, rim, fg) = match tone {
        Tone::Neutral => (mix(theme::SURFACE_CONTAINER_HIGH, hover_fill, h), Color32::TRANSPARENT, mix(theme::ON_SURFACE, Color32::WHITE, h)),
        Tone::Primary => (mix(theme::PRIMARY, Color32::WHITE, h * 0.14), Color32::TRANSPARENT, Color32::BLACK),
        Tone::Active => (mix(Color32::from_rgb(20, 72, 70), Color32::from_rgb(28, 92, 88), h), alpha(theme::PRIMARY, 0.6), Color32::WHITE),
        Tone::Danger => (mix(Color32::TRANSPARENT, alpha(STOP_RED, 0.16), h), mix(alpha(Color32::WHITE, 0.12), STOP_RED, 0.5 + 0.5 * h), mix(theme::ON_SURFACE, STOP_RED, 0.4 + 0.6 * h)),
        Tone::DangerArmed => (mix(STOP_RED, Color32::WHITE, h * 0.12), Color32::TRANSPARENT, Color32::WHITE),
    };
    let dim = if enabled { 1.0 } else { 0.45 };
    p.rect_filled(rect, radius, alpha(fill, dim));
    if rim != Color32::TRANSPARENT {
        p.rect_stroke(rect, radius, Stroke::new(1.0, alpha(rim, dim)), StrokeKind::Inside);
    }
    p.text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(16.0), alpha(fg, dim));
    resp
}

/// The laser-friendly slider: rounded track, accent fill, round thumb, and the
/// value in a chip on the right. A click on the track jumps there; dragging
/// fine-tunes. Returns true if the value changed.
pub(super) fn slider(ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, width: f32, fmt: impl Fn(f32) -> String) -> bool {
    let (lo, hi) = (*range.start(), *range.end());
    let chip_w = 78.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 40.0), Sense::click_and_drag());
    let enabled = ui.is_enabled();
    let track_left = rect.left() + 12.0;
    let track_right = rect.right() - chip_w - 14.0;
    let cy = rect.center().y;
    let mut changed = false;
    if (resp.dragged() || resp.clicked()) && track_right > track_left {
        if let Some(p) = resp.interact_pointer_pos() {
            let t = ((p.x - track_left) / (track_right - track_left)).clamp(0.0, 1.0);
            let nv = lo + t * (hi - lo);
            if (nv - *value).abs() > f32::EPSILON {
                *value = nv;
                changed = true;
            }
        }
    }
    *value = value.clamp(lo, hi);
    let t = if hi > lo { ((*value - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
    let tx = track_left + t * (track_right - track_left);
    let h = if enabled { hover_t(ui, &resp) } else { 0.0 };
    let accent = if enabled { theme::PRIMARY } else { alpha(theme::ON_SURFACE_VAR, 0.4) };
    let p = ui.painter();
    let th = 4.0;
    p.rect_filled(Rect::from_min_max(Pos2::new(track_left, cy - th), Pos2::new(track_right, cy + th)), th, Color32::from_rgb(22, 26, 32));
    p.rect_filled(Rect::from_min_max(Pos2::new(track_left, cy - th), Pos2::new(tx, cy + th)), th, accent);
    p.circle_filled(Pos2::new(tx, cy), 11.0 + 2.0 * h, accent);
    p.circle_filled(Pos2::new(tx, cy), 5.0, Color32::WHITE);
    let chip = Rect::from_min_size(Pos2::new(rect.right() - chip_w, cy - 16.0), egui::vec2(chip_w, 32.0));
    p.rect_filled(chip, 10.0, theme::SURFACE_CONTAINER_HIGH);
    p.text(chip.center(), Align2::CENTER_CENTER, fmt(*value), FontId::proportional(14.5), if enabled { Color32::WHITE } else { theme::ON_SURFACE_VAR });
    changed
}

/// A calm placeholder for an empty list: big glyph, a line, a hint.
pub(super) fn empty_state(ui: &mut egui::Ui, glyph: &str, title: &str, hint: &str) {
    ui.add_space(26.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(glyph).size(40.0).color(alpha(theme::ON_SURFACE_VAR, 0.7)));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(title).size(17.0).color(theme::ON_SURFACE));
        if !hint.is_empty() {
            ui.label(egui::RichText::new(hint).size(13.5).color(theme::ON_SURFACE_VAR));
        }
    });
    ui.add_space(26.0);
}

/// A status badge led by a dot (e.g. "Running").
pub(super) fn live_badge(ui: &mut egui::Ui, text: &str, accent: Color32) {
    let g = ui.fonts(|f| f.layout_no_wrap(text.to_string(), FontId::proportional(12.5), accent));
    let (rect, _) = ui.allocate_exact_size(g.size() + egui::vec2(34.0, 8.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same((rect.height() / 2.0) as u8), alpha(accent, 0.16));
    p.circle_filled(Pos2::new(rect.left() + 13.0, rect.center().y), 4.0, accent);
    p.galley(rect.min + egui::vec2(24.0, 4.0), g, accent);
}

/// A small status badge (e.g. "Active", "Frozen"), `accent`-tinted.
pub(super) fn badge(ui: &mut egui::Ui, text: &str, accent: Color32) {
    let g = ui.fonts(|f| f.layout_no_wrap(text.to_string(), FontId::proportional(12.5), accent));
    let (rect, _) = ui.allocate_exact_size(g.size() + egui::vec2(18.0, 8.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same((rect.height() / 2.0) as u8), alpha(accent, 0.16));
    ui.painter().galley(rect.min + egui::vec2(9.0, 4.0), g, accent);
}
