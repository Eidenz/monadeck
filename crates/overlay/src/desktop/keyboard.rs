//! The VR keyboard: a full classic layout (F-row, ISO main block, nav cluster,
//! numpad) plus Copy/Cut/Paste, rendered with egui on its own layer and typed
//! into the desktop through the virtual uinput keyboard.
//!
//! Modifiers are one-shot latches (tap Shift, tap a key); Caps Lock is a real
//! toggle. The keyboard floats freely, and docks under a mirrored screen when
//! released near its bottom edge, following the screen from then on.
use std::time::Instant;

use egui_phosphor::regular as icon;
use openxr as xr;

use super::keymap::{self, KeyLabels};
use crate::gfx::{theme, PPP};
use crate::mathx::{pose_compose, quat_from_axis_angle, quatf, vec3f};

// evdev codes (linux/input-event-codes.h)
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_CAPSLOCK: u16 = 58;
pub const KEY_RIGHTALT: u16 = 100;
pub const KEY_LEFTMETA: u16 = 125;
const KEY_C: u16 = 46;
const KEY_V: u16 = 47;
const KEY_X: u16 = 45;

pub const MOD_SHIFT: u8 = 1;
pub const MOD_CTRL: u8 = 2;
pub const MOD_ALT: u8 = 4;
pub const MOD_SUPER: u8 = 8;
pub const MOD_ALTGR: u8 = 16;

pub fn mod_code(m: u8) -> u16 {
    match m {
        MOD_SHIFT => KEY_LEFTSHIFT,
        MOD_CTRL => KEY_LEFTCTRL,
        MOD_ALT => KEY_LEFTALT,
        MOD_SUPER => KEY_LEFTMETA,
        _ => KEY_RIGHTALT,
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum KeyKind {
    /// Label from the xkb keymap (letters, digits, punctuation).
    Mapped,
    /// Fixed label (Esc, F-keys, nav, numpad, Space…).
    Fixed(&'static str),
    Modifier(u8),
    Caps,
    /// Chord shortcuts.
    Copy,
    Cut,
    Paste,
}

pub struct KeyDef {
    pub code: u16,
    pub kind: KeyKind,
    /// Position/size in key units.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub enum KeyAction {
    /// Press+release `code` with the given modifier mask held.
    Tap { code: u16, mods: u8 },
    /// Press+release a modifier key on its own (Caps Lock toggle).
    Toggle(u16),
}

/// Key unit in egui points; panel pixels = points * PPP.
const U: f32 = 40.0;
const MARGIN: f32 = 12.0;
const TOP_BAR: f32 = 36.0;
const COLS: f32 = 23.0;
const ROWS: f32 = 6.25;
/// Physical width of the keyboard, metres.
pub const WIDTH_M: f32 = 0.85;
/// Dock tilt below a screen (radians; negative = face tilts up toward you).
const DOCK_TILT: f32 = -0.30;
const DOCK_GAP: f32 = 0.03;
const DOCK_FWD: f32 = 0.04;
/// Release the keyboard within this distance of a screen's dock to attach.
pub const DOCK_SNAP_M: f32 = 0.18;
/// Tapping a latched modifier again within this window sends it on its own
/// (e.g. a solo Super press opens the app launcher).
const MOD_DOUBLE_TAP: f32 = 1.5;

pub fn panel_points() -> (f32, f32) {
    (COLS * U + MARGIN * 2.0, TOP_BAR + ROWS * U + MARGIN * 2.0)
}

pub fn panel_px() -> (u32, u32) {
    let (w, h) = panel_points();
    ((w * PPP).round() as u32, (h * PPP).round() as u32)
}

pub fn size_m() -> (f32, f32) {
    let (w, h) = panel_points();
    (WIDTH_M, WIDTH_M * h / w)
}

pub struct KeyboardState {
    pub keys: Vec<KeyDef>,
    pub labels: KeyLabels,
    pub visible: bool,
    pub pose: xr::Posef,
    /// Placed at least once (else the next show positions it).
    pub placed: bool,
    /// Docked under this screen (index into the viewer's screens).
    pub attached: Option<usize>,
    pub latched: u8,
    pub caps: bool,
    pub pending: Vec<KeyAction>,
    pub grab: Option<(usize, xr::Posef)>,
    /// UI requests drained by the viewer.
    pub close_request: bool,
    pub detach_request: bool,
    pub attach_request: bool,
    /// Which key is pressed this frame (for the click sound / haptics).
    pub clicked: bool,
    /// Last modifier latched + when (double-tap detection).
    latch_at: Option<(u8, Instant)>,
    /// Clipboard text preview for the top bar (set by the viewer).
    pub clipboard: Option<String>,
    /// Top bar asked to switch to this layout index.
    pub layout_switch_request: Option<usize>,
}

impl KeyboardState {
    pub fn new() -> Self {
        Self {
            keys: layout(),
            labels: keymap::load(),
            visible: false,
            pose: xr::Posef::IDENTITY,
            placed: false,
            attached: None,
            latched: 0,
            caps: false,
            pending: Vec::new(),
            grab: None,
            close_request: false,
            detach_request: false,
            attach_request: false,
            clicked: false,
            latch_at: None,
            clipboard: None,
            layout_switch_request: None,
        }
    }

    /// Pose docked under a screen of `screen_size` at `screen_pose`.
    pub fn dock_pose(screen_pose: &xr::Posef, screen_size: (f32, f32)) -> xr::Posef {
        let kb = size_m();
        let local = xr::Posef {
            orientation: quatf(quat_from_axis_angle([1.0, 0.0, 0.0], DOCK_TILT)),
            position: vec3f([0.0, -(screen_size.1 / 2.0 + DOCK_GAP + kb.1 / 2.0 * DOCK_TILT.cos()), DOCK_FWD]),
        };
        pose_compose(screen_pose, &local)
    }
}

fn layout() -> Vec<KeyDef> {
    use KeyKind::*;
    let mut keys = Vec::new();
    fn push(keys: &mut Vec<KeyDef>, code: u16, kind: KeyKind, x: f32, y: f32, w: f32, h: f32) {
        keys.push(KeyDef { code, kind, x, y, w, h });
    }
    // Row layout helper: consecutive keys from x0.
    fn row(keys: &mut Vec<KeyDef>, y: f32, x0: f32, items: &[(u16, KeyKind, f32)]) {
        let mut x = x0;
        for &(code, kind, w) in items {
            keys.push(KeyDef { code, kind, x, y, w, h: 1.0 });
            x += w;
        }
    }
    // --- F row ---
    push(&mut keys, 1, Fixed("Esc"), 0.0, 0.0, 1.0, 1.0);
    for (i, code) in (59..=62).enumerate() {
        push(&mut keys, code, Fixed(FKEYS[i]), 2.0 + i as f32, 0.0, 1.0, 1.0);
    }
    for (i, code) in (63..=66).enumerate() {
        push(&mut keys, code, Fixed(FKEYS[4 + i]), 6.5 + i as f32, 0.0, 1.0, 1.0);
    }
    for (i, code) in [67u16, 68, 87, 88].into_iter().enumerate() {
        push(&mut keys, code, Fixed(FKEYS[8 + i]), 11.0 + i as f32, 0.0, 1.0, 1.0);
    }
    let y1 = 1.25;
    // --- Main block ---
    row(&mut keys, y1, 0.0, &[
        (41, Mapped, 1.0), (2, Mapped, 1.0), (3, Mapped, 1.0), (4, Mapped, 1.0), (5, Mapped, 1.0), (6, Mapped, 1.0),
        (7, Mapped, 1.0), (8, Mapped, 1.0), (9, Mapped, 1.0), (10, Mapped, 1.0), (11, Mapped, 1.0), (12, Mapped, 1.0),
        (13, Mapped, 1.0), (14, Fixed(icon::BACKSPACE), 2.0),
    ]);
    row(&mut keys, y1 + 1.0, 0.0, &[
        (15, Fixed("Tab"), 1.5), (16, Mapped, 1.0), (17, Mapped, 1.0), (18, Mapped, 1.0), (19, Mapped, 1.0), (20, Mapped, 1.0),
        (21, Mapped, 1.0), (22, Mapped, 1.0), (23, Mapped, 1.0), (24, Mapped, 1.0), (25, Mapped, 1.0), (26, Mapped, 1.0),
        (27, Mapped, 1.0), (43, Mapped, 1.5),
    ]);
    row(&mut keys, y1 + 2.0, 0.0, &[
        (58, Caps, 1.75), (30, Mapped, 1.0), (31, Mapped, 1.0), (32, Mapped, 1.0), (33, Mapped, 1.0), (34, Mapped, 1.0),
        (35, Mapped, 1.0), (36, Mapped, 1.0), (37, Mapped, 1.0), (38, Mapped, 1.0), (39, Mapped, 1.0), (40, Mapped, 1.0),
        (28, Fixed("Enter"), 2.25),
    ]);
    row(&mut keys, y1 + 3.0, 0.0, &[
        (42, Modifier(MOD_SHIFT), 1.25), (86, Mapped, 1.0), (44, Mapped, 1.0), (45, Mapped, 1.0), (46, Mapped, 1.0),
        (47, Mapped, 1.0), (48, Mapped, 1.0), (49, Mapped, 1.0), (50, Mapped, 1.0), (51, Mapped, 1.0), (52, Mapped, 1.0),
        (53, Mapped, 1.0), (54, Modifier(MOD_SHIFT), 2.75),
    ]);
    row(&mut keys, y1 + 4.0, 0.0, &[
        (29, Modifier(MOD_CTRL), 1.5), (125, Modifier(MOD_SUPER), 1.25), (56, Modifier(MOD_ALT), 1.25),
        (57, Fixed(""), 7.0), (100, Modifier(MOD_ALTGR), 1.25), (127, Fixed("Menu"), 1.25), (97, Modifier(MOD_CTRL), 1.5),
    ]);
    // --- Nav cluster ---
    let nx = 15.5;
    row(&mut keys, 0.0, nx, &[(99, Fixed("PrtSc"), 1.0), (70, Fixed("ScrLk"), 1.0), (119, Fixed("Pause"), 1.0)]);
    row(&mut keys, y1, nx, &[(110, Fixed("Ins"), 1.0), (102, Fixed("Home"), 1.0), (104, Fixed("PgUp"), 1.0)]);
    row(&mut keys, y1 + 1.0, nx, &[(111, Fixed("Del"), 1.0), (107, Fixed("End"), 1.0), (109, Fixed("PgDn"), 1.0)]);
    push(&mut keys, 103, Fixed(icon::ARROW_UP), nx + 1.0, y1 + 3.0, 1.0, 1.0);
    row(&mut keys, y1 + 4.0, nx, &[(105, Fixed(icon::ARROW_LEFT), 1.0), (108, Fixed(icon::ARROW_DOWN), 1.0), (106, Fixed(icon::ARROW_RIGHT), 1.0)]);
    // --- Numpad + shortcuts ---
    let px = 19.0;
    row(&mut keys, 0.0, px, &[(0, Copy, 4.0 / 3.0), (0, Cut, 4.0 / 3.0), (0, Paste, 4.0 / 3.0)]);
    row(&mut keys, y1, px, &[(69, Fixed("Num"), 1.0), (98, Fixed("/"), 1.0), (55, Fixed("*"), 1.0), (74, Fixed("-"), 1.0)]);
    row(&mut keys, y1 + 1.0, px, &[(71, Fixed("7"), 1.0), (72, Fixed("8"), 1.0), (73, Fixed("9"), 1.0)]);
    push(&mut keys, 78, Fixed("+"), px + 3.0, y1 + 1.0, 1.0, 2.0);
    row(&mut keys, y1 + 2.0, px, &[(75, Fixed("4"), 1.0), (76, Fixed("5"), 1.0), (77, Fixed("6"), 1.0)]);
    row(&mut keys, y1 + 3.0, px, &[(79, Fixed("1"), 1.0), (80, Fixed("2"), 1.0), (81, Fixed("3"), 1.0)]);
    push(&mut keys, 96, Fixed("Enter"), px + 3.0, y1 + 3.0, 1.0, 2.0);
    row(&mut keys, y1 + 4.0, px, &[(82, Fixed("0"), 2.0), (83, Fixed("."), 1.0)]);
    keys
}

const FKEYS: [&str; 12] = ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12"];

fn mod_name(m: u8) -> &'static str {
    match m {
        MOD_SHIFT => "Shift",
        MOD_CTRL => "Ctrl",
        MOD_ALT => "Alt",
        MOD_SUPER => "Super",
        _ => "AltGr",
    }
}

/// Main + small secondary label for a mapped key under the current latch.
fn mapped_labels(labels: &KeyLabels, code: u16, latched: u8) -> (String, String) {
    let Some(l) = labels.current().get(&code) else { return (String::new(), String::new()) };
    let level = if latched & MOD_ALTGR != 0 { 2 } else if latched & MOD_SHIFT != 0 { 1 } else { 0 };
    let mut main = l[level].clone();
    let is_letter = l[0].chars().count() == 1 && l[0].chars().all(char::is_alphabetic);
    if is_letter && level == 0 {
        // Letters read as capitals, like the caps on a physical keyboard.
        main = l[0].to_uppercase();
    }
    let secondary = if level == 0 && !is_letter && l[1] != l[0] { l[1].clone() } else { String::new() };
    (main, secondary)
}

/// Draw the keyboard; presses are queued in `st.pending`.
pub fn build(ctx: &egui::Context, st: &mut KeyboardState) {
    st.clicked = false;
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(16, 20, 26))
        .corner_radius(16)
        .inner_margin(egui::Margin::same(MARGIN as i8));
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        // --- Top bar: layout · latched modifiers · dock/close -----------------
        ui.horizontal(|ui| {
            ui.set_height(TOP_BAR - 8.0);
            ui.label(egui::RichText::new(icon::KEYBOARD).size(18.0).color(theme::PRIMARY));
            // Layout: a click cycles through the configured layouts (KDE switches too).
            let name = st.labels.layout_names.get(st.labels.current).cloned().unwrap_or_default();
            let many = st.labels.layout_names.len() > 1;
            let lb = egui::Button::new(
                egui::RichText::new(if many { format!("{} {name}", icon::TRANSLATE) } else { name })
                    .size(13.0)
                    .color(theme::ON_SURFACE_VAR),
            )
            .fill(if many { theme::SURFACE_CONTAINER_HIGH } else { egui::Color32::TRANSPARENT })
            .min_size(egui::vec2(0.0, 24.0));
            if ui.add(lb).on_hover_text("Switch keyboard layout").clicked() && many {
                st.layout_switch_request = Some((st.labels.current + 1) % st.labels.layout_names.len());
            }
            ui.add_space(12.0);
            // Clipboard preview: what Paste would insert.
            if let Some(clip) = &st.clipboard {
                let mut preview: String = clip.chars().take(48).collect();
                if clip.chars().count() > 48 {
                    preview.push('…');
                }
                ui.label(egui::RichText::new(icon::CLIPBOARD).size(14.0).color(theme::ON_SURFACE_VAR));
                ui.label(egui::RichText::new(preview).size(12.0).color(theme::ON_SURFACE_VAR));
                ui.add_space(12.0);
            }
            for m in [MOD_SHIFT, MOD_CTRL, MOD_ALT, MOD_SUPER, MOD_ALTGR] {
                if st.latched & m != 0 {
                    chip(ui, mod_name(m), theme::PRIMARY);
                }
            }
            if st.caps {
                chip(ui, "Caps", egui::Color32::from_rgb(232, 188, 84));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if small_button(ui, icon::X, "Close").clicked() {
                    st.close_request = true;
                }
                ui.add_space(4.0);
                if st.attached.is_some() {
                    if small_button(ui, icon::LINK_BREAK, "Detach from screen").clicked() {
                        st.detach_request = true;
                    }
                } else if small_button(ui, icon::LINK, "Dock under nearest screen").clicked() {
                    st.attach_request = true;
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(if st.attached.is_some() { "docked" } else { "floating · grip to move" })
                        .size(12.0)
                        .color(theme::ON_SURFACE_VAR),
                );
            });
        });
        // --- Keys ---------------------------------------------------------------
        let origin = egui::pos2(ui.max_rect().min.x, ui.max_rect().min.y + TOP_BAR);
        let mut action: Option<KeyAction> = None;
        let mut latch_toggle: Option<u8> = None;
        let mut caps_toggle = false;
        for (i, k) in st.keys.iter().enumerate() {
            let rect = egui::Rect::from_min_size(
                origin + egui::vec2(k.x * U + 2.0, k.y * U + 2.0),
                egui::vec2(k.w * U - 4.0, k.h * U - 4.0),
            );
            let id = ui.id().with(("key", i));
            let resp = ui.interact(rect, id, egui::Sense::click());
            let down = resp.is_pointer_button_down_on();
            let latched_here = matches!(k.kind, KeyKind::Modifier(m) if st.latched & m != 0)
                || (k.kind == KeyKind::Caps && st.caps);
            let (fill, fg) = if down || latched_here {
                (theme::PRIMARY, egui::Color32::BLACK)
            } else if resp.hovered() {
                (egui::Color32::from_rgb(52, 74, 82), egui::Color32::WHITE)
            } else {
                let special = !matches!(k.kind, KeyKind::Mapped) && !matches!(k.kind, KeyKind::Fixed(""));
                let base = if special { egui::Color32::from_rgb(30, 36, 46) } else { egui::Color32::from_rgb(38, 46, 58) };
                (base, theme::ON_SURFACE)
            };
            let painter = ui.painter();
            painter.rect_filled(rect, egui::CornerRadius::same(7), fill);
            painter.rect_stroke(
                rect,
                egui::CornerRadius::same(7),
                egui::Stroke::new(1.0, egui::Color32::from_white_alpha(14)),
                egui::StrokeKind::Inside,
            );
            let (main, secondary, size) = match k.kind {
                KeyKind::Mapped => {
                    let (m, s) = mapped_labels(&st.labels, k.code, st.latched);
                    (m, s, 17.0)
                }
                KeyKind::Fixed(l) => (l.to_string(), String::new(), if l.chars().count() > 2 { 12.0 } else { 16.0 }),
                KeyKind::Modifier(m) => (mod_name(m).to_string(), String::new(), 12.0),
                KeyKind::Caps => ("Caps".into(), String::new(), 12.0),
                KeyKind::Copy => (format!("{} Copy", icon::COPY), String::new(), 12.0),
                KeyKind::Cut => (format!("{} Cut", icon::SCISSORS), String::new(), 12.0),
                KeyKind::Paste => (format!("{} Paste", icon::CLIPBOARD_TEXT), String::new(), 12.0),
            };
            if !secondary.is_empty() {
                painter.text(
                    rect.right_top() + egui::vec2(-6.0, 4.0),
                    egui::Align2::RIGHT_TOP,
                    secondary,
                    egui::FontId::proportional(11.0),
                    if down { egui::Color32::from_black_alpha(160) } else { theme::ON_SURFACE_VAR },
                );
                painter.text(
                    rect.left_bottom() + egui::vec2(7.0, -5.0),
                    egui::Align2::LEFT_BOTTOM,
                    main,
                    egui::FontId::proportional(size),
                    fg,
                );
            } else {
                painter.text(rect.center(), egui::Align2::CENTER_CENTER, main, egui::FontId::proportional(size), fg);
            }
            if resp.clicked() {
                st.clicked = true;
                match k.kind {
                    KeyKind::Modifier(m) => latch_toggle = Some(m),
                    KeyKind::Caps => caps_toggle = true,
                    KeyKind::Copy => action = Some(KeyAction::Tap { code: KEY_C, mods: MOD_CTRL }),
                    KeyKind::Cut => action = Some(KeyAction::Tap { code: KEY_X, mods: MOD_CTRL }),
                    KeyKind::Paste => action = Some(KeyAction::Tap { code: KEY_V, mods: MOD_CTRL }),
                    KeyKind::Mapped | KeyKind::Fixed(_) => {
                        action = Some(KeyAction::Tap { code: k.code, mods: st.latched })
                    }
                }
            }
        }
        if let Some(m) = latch_toggle {
            let double = st.latched & m != 0
                && st.latch_at.is_some_and(|(pm, t)| pm == m && t.elapsed().as_secs_f32() < MOD_DOUBLE_TAP);
            if double {
                // Second tap: send the modifier by itself (solo Super = app launcher).
                st.latched &= !m;
                st.latch_at = None;
                st.pending.push(KeyAction::Toggle(mod_code(m)));
            } else {
                st.latched ^= m;
                st.latch_at = (st.latched & m != 0).then(|| (m, Instant::now()));
            }
        }
        if caps_toggle {
            st.caps = !st.caps;
            st.pending.push(KeyAction::Toggle(KEY_CAPSLOCK));
        }
        if let Some(a) = action {
            st.pending.push(a);
            st.latched = 0; // one-shot
            st.latch_at = None;
        }
    });
}

fn chip(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    let text = egui::RichText::new(label).size(12.0).color(egui::Color32::BLACK);
    egui::Frame::default()
        .fill(color)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(text);
        });
    ui.add_space(4.0);
}

fn small_button(ui: &mut egui::Ui, glyph: &str, tip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(glyph).size(15.0).color(theme::ON_SURFACE))
            .min_size(egui::vec2(30.0, 26.0))
            .fill(theme::SURFACE_CONTAINER_HIGH),
    )
    .on_hover_text(tip)
}
