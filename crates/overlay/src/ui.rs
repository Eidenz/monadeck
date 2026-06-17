// The in-headset library UI. Art is lazy: each view records which tiles are
// on-screen (`visible_now`) and which game is selected; the render loop decodes
// only those. Drawing reads each game's `ArtState` (Ready / loading / Missing).
use egui_phosphor::regular as icon;

use crate::games::{ArtState, LibGame};
use crate::gfx::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Home,
    Library,
    Favorites,
    Tags,
    Settings,
}

/// All mutable UI state for the launcher panel.
pub struct LibState {
    pub games: Vec<LibGame>,
    pub scanning: bool,
    pub search: String,
    pub nav: Nav,
    pub selected: Option<usize>,
    /// Game indices whose tiles were on-screen this frame (drives lazy art).
    pub visible_now: Vec<usize>,
    /// Index of the currently-running game in `games`, if it's in the catalogue.
    pub running_index: Option<usize>,
    /// Index of the tile under the pointer this frame (drives hover haptics).
    pub hovered_index: Option<usize>,
    pub launch_request: Option<usize>,
    pub stop_request: Option<usize>,
    pub favorite_toggle_request: Option<usize>,
    pub recenter_request: bool,
    pub recenter_playspace_request: bool,
    pub keyboard_open: bool,
    /// Name of the game being launched (shows the "Launching…" overlay), set by
    /// the loop for ~1.5 s after Play before the dashboard auto-hides.
    pub launching_name: Option<String>,
    /// Summon fade-in amount (1 = fully dark, 0 = clear), set by the loop.
    pub fade_in: f32,
    /// One-shot UI-sound requests, drained by the loop.
    pub sound_select: bool,
    pub sound_tab: bool,
    /// Sound settings (mirrored to/from the persisted overlay config by the loop).
    pub audio_enabled: bool,
    pub audio_volume: f32,
}

impl LibState {
    pub fn new() -> Self {
        Self {
            games: Vec::new(),
            scanning: true,
            search: String::new(),
            nav: Nav::Home,
            selected: None,
            visible_now: Vec::new(),
            running_index: None,
            hovered_index: None,
            launch_request: None,
            stop_request: None,
            favorite_toggle_request: None,
            recenter_request: false,
            recenter_playspace_request: false,
            keyboard_open: false,
            launching_name: None,
            fade_in: 0.0,
            sound_select: false,
            sound_tab: false,
            audio_enabled: true,
            audio_volume: 0.55,
        }
    }
}

const TILE_W: f32 = 168.0;
const TILE_H: f32 = 252.0; // 2:3 portrait capsule.

pub fn build(ctx: &egui::Context, st: &mut LibState) {
    let searchable = !matches!(st.nav, Nav::Settings);
    left_rail(ctx, st);
    bottom_bar(ctx, st);
    if searchable && st.keyboard_open {
        keyboard(ctx, st);
    }
    if searchable {
        top_bar(ctx, st);
    }
    central(ctx, st);
    overlays(ctx, st);
}

/// Foreground overlays: the summon fade-in and the "Launching…" modal.
fn overlays(ctx: &egui::Context, st: &LibState) {
    let screen = ctx.screen_rect();
    if st.launching_name.is_some() || st.fade_in > 0.001 {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("overlay-dim")));
        let dim = if st.launching_name.is_some() { 180 } else { (st.fade_in * 255.0) as u8 };
        painter.rect_filled(screen, egui::CornerRadius::ZERO, egui::Color32::from_black_alpha(dim));
    }
    if let Some(name) = &st.launching_name {
        egui::Area::new(egui::Id::new("launching"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                hero_card().show(ui, |ui| {
                    ui.set_width(360.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        ui.add(egui::Spinner::new().size(34.0).color(theme::PRIMARY));
                        ui.add_space(12.0);
                        ui.label(egui::RichText::new("Launching").size(15.0).color(theme::ON_SURFACE_VAR));
                        ui.label(egui::RichText::new(name).size(22.0).strong());
                        ui.add_space(8.0);
                    });
                });
            });
    }
}

// --- chrome -----------------------------------------------------------------

fn left_rail(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(16, 19, 23))
        .inner_margin(egui::Margin::symmetric(10, 14));
    egui::SidePanel::left("nav")
        .exact_width(72.0)
        .resizable(false)
        .frame(frame)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(icon::CUBE).size(28.0).color(theme::PRIMARY));
                ui.add_space(14.0);
                for (glyph, nav) in [
                    (icon::HOUSE, Nav::Home),
                    (icon::SQUARES_FOUR, Nav::Library),
                    (icon::STAR, Nav::Favorites),
                    (icon::TAG, Nav::Tags),
                ] {
                    if rail_button(ui, glyph, st.nav == nav).clicked() && st.nav != nav {
                        st.nav = nav;
                        st.sound_tab = true;
                    }
                    ui.add_space(6.0);
                }
                // Recenter playspace + Settings, pinned to the bottom.
                let avail = ui.available_height();
                ui.add_space((avail - 102.0).max(0.0));
                if rail_button(ui, icon::CROSSHAIR, false)
                    .on_hover_text("Recenter playspace")
                    .clicked()
                {
                    st.recenter_playspace_request = true;
                    st.sound_tab = true;
                }
                ui.add_space(6.0);
                if rail_button(ui, icon::GEAR, st.nav == Nav::Settings).clicked() && st.nav != Nav::Settings {
                    st.nav = Nav::Settings;
                    st.sound_tab = true;
                }
            });
        });
}

fn rail_button(ui: &mut egui::Ui, glyph: &str, active: bool) -> egui::Response {
    let fg = if active { egui::Color32::BLACK } else { theme::ON_SURFACE_VAR };
    let fill = if active { theme::PRIMARY } else { egui::Color32::TRANSPARENT };
    let btn = egui::Button::new(egui::RichText::new(glyph).size(24.0).color(fg))
        .min_size(egui::vec2(48.0, 48.0))
        .fill(fill)
        .frame(true);
    ui.add(btn)
}

fn top_bar(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(13, 16, 20))
        .inner_margin(egui::Margin::symmetric(18, 12));
    egui::TopBottomPanel::top("search").exact_height(58.0).frame(frame).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            ui.label(egui::RichText::new(icon::MAGNIFYING_GLASS).size(20.0).color(theme::ON_SURFACE_VAR));
            ui.add_space(8.0);
            let kbd_w = 46.0;
            let resp = ui.add_sized(
                egui::vec2(ui.available_width() - kbd_w - 10.0, 30.0),
                egui::TextEdit::singleline(&mut st.search).hint_text("Search for games…").frame(false),
            );
            if resp.clicked() || resp.gained_focus() {
                st.keyboard_open = true;
            }
            ui.add_space(8.0);
            let kbd = egui::Button::new(egui::RichText::new(icon::KEYBOARD).size(20.0))
                .min_size(egui::vec2(kbd_w, 36.0))
                .fill(if st.keyboard_open { theme::PRIMARY } else { theme::SURFACE_CONTAINER_HIGH });
            if ui.add(kbd).clicked() {
                st.keyboard_open = !st.keyboard_open;
            }
        });
    });
}

fn bottom_bar(ctx: &egui::Context, st: &LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(13, 16, 20))
        .inner_margin(egui::Margin::symmetric(18, 8));
    egui::TopBottomPanel::bottom("status").exact_height(46.0).frame(frame).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            ui.label(egui::RichText::new(icon::GAME_CONTROLLER).size(22.0).color(theme::PRIMARY));
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Monadeck").strong());
            let count = if st.scanning {
                "scanning…".to_string()
            } else {
                format!("{} games", st.games.len())
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(count).color(theme::ON_SURFACE_VAR));
            });
        });
    });
}

// --- on-panel virtual keyboard ----------------------------------------------

fn keyboard(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(13, 16, 20))
        .inner_margin(egui::Margin::symmetric(14, 12));
    egui::TopBottomPanel::bottom("keyboard").frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        for row in ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"] {
            key_row(ui, row, &mut st.search);
        }
        let sp = 6.0;
        let total = 96.0 + 240.0 + 96.0 + 110.0 + 3.0 * sp;
        let pad = ((ui.available_width() - total) * 0.5).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.spacing_mut().item_spacing.x = sp;
            if fkey(ui, &format!("{}  Back", icon::BACKSPACE), 96.0, false).clicked() {
                st.search.pop();
            }
            if fkey(ui, "Space", 240.0, false).clicked() {
                st.search.push(' ');
            }
            if fkey(ui, "Clear", 96.0, false).clicked() {
                st.search.clear();
            }
            if fkey(ui, "Done", 110.0, true).clicked() {
                st.keyboard_open = false;
            }
        });
    });
}

fn key_row(ui: &mut egui::Ui, chars: &str, target: &mut String) {
    let (kw, sp) = (44.0, 6.0);
    let n = chars.chars().count() as f32;
    let total = n * kw + (n - 1.0).max(0.0) * sp;
    let pad = ((ui.available_width() - total) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(pad);
        ui.spacing_mut().item_spacing.x = sp;
        for ch in chars.chars() {
            let key = egui::Button::new(egui::RichText::new(ch.to_string()).size(18.0))
                .min_size(egui::vec2(kw, 44.0));
            if ui.add(key).clicked() {
                target.push(ch);
            }
        }
    });
}

fn fkey(ui: &mut egui::Ui, label: &str, w: f32, accent: bool) -> egui::Response {
    let text = egui::RichText::new(label).size(16.0);
    let text = if accent { text.color(egui::Color32::BLACK) } else { text };
    let mut btn = egui::Button::new(text).min_size(egui::vec2(w, 44.0));
    if accent {
        btn = btn.fill(theme::PRIMARY);
    }
    ui.add(btn)
}

// --- central views ----------------------------------------------------------

fn central(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default().fill(theme::SURFACE).inner_margin(egui::Margin::symmetric(24, 18));
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        if st.scanning {
            ui.add_space(80.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(28.0));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Scanning your libraries…").color(theme::ON_SURFACE_VAR));
            });
            return;
        }
        match st.nav {
            Nav::Home => home_view(ui, st),
            Nav::Library => grid_view(ui, st, "Library"),
            Nav::Favorites => favorites_view(ui, st),
            Nav::Tags => tags_view(ui, st),
            Nav::Settings => {
                st.visible_now.clear();
                st.hovered_index = None;
                settings_view(ui, st);
            }
        }
    });
}

fn home_view(ui: &mut egui::Ui, st: &mut LibState) {
    hero(ui, st);
    ui.add_space(14.0);
    ui.label(egui::RichText::new("Recent Games").heading().strong());
    ui.add_space(8.0);

    let shown = filtered(st);
    if shown.is_empty() {
        st.visible_now.clear();
        st.hovered_index = None;
        empty_note(ui, st);
        return;
    }
    let (mut visible, mut newly, mut hovered) = (Vec::new(), None, None);
    egui::ScrollArea::horizontal().id_salt("home-row").show(ui, |ui| {
        ui.horizontal(|ui| {
            for &i in &shown {
                let r = tile(ui, &st.games[i], st.selected == Some(i), st.running_index == Some(i));
                if ui.is_rect_visible(r.rect) {
                    visible.push(i);
                }
                if r.hovered() {
                    hovered = Some(i);
                }
                if r.clicked() {
                    newly = Some(i);
                }
                ui.add_space(14.0);
            }
        });
    });
    st.visible_now = visible;
    st.hovered_index = hovered;
    if newly.is_some() {
        st.selected = newly;
        st.sound_select = true;
    }
}

fn grid_view(ui: &mut egui::Ui, st: &mut LibState, title: &str) {
    ui.label(egui::RichText::new(title).heading().strong());
    ui.add_space(10.0);
    let shown = filtered(st);
    if shown.is_empty() {
        st.visible_now.clear();
        st.hovered_index = None;
        empty_note(ui, st);
        return;
    }
    game_grid(ui, st, &shown, "grid");
}

fn favorites_view(ui: &mut egui::Ui, st: &mut LibState) {
    ui.label(egui::RichText::new("Favorites").heading().strong());
    ui.add_space(10.0);
    let shown: Vec<usize> = filtered(st).into_iter().filter(|&i| st.games[i].is_favorite).collect();
    if shown.is_empty() {
        st.visible_now.clear();
        st.hovered_index = None;
        ui.add_space(50.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new(format!("{}  No favorites yet", icon::STAR)).size(18.0).color(theme::ON_SURFACE_VAR));
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Tap the ★ on a game to pin it here.").small().color(theme::ON_SURFACE_VAR));
        });
        return;
    }
    game_grid(ui, st, &shown, "favs");
}

/// A vertical wrapped grid of the given game indices, with hover/select/launch
/// tracking. Shared by Library + Favorites.
fn game_grid(ui: &mut egui::Ui, st: &mut LibState, shown: &[usize], salt: &str) {
    let (mut visible, mut newly, mut launch, mut hovered) = (Vec::new(), None, None, None);
    egui::ScrollArea::vertical().id_salt(salt).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for &i in shown {
                let r = tile(ui, &st.games[i], st.selected == Some(i), st.running_index == Some(i));
                if ui.is_rect_visible(r.rect) {
                    visible.push(i);
                }
                if r.hovered() {
                    hovered = Some(i);
                }
                if r.clicked() {
                    newly = Some(i);
                }
                if r.double_clicked() {
                    launch = Some(i);
                }
            }
        });
    });
    st.visible_now = visible;
    st.hovered_index = hovered;
    if newly.is_some() {
        st.selected = newly;
        st.sound_select = true;
    }
    if launch.is_some() {
        st.launch_request = launch;
    }
}

fn tags_view(ui: &mut egui::Ui, st: &mut LibState) {
    ui.label(egui::RichText::new("Categories").heading().strong());
    ui.add_space(8.0);
    let shown = filtered(st);
    if shown.is_empty() {
        st.visible_now.clear();
        st.hovered_index = None;
        empty_note(ui, st);
        return;
    }
    let groups: [(&str, fn(&LibGame) -> bool); 2] = [
        ("Steam", |g| g.source == "Steam"),
        ("Non-Steam", |g| g.source == "Non-Steam"),
    ];
    let (mut visible, mut newly, mut hovered) = (Vec::new(), None, None);
    egui::ScrollArea::vertical().id_salt("tags").show(ui, |ui| {
        for (label, pred) in groups {
            let group: Vec<usize> = shown.iter().copied().filter(|&i| pred(&st.games[i])).collect();
            if group.is_empty() {
                continue;
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!("{label}  ·  {}", group.len())).strong().color(theme::ON_SURFACE_VAR));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for &i in &group {
                    let r = tile(ui, &st.games[i], st.selected == Some(i), st.running_index == Some(i));
                    if ui.is_rect_visible(r.rect) {
                        visible.push(i);
                    }
                    if r.hovered() {
                        hovered = Some(i);
                    }
                    if r.clicked() {
                        newly = Some(i);
                    }
                }
            });
            ui.add_space(14.0);
        }
    });
    st.visible_now = visible;
    st.hovered_index = hovered;
    if newly.is_some() {
        st.selected = newly;
        st.sound_select = true;
    }
}

fn settings_view(ui: &mut egui::Ui, st: &mut LibState) {
    ui.label(egui::RichText::new("Overlay").heading().strong());
    ui.add_space(14.0);
    let recenter = egui::Button::new(
        egui::RichText::new(format!("{}  Recenter panel", icon::CROSSHAIR_SIMPLE)).size(17.0),
    )
    .min_size(egui::vec2(220.0, 42.0));
    if ui.add(recenter).clicked() {
        st.recenter_request = true;
        st.sound_tab = true;
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Brings the panel back in front of you. Grip to grab and move it.")
            .small()
            .color(theme::ON_SURFACE_VAR),
    );
    ui.add_space(20.0);
    ui.separator();
    ui.add_space(12.0);
    ui.label(egui::RichText::new(format!("{} games in your library", st.games.len())).color(theme::ON_SURFACE_VAR));

    ui.add_space(20.0);
    ui.separator();
    ui.add_space(12.0);
    ui.label(egui::RichText::new("Sound").strong());
    ui.add_space(8.0);
    ui.checkbox(&mut st.audio_enabled, "UI sounds (select, launch, tabs)");
    ui.add_space(6.0);
    ui.add_enabled_ui(st.audio_enabled, |ui| {
        ui.add(egui::Slider::new(&mut st.audio_volume, 0.0..=1.0).text("Volume").show_value(false));
    });
}

// --- hero banner ------------------------------------------------------------

enum HeroAction {
    None,
    Launch,
    Stop,
    ToggleFavorite,
}

const RUNNING_GREEN: egui::Color32 = egui::Color32::from_rgb(90, 220, 120);
const STOP_RED: egui::Color32 = egui::Color32::from_rgb(224, 78, 78);
const FAV_GOLD: egui::Color32 = egui::Color32::from_rgb(255, 200, 70);

fn hero(ui: &mut egui::Ui, st: &mut LibState) {
    let sel = st.selected.filter(|&i| i < st.games.len());
    let running = sel.is_some() && sel == st.running_index;
    let action = match sel {
        Some(i) => hero_banner(ui, &st.games[i], running),
        None => {
            hero_empty(ui);
            HeroAction::None
        }
    };
    match action {
        HeroAction::Launch => st.launch_request = sel,
        HeroAction::Stop => st.stop_request = sel,
        HeroAction::ToggleFavorite => st.favorite_toggle_request = sel,
        HeroAction::None => {}
    }
}

fn hero_card() -> egui::Frame {
    egui::Frame::default()
        .fill(egui::Color32::from_rgb(17, 21, 27))
        .corner_radius(14)
        .inner_margin(egui::Margin::same(16))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 44, 54)))
}

/// The single, consistent hero layout: a wide banner showing the game's hero art
/// when it's loaded, or a gradient placeholder (lazy-load swaps it in later) —
/// so the hero never switches shape or flickers between portrait/landscape.
fn hero_banner(ui: &mut egui::Ui, g: &LibGame, running: bool) -> HeroAction {
    let w = ui.available_width();
    let h = (w * 0.30).clamp(190.0, 300.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    match &g.hero {
        ArtState::Ready(tex) => draw_texture_cover(ui.painter(), rect, tex),
        _ => draw_hero_placeholder(ui.painter(), rect, &g.name),
    }

    let painter = ui.painter();
    let band = 18.0;
    for k in 0..7 {
        let y1 = rect.bottom() - k as f32 * band;
        let y0 = y1 - band;
        let alpha = ((7 - k) as f32 / 7.0 * 190.0) as u8;
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(rect.left(), y0), egui::pos2(rect.right(), y1)),
            egui::CornerRadius::ZERO,
            egui::Color32::from_black_alpha(alpha),
        );
    }
    // Logo art over the gradient, else the text title.
    if let ArtState::Ready(logo) = &g.logo {
        let [lw, lh] = logo.size();
        let aspect = lw as f32 / lh.max(1) as f32;
        let max_h = (h * 0.40).min(110.0);
        let max_w = w * 0.42;
        let mut dh = max_h;
        let mut dw = dh * aspect;
        if dw > max_w {
            dw = max_w;
            dh = dw / aspect;
        }
        let logo_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 22.0, rect.bottom() - 34.0 - dh),
            egui::vec2(dw, dh),
        );
        painter.image(
            logo.id(),
            logo_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        painter.text(
            egui::pos2(rect.left() + 24.0, rect.bottom() - 12.0),
            egui::Align2::LEFT_BOTTOM,
            sub_label(g),
            egui::FontId::proportional(13.0),
            theme::ON_SURFACE_VAR,
        );
    } else {
        painter.text(
            egui::pos2(rect.left() + 22.0, rect.bottom() - 54.0),
            egui::Align2::LEFT_BOTTOM,
            sub_label(g),
            egui::FontId::proportional(14.0),
            theme::ON_SURFACE_VAR,
        );
        painter.text(
            egui::pos2(rect.left() + 20.0, rect.bottom() - 20.0),
            egui::Align2::LEFT_BOTTOM,
            &g.name,
            egui::FontId::proportional(30.0),
            egui::Color32::WHITE,
        );
    }

    if running {
        let c = egui::pos2(rect.left() + 26.0, rect.top() + 26.0);
        painter.circle_filled(c, 5.0, RUNNING_GREEN);
        painter.text(
            egui::pos2(c.x + 12.0, c.y),
            egui::Align2::LEFT_CENTER,
            "Running",
            egui::FontId::proportional(14.0),
            RUNNING_GREEN,
        );
    }

    let play_rect = egui::Rect::from_min_size(
        egui::pos2(rect.right() - 16.0 - 150.0, rect.bottom() - 16.0 - 46.0),
        egui::vec2(150.0, 46.0),
    );
    let (label, fill, fg, action) = if running {
        (format!("{}  Stop", icon::STOP), STOP_RED, egui::Color32::WHITE, HeroAction::Stop)
    } else {
        (format!("{}  Play", icon::PLAY), theme::PRIMARY, egui::Color32::BLACK, HeroAction::Launch)
    };
    let btn = egui::Button::new(egui::RichText::new(label).size(19.0).color(fg)).fill(fill);
    let play_clicked = ui.put(play_rect, btn).clicked();

    let star_rect = egui::Rect::from_min_size(
        egui::pos2(play_rect.left() - 12.0 - 46.0, play_rect.top()),
        egui::vec2(46.0, 46.0),
    );
    let star_clicked = ui.put(star_rect, fav_button(g.is_favorite)).clicked();

    if star_clicked {
        HeroAction::ToggleFavorite
    } else if play_clicked {
        action
    } else {
        HeroAction::None
    }
}

/// The ★ favorite toggle: gold when pinned, muted otherwise.
fn fav_button(is_favorite: bool) -> egui::Button<'static> {
    let color = if is_favorite { FAV_GOLD } else { theme::ON_SURFACE_VAR };
    egui::Button::new(egui::RichText::new(icon::STAR).size(20.0).color(color))
        .fill(egui::Color32::from_black_alpha(120))
        .min_size(egui::vec2(46.0, 46.0))
}

/// A subtle vertical gradient placeholder for the hero banner when there's no
/// art (or it's still loading). Tinted from a hash of the name so each game gets
/// a consistent, distinct look instead of a flat block.
fn draw_hero_placeholder(painter: &egui::Painter, rect: egui::Rect, name: &str) {
    const PALETTE: [(egui::Color32, egui::Color32); 6] = [
        (egui::Color32::from_rgb(18, 22, 30), egui::Color32::from_rgb(33, 44, 62)),
        (egui::Color32::from_rgb(24, 19, 30), egui::Color32::from_rgb(46, 33, 58)),
        (egui::Color32::from_rgb(16, 28, 27), egui::Color32::from_rgb(26, 50, 46)),
        (egui::Color32::from_rgb(30, 23, 17), egui::Color32::from_rgb(54, 40, 28)),
        (egui::Color32::from_rgb(30, 18, 23), egui::Color32::from_rgb(56, 31, 42)),
        (egui::Color32::from_rgb(19, 25, 20), egui::Color32::from_rgb(33, 48, 35)),
    ];
    let (top, bottom) = PALETTE[name_hash(name) as usize % PALETTE.len()];
    const BANDS: usize = 28;
    for k in 0..BANDS {
        let t = (k as f32 + 0.5) / BANDS as f32;
        let y0 = rect.top() + (k as f32 / BANDS as f32) * rect.height();
        let y1 = rect.top() + ((k + 1) as f32 / BANDS as f32) * rect.height();
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(rect.left(), y0), egui::pos2(rect.right(), y1)),
            egui::CornerRadius::ZERO,
            lerp_color(top, bottom, t),
        );
    }
}

fn name_hash(s: &str) -> u32 {
    s.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
}

fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    egui::Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn hero_empty(ui: &mut egui::Ui) {
    hero_card().show(ui, |ui| {
        ui.set_min_height(248.0);
        ui.vertical_centered(|ui| {
            ui.add_space(96.0);
            ui.label(egui::RichText::new("Select a game to get started").size(18.0).color(theme::ON_SURFACE_VAR));
        });
    });
}

// --- helpers ----------------------------------------------------------------

fn filtered(st: &LibState) -> Vec<usize> {
    // Recency order only (games are already last-played-sorted). Favorites are
    // surfaced via the dedicated Favorites tab + the ★ tile badge, not by
    // reordering the main lists.
    let q = st.search.trim().to_lowercase();
    st.games
        .iter()
        .enumerate()
        .filter(|(_, g)| q.is_empty() || g.name.to_lowercase().contains(&q))
        .map(|(i, _)| i)
        .collect()
}

fn empty_note(ui: &mut egui::Ui, st: &LibState) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(if st.games.is_empty() {
                "No games found."
            } else {
                "No games match your search."
            })
            .color(theme::ON_SURFACE_VAR),
        );
    });
}

fn sub_label(g: &LibGame) -> String {
    let mut parts = vec![g.source.clone()];
    if let Some(m) = g.playtime_minutes {
        parts.push(human_playtime(m));
    }
    if let Some(a) = played_ago(g.last_played) {
        parts.push(a);
    }
    if let Some(sz) = g.size_on_disk {
        parts.push(human_size(sz));
    }
    parts.join("  ·  ")
}

fn human_playtime(minutes: u32) -> String {
    if minutes < 60 {
        format!("{minutes}m played")
    } else {
        format!("{:.1}h played", minutes as f32 / 60.0)
    }
}

fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.0} MB", b / 1e6)
    } else {
        format!("{:.0} KB", (b / 1e3).max(1.0))
    }
}

fn played_ago(ts: Option<u64>) -> Option<String> {
    let ts = ts.filter(|&t| t > 0)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    if now <= ts {
        return Some("just now".into());
    }
    let d = now - ts;
    let plural = |n: u64, unit: &str| format!("{n} {unit}{} ago", if n == 1 { "" } else { "s" });
    Some(if d < 3600 {
        "less than an hour ago".into()
    } else if d < 86_400 {
        plural(d / 3600, "hour")
    } else if d < 86_400 * 2 {
        "yesterday".into()
    } else if d < 86_400 * 30 {
        plural(d / 86_400, "day")
    } else if d < 86_400 * 365 {
        plural(d / (86_400 * 30), "month")
    } else {
        plural(d / (86_400 * 365), "year")
    })
}

fn tile(ui: &mut egui::Ui, game: &LibGame, selected: bool, running: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(TILE_W, TILE_H), egui::Sense::click());
    draw_art(ui.painter(), rect, &game.cover, &game.name);
    // Smoothly fade the hover highlight in/out.
    let hover_t = ui.ctx().animate_bool(resp.id, resp.hovered());
    if hover_t > 0.001 {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(8), egui::Color32::from_white_alpha((hover_t * 24.0) as u8));
    }
    if selected {
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(8),
            egui::Stroke::new(3.0, theme::PRIMARY),
            egui::StrokeKind::Inside,
        );
    }
    if running {
        let c = egui::pos2(rect.left() + 13.0, rect.top() + 13.0);
        ui.painter().circle_filled(c, 6.0, egui::Color32::from_black_alpha(140));
        ui.painter().circle_filled(c, 4.0, RUNNING_GREEN);
    }
    if game.is_favorite {
        let c = egui::pos2(rect.right() - 15.0, rect.top() + 15.0);
        ui.painter().circle_filled(c, 11.0, egui::Color32::from_black_alpha(120));
        ui.painter().text(c, egui::Align2::CENTER_CENTER, icon::STAR, egui::FontId::proportional(15.0), FAV_GOLD);
    }
    resp.on_hover_text(&game.name)
}

/// Draw a game's cover from its lazy art state: the texture (center-cropped) when
/// ready, a loading tint while pending, or a named placeholder if it has none.
fn draw_art(painter: &egui::Painter, rect: egui::Rect, art: &ArtState, name: &str) {
    let radius = egui::CornerRadius::same(8);
    match art {
        ArtState::Ready(tex) => draw_texture_cover(painter, rect, tex),
        ArtState::Missing => {
            painter.rect_filled(rect, radius, theme::SURFACE_CONTAINER_HIGH);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                short(name),
                egui::FontId::proportional(15.0),
                theme::ON_SURFACE_VAR,
            );
        }
        _ => {
            painter.rect_filled(rect, radius, theme::SURFACE_CONTAINER);
        }
    }
}

/// Paint a texture into `rect` with center-crop (object-fit: cover).
fn draw_texture_cover(painter: &egui::Painter, rect: egui::Rect, tex: &egui::TextureHandle) {
    let [tw, th] = tex.size();
    let img_aspect = tw as f32 / th.max(1) as f32;
    let tile_aspect = rect.width() / rect.height().max(1.0);
    let uv = if img_aspect > tile_aspect {
        let keep = tile_aspect / img_aspect;
        let x0 = (1.0 - keep) * 0.5;
        egui::Rect::from_min_max(egui::pos2(x0, 0.0), egui::pos2(x0 + keep, 1.0))
    } else {
        let keep = img_aspect / tile_aspect;
        let y0 = (1.0 - keep) * 0.5;
        egui::Rect::from_min_max(egui::pos2(0.0, y0), egui::pos2(1.0, y0 + keep))
    };
    painter.rect_filled(rect, egui::CornerRadius::same(8), egui::Color32::from_rgb(12, 14, 18));
    painter.image(tex.id(), rect, uv, egui::Color32::WHITE);
}

fn short(name: &str) -> String {
    if name.chars().count() > 28 {
        format!("{}…", name.chars().take(27).collect::<String>())
    } else {
        name.to_string()
    }
}
