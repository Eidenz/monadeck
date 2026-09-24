// Photos: the screenshot gallery, how shots are taken, QR codes, and sharing,
// as categories of one page built from the shared kit.
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind};
use egui_phosphor::regular as icon;

use super::kit::*;
use super::LibState;
use crate::gfx::theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PhotosTab {
    Gallery,
    Capture,
    Qr,
    Sharing,
}

impl PhotosTab {
    pub const ALL: [PhotosTab; 4] = [Self::Gallery, Self::Capture, Self::Qr, Self::Sharing];

    fn tab(self) -> Tab {
        match self {
            Self::Gallery => Tab { glyph: icon::IMAGES, label: "Gallery", blurb: "Your in-headset screenshots" },
            Self::Capture => Tab { glyph: icon::CAMERA, label: "Capture", blurb: "The finger-frame gesture, and what happens to a new shot" },
            Self::Qr => Tab { glyph: icon::QR_CODE, label: "QR codes", blurb: "Codes found in a shot open on the wrist" },
            Self::Sharing => Tab { glyph: icon::SHARE_NETWORK, label: "Sharing", blurb: "Translate, share, and where shots are kept" },
        }
    }
}

pub(super) fn photos_page(ui: &mut egui::Ui, st: &mut LibState) {
    let tabs: Vec<Tab> = PhotosTab::ALL.iter().map(|t| t.tab()).collect();
    let current = PhotosTab::ALL.iter().position(|t| *t == st.photos_tab).unwrap_or(0);
    let tab = st.photos_tab;
    let picked = shell(ui, icon::IMAGES, "Photos", &tabs, current, "photos", |ui| match tab {
        PhotosTab::Gallery => gallery(ui, st),
        PhotosTab::Capture => capture(ui, st),
        PhotosTab::Qr => qr(ui, st),
        PhotosTab::Sharing => sharing(ui, st),
    });
    if let Some(i) = picked {
        st.photos_tab = PhotosTab::ALL[i];
        st.sound_tab = true;
    }
}

fn gallery(ui: &mut egui::Ui, st: &mut LibState) {
    let mut req = crate::photos::GalleryRequests::default();
    let (page, pages, total) = (st.gallery_page, st.gallery_pages.max(1), st.gallery_total);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{total} screenshot{}", if total == 1 { "" } else { "s" })).size(16.0).color(theme::ON_SURFACE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if small_icon(ui, icon::ARROWS_CLOCKWISE, "Look for new shots").clicked() {
                req.refresh = true;
            }
            ui.add_space(12.0);
            if icon_btn(ui, icon::CARET_RIGHT, "Next page", false, page + 1 < pages).clicked() {
                req.next = true;
            }
            ui.label(egui::RichText::new(format!("{} / {pages}", page + 1)).size(15.0).color(theme::ON_SURFACE_VAR));
            if icon_btn(ui, icon::CARET_LEFT, "Previous page", false, page > 0).clicked() {
                req.prev = true;
            }
        });
    });
    ui.add_space(12.0);
    if total == 0 {
        card(ui, |ui| empty_state(ui, icon::CAMERA, "No screenshots yet", "Frame a shot with both hands in a game (Capture)"));
    } else if st.gallery_loading {
        card(ui, |ui| {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| ui.add(egui::Spinner::new().size(36.0)));
            ui.add_space(60.0);
        });
    } else {
        const COLS: usize = 4;
        let gap = 14.0;
        let w = ui.available_width();
        let tile_w = (w - gap * (COLS - 1) as f32) / COLS as f32;
        let img_h = tile_w * 10.0 / 16.0;
        let items: Vec<(egui::TextureId, egui::Vec2, String)> = st.gallery_items.iter().map(|(t, when)| (t.id(), t.size_vec2(), when.clone())).collect();
        for chunk in items.chunks(COLS).enumerate() {
            let (r, row_items) = chunk;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (c, (tex, size, when)) in row_items.iter().enumerate() {
                    let k = r * COLS + c;
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(tile_w, img_h + 44.0), Sense::hover());
                    let img = Rect::from_min_size(rect.min, egui::vec2(tile_w, img_h));
                    let open = ui.interact(img, resp.id.with("open"), Sense::click());
                    let h = hover_t(ui, &open);
                    let p = ui.painter();
                    let radius = CornerRadius::same(14);
                    p.rect_filled(img, radius, theme::SURFACE_CONTAINER);
                    // Centre-crop the shot into the tile (object-fit: cover).
                    egui::Image::new(egui::load::SizedTexture::new(*tex, *size))
                        .uv(cover_uv(*size, img.size()))
                        .corner_radius(radius)
                        .paint_at(ui, img);
                    let p = ui.painter();
                    p.rect_stroke(img, radius, Stroke::new(1.5 + h, mix(alpha(Color32::WHITE, 0.08), theme::PRIMARY, h)), StrokeKind::Inside);
                    if open.clicked() {
                        req.open = Some(k);
                    }
                    let foot = Rect::from_min_max(Pos2::new(rect.left(), img.bottom() + 6.0), rect.max);
                    p.text(Pos2::new(foot.left() + 4.0, foot.center().y), Align2::LEFT_CENTER, when, FontId::proportional(13.5), theme::ON_SURFACE_VAR);
                    let key = format!("photo-del:{k}");
                    let armed = st.is_armed(&key);
                    let del = Rect::from_min_size(Pos2::new(foot.right() - 36.0, foot.center().y - 18.0), egui::vec2(36.0, 36.0));
                    let dresp = ui.interact(del, resp.id.with("del"), Sense::click()).on_hover_text(if armed { "Tap again to delete" } else { "Delete" });
                    let dh = hover_t(ui, &dresp);
                    let p = ui.painter();
                    let fill = if armed { theme_red() } else { alpha(Color32::WHITE, 0.07 * dh) };
                    p.rect_filled(del, CornerRadius::same(10), fill);
                    let fg = if armed { Color32::WHITE } else { mix(theme::ON_SURFACE_VAR, Color32::WHITE, dh) };
                    p.text(del.center(), Align2::CENTER_CENTER, icon::TRASH, FontId::proportional(16.0), fg);
                    if dresp.clicked() && st.confirm_tap(&key) {
                        req.delete = Some(k);
                    }
                }
            });
            ui.add_space(gap);
        }
    }
    if req.open.is_some() || req.delete.is_some() || req.prev || req.next || req.refresh {
        st.gallery_req = req;
        st.sound_tab = true;
    }
}

fn theme_red() -> Color32 {
    Color32::from_rgb(224, 78, 78)
}

/// UVs that centre-crop an image of `tex` size into a `dst`-shaped rect.
fn cover_uv(tex: egui::Vec2, dst: egui::Vec2) -> Rect {
    let (ta, da) = (tex.x / tex.y.max(1.0), dst.x / dst.y.max(1.0));
    if ta > da {
        let w = da / ta;
        Rect::from_min_max(Pos2::new((1.0 - w) / 2.0, 0.0), Pos2::new((1.0 + w) / 2.0, 1.0))
    } else {
        let h = ta / da;
        Rect::from_min_max(Pos2::new(0.0, (1.0 - h) / 2.0), Pos2::new(1.0, (1.0 + h) / 2.0))
    }
}

fn capture(ui: &mut egui::Ui, st: &mut LibState) {
    let mut t = false;
    group(ui, "Finger-frame gesture");
    card(ui, |ui| {
        t |= switch_row(ui, "Gesture", "Frame a shot with both hands to take a screenshot", &mut st.gesture_enabled);
        divider(ui);
        ui.add_enabled_ui(st.gesture_enabled, |ui| {
            row(ui, "Hold delay", "Hold the frame this long before the viewfinder shows; then curl an index finger to shoot", SLIDER_W, |ui| {
                slider(ui, &mut st.gesture_hold_ms, 500.0..=4000.0, SLIDER_W, |v| format!("{:.1} s", v / 1000.0));
            });
            divider(ui);
            t |= switch_row(ui, "Viewfinder", "Show the frame in the headset · off arms silently", &mut st.gesture_feedback);
        });
    });
    group(ui, "New shots");
    card(ui, |ui| {
        t |= switch_row(ui, "Open straight away", "Skip the wrist card and open a photo window at once", &mut st.photo_skip_wrist);
        divider(ui);
        row(ui, "Crop margin", "Trim each edge of a new shot (hides stray fingers)", 200.0, |ui| {
            stepper(ui, &mut st.photo_crop_margin, 0.0, 25.0, 5.0, |v| format!("{v:.0} %"));
        });
        divider(ui);
        row(ui, "Clean up", "Delete shots older than this when the overlay starts", 220.0, |ui| {
            stepper_with(ui, &mut st.photo_cleanup_days, 0.0, 90.0, 5.0, (icon::MINUS, icon::PLUS), 116.0, |v| if v < 1.0 { "never".into() } else { format!("{v:.0} days") });
        });
    });
    if t {
        st.sound_tab = true;
    }
}

fn qr(ui: &mut egui::Ui, st: &mut LibState) {
    card(ui, |ui| {
        let mut t = switch_row(ui, "Detect QR codes", "A code in a shot shows its content on the wrist instead of the photo", &mut st.photo_qr_detect);
        divider(ui);
        ui.add_enabled_ui(st.photo_qr_detect, |ui| {
            t |= switch_row(ui, "Keep only the code", "Delete the screenshot once its code is read", &mut st.photo_qr_autodelete);
            divider(ui);
            t |= switch_row(ui, "Open straight away", "Links open on the desktop, text in a window", &mut st.photo_skip_wrist_qr);
        });
        if t {
            st.sound_tab = true;
        }
    });
}

fn sharing(ui: &mut egui::Ui, st: &mut LibState) {
    card(ui, |ui| {
        let status = |ok: bool| if ok { "Ready" } else { "Not set up (crates/overlay/*.env at build time)" };
        row(ui, "Translate", status(st.photo_translate_ok), 0.0, |_| {});
        divider(ui);
        row(ui, "Share (Picsur)", status(st.photo_share_ok), 0.0, |_| {});
        divider(ui);
        row(ui, "Folder", &st.photo_dir.clone(), 0.0, |_| {});
    });
    note(ui, icon::INFO, "Translate and Share show up on a photo window when they're set up");
}
