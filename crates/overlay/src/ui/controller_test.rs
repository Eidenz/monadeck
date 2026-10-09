// Settings › Controller test: SteamVR's "Test controller", both hands side by
// side. Each drawing rings the part under your finger and fills it as far as
// it's pressed; sticks and trackpads carry a dot where the thumb is. Beside
// and under it, every value as the runtime reports it.
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind};
use monadeck_core::bindings::{self as core, own, Hand};
use std::collections::HashMap;

use super::bindings_page::{art_texture, controller_glyph};
use super::{kit, LibState, TestHand, RUNNING_GREEN};
use crate::gfx::theme;

const ART_W: f32 = 150.0;
const ART_H: f32 = 210.0;
/// A stick's or trackpad's dial.
const DIAL: f32 = 96.0;
const ROW_H: f32 = 34.0;

/// What a controller can report, past the trigger, grip, stick and buttons.
struct Has {
    touch: bool,
    trigger_click: bool,
    grip_force: bool,
    trackpad: bool,
    thumbrest: bool,
    /// The face buttons and the system button as this hand labels them; no
    /// system button where it's the headset's own.
    a: &'static str,
    b: &'static str,
    system: Option<&'static str>,
}

fn has(ty: &str, hand: Hand) -> Has {
    let left = hand == Hand::Left;
    match ty {
        "oculus_touch" => Has {
            touch: true,
            trigger_click: false,
            grip_force: false,
            trackpad: false,
            thumbrest: true,
            a: if left { "X" } else { "A" },
            b: if left { "Y" } else { "B" },
            system: left.then_some("Menu"),
        },
        own::GLOVES => Has { touch: false, trigger_click: false, grip_force: false, trackpad: false, thumbrest: false, a: "A", b: "B", system: Some("System") },
        _ => Has { touch: true, trigger_click: true, grip_force: true, trackpad: true, thumbrest: false, a: "A", b: "B", system: Some("System") },
    }
}

fn controller_name(ty: &str) -> &'static str {
    match ty {
        "knuckles" => "Index controller",
        "oculus_touch" => "Touch controller",
        own::GLOVES => "UdCap glove",
        "vive_controller" => "Vive wand",
        _ => "Controller",
    }
}

pub(super) fn controller_test(ui: &mut egui::Ui, st: &mut LibState) {
    let w = ui.available_width();
    let col = (w - 12.0) / 2.0;
    ui.add_space(8.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        for (hi, hand) in [Hand::Left, Hand::Right].into_iter().enumerate() {
            let input = st.test_hands[hi];
            let ty = st.test_types[hi];
            ui.allocate_ui_with_layout(egui::vec2(col, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(col);
                hand_card(ui, &mut st.binds.art, hand, ty, &input);
            });
        }
    });
}

fn hand_card(ui: &mut egui::Ui, cache: &mut HashMap<&'static str, egui::TextureHandle>, hand: Hand, ty: &'static str, h: &TestHand) {
    let has = has(ty, hand);
    egui::Frame::default()
        .fill(theme::SURFACE_CONTAINER)
        .stroke(Stroke::new(1.0f32, kit::alpha(Color32::WHITE, 0.05)))
        .corner_radius(18)
        .inner_margin(egui::Margin::same(18))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            header(ui, hand, ty, h.active);
            ui.add_space(4.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                drawing(ui, cache, hand, ty, h, has.touch);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 18.0;
                    ui.add_space(4.0);
                    let stick_touch = if has.touch { Some(h.stick_touch) } else { None };
                    dial(ui, "Thumbstick", h.stick, true, stick_touch, Some(h.stick_click), None);
                    if has.trackpad {
                        dial(ui, "Trackpad", h.pad, h.pad_touch, Some(h.pad_touch), None, Some(h.pad_force));
                    }
                });
            });
            ui.add_space(6.0);
            let touch = has.touch.then_some(h.trigger_touch);
            let click = has.trigger_click.then_some(h.trigger_click);
            value_row(ui, "Trigger", h.trigger, &[("Touch", touch), ("Click", click)]);
            value_row(ui, "Grip", h.grip, &[]);
            if has.grip_force {
                value_row(ui, "Squeeze", h.grip_force, &[]);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let touch = |t: bool| has.touch && t;
                button_chip(ui, has.a, touch(h.a_touch), h.a);
                button_chip(ui, has.b, touch(h.b_touch), h.b);
                if let Some(label) = has.system {
                    button_chip(ui, label, touch(h.system_touch), h.system);
                }
                if has.thumbrest {
                    button_chip(ui, "Thumbrest", h.thumbrest, false);
                }
            });
        });
}

fn header(ui: &mut egui::Ui, hand: Hand, ty: &str, active: bool) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(controller_glyph(ty, hand)).size(24.0).color(theme::PRIMARY));
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let side = if hand == Hand::Left { "Left hand" } else { "Right hand" };
            ui.label(egui::RichText::new(side).size(17.0).strong().color(Color32::WHITE));
            ui.label(egui::RichText::new(controller_name(ty)).size(13.0).color(theme::ON_SURFACE_VAR));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if active {
                kit::live_badge(ui, "Tracked", RUNNING_GREEN);
            } else {
                kit::badge(ui, "Not tracked", theme::ON_SURFACE_VAR);
            }
        });
    });
}

/// How one part of the drawing shows: ringed, filled, a thumb dot.
struct Spot {
    touch: bool,
    press: f32,
    dot: Option<(f32, f32)>,
}

fn spot(ty: &str, hand: Hand, id: &str, h: &TestHand, sensors: bool) -> Option<Spot> {
    // Only the parts this hand has (Touch: X, Y and the menu on the left).
    // The thumbrest isn't bindable, so it's on no list.
    if id != "thumbrest" {
        let def = own::controller(ty)?.input(id)?;
        if !def.on(hand) {
            return None;
        }
    }
    let moved = |(x, y): (f32, f32)| x.hypot(y) > 0.05;
    // Gloves have no touch sensors: only what moves lights up.
    let sensed = |t: bool| sensors && t;
    let button = |touch: bool, press: bool| Spot { touch, press: if press { 1.0 } else { 0.0 }, dot: None };
    Some(match id {
        "trigger" => Spot { touch: sensed(h.trigger_touch) || h.trigger > 0.02, press: h.trigger, dot: None },
        "grip" => Spot { touch: h.grip > 0.02, press: h.grip.max(h.grip_force), dot: None },
        "thumbstick" | "joystick" => Spot { touch: sensed(h.stick_touch) || moved(h.stick), press: if h.stick_click { 1.0 } else { 0.0 }, dot: Some(h.stick) },
        "trackpad" => Spot { touch: h.pad_touch, press: h.pad_force, dot: h.pad_touch.then_some(h.pad) },
        "a" | "x" => button(sensed(h.a_touch), h.a),
        "b" | "y" => button(sensed(h.b_touch), h.b),
        // Touch's right system button belongs to the headset.
        "system" if ty == "oculus_touch" => return None,
        "system" | "application_menu" => button(sensed(h.system_touch), h.system),
        "thumbrest" if ty == "oculus_touch" => button(h.thumbrest, false),
        _ => return None,
    })
}

fn drawing(ui: &mut egui::Ui, cache: &mut HashMap<&'static str, egui::TextureHandle>, hand: Hand, ty: &'static str, h: &TestHand, sensors: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ART_W, ART_H), Sense::hover());
    let p = ui.painter();
    let Some(tex) = art_texture(ui.ctx(), cache, ty) else {
        p.text(rect.center(), Align2::CENTER_CENTER, controller_glyph(ty, hand), FontId::proportional(64.0), theme::ON_SURFACE_VAR);
        return;
    };
    let [tw, th] = tex.size();
    let s = (rect.width() / tw as f32).min(rect.height() / th as f32);
    let art = Rect::from_center_size(rect.center(), egui::vec2(tw as f32 * s, th as f32 * s));
    let uv = if hand == Hand::Left { Rect::from_min_max(Pos2::new(1.0, 0.0), Pos2::new(0.0, 1.0)) } else { Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)) };
    p.image(tex.id(), art, uv, kit::alpha(theme::ON_SURFACE, if h.active { 0.92 } else { 0.35 }));
    for &(id, fx, fy, fr) in core::spots(ty) {
        let Some(sp) = spot(ty, hand, id, h, sensors) else { continue };
        let x = if hand == Hand::Left { 1.0 - fx } else { fx };
        let c = Pos2::new(art.left() + x * art.width(), art.top() + fy * art.height());
        let r = (fr * art.width()).max(7.0);
        p.circle_stroke(c, r, Stroke::new(1.0f32, kit::alpha(theme::ON_SURFACE_VAR, 0.3)));
        if sp.press > 0.01 {
            p.circle_filled(c, r, kit::alpha(theme::PRIMARY, 0.25 + 0.6 * sp.press.clamp(0.0, 1.0)));
        }
        if sp.touch {
            p.circle_stroke(c, r + 2.0, Stroke::new(2.0f32, theme::PRIMARY));
        }
        if let Some((dx, dy)) = sp.dot {
            p.circle_filled(c + egui::vec2(dx, -dy) * r * 0.8, 3.0, Color32::WHITE);
        }
        if ty == "oculus_touch" && matches!(id, "a" | "b" | "x" | "y") {
            // The drawing's buttons carry no letters (it's mirrored).
            p.text(c, Align2::CENTER_CENTER, id.to_uppercase(), FontId::proportional(11.0), theme::ON_SURFACE);
        }
    }
}

/// A stick or trackpad: a dial with the thumb's dot, its position, and its
/// touch / click (or force) beside it.
fn dial(ui: &mut egui::Ui, label: &str, (x, y): (f32, f32), show_dot: bool, touch: Option<bool>, click: Option<bool>, force: Option<f32>) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, DIAL), Sense::hover());
    let p = ui.painter();
    let c = Pos2::new(rect.left() + DIAL / 2.0, rect.center().y);
    let r = DIAL / 2.0 - 2.0;
    p.circle_filled(c, r, theme::SURFACE_CONTAINER_HIGH);
    let faint = Stroke::new(1.0f32, kit::alpha(Color32::WHITE, 0.08));
    p.line_segment([c - egui::vec2(r, 0.0), c + egui::vec2(r, 0.0)], faint);
    p.line_segment([c - egui::vec2(0.0, r), c + egui::vec2(0.0, r)], faint);
    if touch == Some(true) {
        p.circle_stroke(c, r, Stroke::new(2.0f32, theme::PRIMARY));
    }
    if show_dot {
        let d = c + egui::vec2(x.clamp(-1.0, 1.0), -y.clamp(-1.0, 1.0)) * (r - 7.0);
        p.line_segment([c, d], Stroke::new(2.0f32, kit::alpha(theme::PRIMARY, 0.5)));
        p.circle_filled(d, 7.0, if click == Some(true) { theme::PRIMARY } else { Color32::WHITE });
    }
    let tx = rect.left() + DIAL + 16.0;
    p.text(Pos2::new(tx, rect.top() + 14.0), Align2::LEFT_CENTER, label, FontId::proportional(15.0), theme::ON_SURFACE);
    p.text(Pos2::new(tx, rect.top() + 38.0), Align2::LEFT_CENTER, format!("x {x:+.2}   y {y:+.2}"), FontId::monospace(13.0), theme::ON_SURFACE_VAR);
    let mut fx = tx;
    let fy = rect.top() + 68.0;
    for (name, on) in [("Touch", touch), ("Click", click)] {
        if let Some(on) = on {
            fx += flag(p, Pos2::new(fx, fy), name, on) + 6.0;
        }
    }
    if let Some(f) = force {
        let g = p.layout_no_wrap(format!("Force {f:.2}"), FontId::monospace(12.5), if f > 0.01 { theme::PRIMARY } else { theme::ON_SURFACE_VAR });
        p.galley(Pos2::new(fx + 2.0, fy - g.size().y / 2.0), g, Color32::WHITE);
    }
}

/// A 0..1 input as a bar, its value, and its touch / click flags.
fn value_row(ui: &mut egui::Ui, label: &str, v: f32, flags: &[(&str, Option<bool>)]) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, ROW_H), Sense::hover());
    let p = ui.painter();
    let cy = rect.center().y;
    p.text(Pos2::new(rect.left(), cy), Align2::LEFT_CENTER, label, FontId::proportional(15.0), theme::ON_SURFACE);
    let flags_w: f32 = flags.iter().filter(|(_, on)| on.is_some()).count() as f32 * 62.0;
    let bar = Rect::from_min_max(Pos2::new(rect.left() + 86.0, cy - 5.0), Pos2::new(rect.right() - flags_w - 62.0, cy + 5.0));
    p.rect_filled(bar, CornerRadius::same(5), theme::SURFACE_CONTAINER_HIGH);
    let v = v.clamp(0.0, 1.0);
    if v > 0.0 {
        let fill = Rect::from_min_max(bar.min, Pos2::new(bar.left() + bar.width() * v, bar.bottom()));
        p.rect_filled(fill, CornerRadius::same(5), theme::PRIMARY);
    }
    p.text(Pos2::new(bar.right() + 10.0, cy), Align2::LEFT_CENTER, format!("{v:.2}"), FontId::monospace(13.0), theme::ON_SURFACE_VAR);
    let mut fx = rect.right() - flags_w + 6.0;
    for (name, on) in flags {
        if let Some(on) = on {
            fx += flag(p, Pos2::new(fx, cy), name, *on) + 6.0;
        }
    }
}

/// A small "Touch" / "Click" pill at `left_center`, lit while on. Returns its width.
fn flag(p: &egui::Painter, left_center: Pos2, text: &str, on: bool) -> f32 {
    let color = if on { theme::PRIMARY } else { kit::alpha(theme::ON_SURFACE_VAR, 0.55) };
    let g = p.layout_no_wrap(text.to_string(), FontId::proportional(12.0), color);
    let r = Rect::from_min_size(Pos2::new(left_center.x, left_center.y - 11.0), egui::vec2(g.size().x + 16.0, 22.0));
    if on {
        p.rect_filled(r, CornerRadius::same(11), kit::alpha(theme::PRIMARY, 0.16));
    } else {
        p.rect_stroke(r, CornerRadius::same(11), Stroke::new(1.0f32, kit::alpha(Color32::WHITE, 0.08)), StrokeKind::Inside);
    }
    p.galley(Pos2::new(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, color);
    r.width()
}

/// A face or system button: ringed while touched, filled while pressed.
fn button_chip(ui: &mut egui::Ui, label: &str, touch: bool, press: bool) {
    let (fill, text) = if press { (theme::PRIMARY, theme::SURFACE) } else { (theme::SURFACE_CONTAINER_HIGH, theme::ON_SURFACE) };
    let g = ui.fonts(|f| f.layout_no_wrap(label.to_string(), FontId::proportional(14.0), text));
    let (r, _) = ui.allocate_exact_size(egui::vec2((g.size().x + 30.0).max(58.0), 36.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(12), fill);
    if touch && !press {
        p.rect_stroke(r, CornerRadius::same(12), Stroke::new(2.0f32, theme::PRIMARY), StrokeKind::Inside);
    }
    p.galley(r.center() - g.size() / 2.0, g, text);
}
