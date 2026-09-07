//! The screen "island": a small pill floating just above the top edge of a
//! mirrored screen with one numbered button per approved screen, the current
//! one lit (outside the image, so the desktop stays fully clickable).
//! Tapping another number swaps that screen into this spot (same pose, size,
//! curve and docking) so a screen can be replaced without hiding it and
//! spawning the other. Shown for a couple of seconds when a screen appears,
//! then whenever the laser aims near the top-centre of the screen.
use std::time::{Duration, Instant};

use egui_phosphor::regular as icon;

use crate::gfx::theme;

/// How long the island stays after a screen appears / the laser leaves it.
pub const LINGER: Duration = Duration::from_millis(2000);
/// Fade-out at the end of its stay.
const FADE_SECS: f32 = 0.35;
/// Physical height of the island, metres; width follows the panel aspect.
pub const HEIGHT_M: f32 = 0.042;
/// Gap between the screen's top edge and the island (it sits above the screen).
pub const TOP_GAP_M: f32 = 0.012;
/// The island floats this far in front of the screen surface.
pub const FWD_M: f32 = 0.006;
/// Screen zone (fractions of its size, from the top-centre) that reveals it —
/// the top band of the image, so it pops up as the laser approaches; aiming
/// at the island's own spot above the screen reveals it too.
pub const REVEAL_V: f32 = 0.12;
pub const REVEAL_HALF_U: f32 = 0.30;

/// Fixed swapchain; the island's content is centred, the rest transparent.
pub const PANEL_PX: (u32, u32) = (480, 60);
const BTN_W: f32 = 44.0; // egui points
const BTN_H: f32 = 30.0;
const GAP: f32 = 6.0;
const PAD: f32 = 8.0;

pub fn size_m() -> (f32, f32) {
    (HEIGHT_M * PANEL_PX.0 as f32 / PANEL_PX.1 as f32, HEIGHT_M)
}

/// Fraction of the panel width the pill actually covers for `n` buttons —
/// laser hits outside it fall through to the screen.
pub fn content_frac(n: usize) -> f32 {
    let w = PAD * 2.0 + n as f32 * BTN_W + n.saturating_sub(1) as f32 * GAP;
    (w / (PANEL_PX.0 as f32 / crate::gfx::PPP)).min(1.0)
}

/// Opacity for an island that stays visible until `until` (None = hidden).
pub fn alpha(until: Option<Instant>, now: Instant) -> f32 {
    let Some(u) = until else { return 0.0 };
    if now >= u {
        return 0.0;
    }
    let left = (u - now).as_secs_f32();
    (left / FADE_SECS).clamp(0.0, 1.0)
}

/// Draw the pill. `items` = (screen index, label) in bar order; returns the
/// screen index of a tapped button that isn't the current one.
pub fn build(ctx: &egui::Context, items: &[(usize, String)], current: usize, alpha: f32) -> Option<usize> {
    let mut picked = None;
    egui::Area::new(egui::Id::new("island")).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).show(ctx, |ui| {
        ui.set_opacity(alpha);
        egui::Frame::default()
            .fill(egui::Color32::from_rgba_unmultiplied(12, 15, 20, 236))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 110, 120)))
            .corner_radius(17)
            .inner_margin(egui::Margin::symmetric(PAD as i8, 5))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                ui.horizontal(|ui| {
                    for (n, (si, name)) in items.iter().enumerate() {
                        let on = *si == current;
                        let fg = if on { egui::Color32::BLACK } else { theme::ON_SURFACE };
                        let fill = if on { theme::PRIMARY } else { egui::Color32::from_rgb(34, 40, 48) };
                        let b = egui::Button::new(egui::RichText::new(format!("{} {}", icon::MONITOR, n + 1)).size(13.0).color(fg))
                            .fill(fill)
                            .corner_radius(11)
                            .min_size(egui::vec2(BTN_W, BTN_H));
                        let tip = if on { format!("{name} · shown here") } else { format!("Swap in {name}") };
                        if ui.add(b).on_hover_text(tip).clicked() && !on {
                            picked = Some(*si);
                        }
                    }
                });
            });
    });
    picked
}
