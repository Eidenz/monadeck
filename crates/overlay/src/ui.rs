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
    Tools,
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
    /// Settings (mirrored to/from the persisted overlay config by the loop).
    pub audio_enabled: bool,
    pub audio_volume: f32,
    pub summon_tilt: bool,
    /// Timer state. `timer_secs` is the configured duration (adjusted when idle);
    /// `timer_remaining`/`timer_running`/`timer_paused` are set by the loop; the
    /// request flags are drained by it.
    pub timer_secs: u32,
    /// Duration the running countdown started from (drives the progress ring).
    pub timer_total: u32,
    pub timer_remaining: u32,
    pub timer_running: bool,
    pub timer_paused: bool,
    pub timer_toggle_request: bool,
    pub timer_reset_request: bool,
    /// Main panel is showing the active-game splash (toggled from the bottom bar).
    pub show_splash: bool,
    /// Device batteries + wall clock, refreshed by the loop for the bottom bar.
    pub batteries: Vec<crate::monado::BatteryInfo>,
    pub clock: String,
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
            summon_tilt: false,
            timer_secs: 300,
            timer_total: 300,
            timer_remaining: 300,
            timer_running: false,
            timer_paused: false,
            timer_toggle_request: false,
            timer_reset_request: false,
            show_splash: false,
            batteries: Vec::new(),
            clock: String::new(),
        }
    }
}

const TILE_W: f32 = 168.0;
const TILE_H: f32 = 252.0; // 2:3 portrait capsule.

/// The main (centre) panel: search bar, the active view (or active-game splash),
/// the on-screen keyboard, and the launching/fade overlays.
pub fn build_main(ctx: &egui::Context, st: &mut LibState) {
    let searchable = !st.show_splash && !matches!(st.nav, Nav::Settings | Nav::Tools);
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
                        ui.label(egui::RichText::new(name).size(22.0).strong().color(egui::Color32::WHITE));
                        ui.add_space(8.0);
                    });
                });
            });
    }
}

// --- chrome -----------------------------------------------------------------

/// The left floating nav rail (its own composition layer).
pub fn build_rail(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(18, 22, 28))
        .corner_radius(20)
        .inner_margin(egui::Margin::symmetric(10, 16));
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(10.0);
            for (glyph, nav) in [
                (icon::HOUSE, Nav::Home),
                (icon::SQUARES_FOUR, Nav::Library),
                (icon::STAR, Nav::Favorites),
                (icon::TAG, Nav::Tags),
                (icon::TIMER, Nav::Tools),
            ] {
                let active = st.nav == nav && !st.show_splash;
                if rail_button(ui, glyph, active).clicked() && !active {
                    st.nav = nav;
                    st.show_splash = false;
                    st.sound_tab = true;
                }
                ui.add_space(8.0);
            }
            // Settings pinned to the bottom.
            let avail = ui.available_height();
            ui.add_space((avail - 56.0).max(0.0));
            let active = st.nav == Nav::Settings && !st.show_splash;
            if rail_button(ui, icon::GEAR, active).clicked() && !active {
                st.nav = Nav::Settings;
                st.show_splash = false;
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

/// The bottom floating bar (its own layer): recenter · active-game splash toggle ·
/// device batteries · clock.
pub fn build_bottom(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(18, 22, 28))
        .corner_radius(20)
        .inner_margin(egui::Margin::symmetric(18, 6));
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            // Recenter playspace.
            let recenter = egui::Button::new(egui::RichText::new(icon::CROSSHAIR).size(22.0))
                .min_size(egui::vec2(46.0, 40.0))
                .fill(egui::Color32::TRANSPARENT);
            if ui.add(recenter).on_hover_text("Recenter playspace").clicked() {
                st.recenter_playspace_request = true;
                st.sound_tab = true;
            }
            ui.add_space(6.0);
            // Active-game splash toggle (only while a game runs).
            if let Some(i) = st.running_index {
                let name = short(&st.games[i].name);
                let btn = egui::Button::new(
                    egui::RichText::new(format!("{}  {}", icon::GAME_CONTROLLER, name))
                        .size(15.0)
                        .color(if st.show_splash { egui::Color32::BLACK } else { theme::ON_SURFACE }),
                )
                .fill(if st.show_splash { theme::PRIMARY } else { theme::SURFACE_CONTAINER_HIGH })
                .min_size(egui::vec2(0.0, 40.0));
                if ui.add(btn).on_hover_text("Active game").clicked() {
                    st.show_splash = !st.show_splash;
                    st.sound_tab = true;
                }
            }
            // Clock + batteries on the right.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !st.clock.is_empty() {
                    ui.label(egui::RichText::new(&st.clock).size(20.0).strong().color(egui::Color32::WHITE));
                }
                ui.add_space(16.0);
                for b in &st.batteries {
                    battery_widget(ui, b);
                    ui.add_space(10.0);
                }
            });
        });
    });
}

fn battery_widget(ui: &mut egui::Ui, b: &crate::monado::BatteryInfo) {
    use crate::monado::BatteryKind;
    let pct = (b.charge * 100.0).round() as i32;
    let bat = if b.charging {
        icon::BATTERY_CHARGING
    } else if b.charge > 0.66 {
        icon::BATTERY_FULL
    } else if b.charge > 0.33 {
        icon::BATTERY_MEDIUM
    } else if b.charge > 0.1 {
        icon::BATTERY_LOW
    } else {
        icon::BATTERY_WARNING
    };
    let color = if b.charge > 0.33 {
        RUNNING_GREEN
    } else if b.charge > 0.15 {
        FAV_GOLD
    } else {
        STOP_RED
    };
    let dev = match b.kind {
        BatteryKind::Glove => icon::HAND,
        BatteryKind::Controller => icon::GAME_CONTROLLER,
        _ => icon::CIRCLE,
    };
    ui.label(
        egui::RichText::new(format!("{dev} {bat} {pct}%"))
            .size(14.0)
            .color(color),
    )
    .on_hover_text(match b.kind {
        BatteryKind::Glove => "Glove",
        BatteryKind::Controller => "Controller",
        BatteryKind::Tracker => "Tracker",
        BatteryKind::Other => "Device",
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
        if st.show_splash {
            splash_view(ui, st);
            return;
        }
        match st.nav {
            Nav::Home => home_view(ui, st),
            Nav::Library => grid_view(ui, st, "Library"),
            Nav::Favorites => favorites_view(ui, st),
            Nav::Tags => tags_view(ui, st),
            Nav::Tools => {
                st.visible_now.clear();
                st.hovered_index = None;
                tools_view(ui, st);
            }
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
    ui.label(egui::RichText::new("Recent Games").heading().strong().color(egui::Color32::WHITE));
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
    ui.label(egui::RichText::new(title).heading().strong().color(egui::Color32::WHITE));
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
    ui.label(egui::RichText::new("Favorites").heading().strong().color(egui::Color32::WHITE));
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
    ui.label(egui::RichText::new("Categories").heading().strong().color(egui::Color32::WHITE));
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

/// Full-screen splash for the currently-running game: cover, title, and Stop.
fn splash_view(ui: &mut egui::Ui, st: &mut LibState) {
    st.hovered_index = None;
    let Some(i) = st.running_index.filter(|&i| i < st.games.len()) else {
        st.visible_now.clear();
        ui.add_space(80.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("No game is running.").size(18.0).color(theme::ON_SURFACE_VAR));
        });
        return;
    };
    // Keep the running game's cover loaded while the splash is shown.
    st.visible_now = vec![i];
    let g = &st.games[i];

    // Hero art (dimmed) or a gradient as a full-bleed background.
    let full = ui.max_rect();
    match &g.hero {
        ArtState::Ready(tex) => {
            draw_texture_cover(ui.painter(), full, tex);
            ui.painter().rect_filled(full, egui::CornerRadius::ZERO, egui::Color32::from_black_alpha(175));
        }
        _ => {
            draw_hero_placeholder(ui.painter(), full, &g.name);
            ui.painter().rect_filled(full, egui::CornerRadius::ZERO, egui::Color32::from_black_alpha(90));
        }
    }

    ui.add_space(40.0);
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(300.0, 450.0), egui::Sense::hover());
        draw_art(ui.painter(), rect, &g.cover, &g.name);
        ui.add_space(48.0);
        ui.vertical(|ui| {
            ui.add_space(30.0);
            ui.label(egui::RichText::new(&g.name).size(42.0).strong().color(egui::Color32::WHITE));
            ui.add_space(10.0);
            ui.label(egui::RichText::new(format!("●  Running")).size(17.0).color(RUNNING_GREEN).strong());
            ui.add_space(10.0);
            ui.label(egui::RichText::new(sub_label(g)).size(16.0).color(theme::ON_SURFACE));
            ui.add_space(36.0);
            let stop = egui::Button::new(
                egui::RichText::new(format!("{}  Stop", icon::STOP)).size(24.0).color(egui::Color32::WHITE),
            )
            .fill(STOP_RED)
            .min_size(egui::vec2(280.0, 66.0));
            if ui.add(stop).clicked() {
                st.stop_request = Some(i);
            }
        });
    });
}

/// A countdown timer that fires a toast + chime when it reaches zero. Centred on
/// a circular progress ring with the time inside it.
fn tools_view(ui: &mut egui::Ui, st: &mut LibState) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon::TIMER).size(28.0).color(theme::PRIMARY));
        ui.add_space(10.0);
        ui.label(egui::RichText::new("Timer").size(28.0).strong().color(egui::Color32::WHITE));
    });
    ui.add_space(6.0);

    ui.vertical_centered(|ui| {
        // Progress ring with the remaining time inside it.
        let dim = 248.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(dim, dim), egui::Sense::hover());
        let frac = if st.timer_running || st.timer_paused {
            st.timer_remaining as f32 / st.timer_total.max(1) as f32
        } else {
            1.0
        };
        let accent = if st.timer_running {
            theme::PRIMARY
        } else if st.timer_paused {
            FAV_GOLD
        } else {
            egui::Color32::from_rgb(60, 78, 80) // muted teal: armed, not running
        };
        timer_ring(ui.painter(), rect, frac.clamp(0.0, 1.0), accent);

        // The ring is fixed-size; the time shrinks (and gains an hours field) so
        // long durations still fit inside it.
        let secs = st.timer_remaining;
        let label = if secs >= 3600 {
            format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
        } else {
            format!("{:02}:{:02}", secs / 60, secs % 60)
        };
        let fs = match label.chars().count() {
            0..=5 => 60.0,
            6 => 50.0,
            7 => 42.0,
            _ => 34.0,
        };
        ui.painter().text(
            rect.center() - egui::vec2(0.0, 8.0),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(fs),
            egui::Color32::WHITE,
        );
        let status = if st.timer_running {
            "running"
        } else if st.timer_paused {
            "paused"
        } else {
            "ready"
        };
        ui.painter().text(
            rect.center() + egui::vec2(0.0, 42.0),
            egui::Align2::CENTER_CENTER,
            status,
            egui::FontId::proportional(15.0),
            theme::ON_SURFACE_VAR,
        );

        ui.add_space(22.0);

        if !st.timer_running && !st.timer_paused {
            centered_row(ui, 310.0, |ui| {
                if pill(ui, "−1m", 70.0, false).clicked() {
                    st.timer_secs = st.timer_secs.saturating_sub(60);
                }
                if pill(ui, "−10s", 70.0, false).clicked() {
                    st.timer_secs = st.timer_secs.saturating_sub(10);
                }
                if pill(ui, "+10s", 70.0, false).clicked() {
                    st.timer_secs = (st.timer_secs + 10).min(86_400);
                }
                if pill(ui, "+1m", 70.0, false).clicked() {
                    st.timer_secs = (st.timer_secs + 60).min(86_400);
                }
            });
            ui.add_space(10.0);
            centered_row(ui, 278.0, |ui| {
                for m in [1u32, 5, 10, 30] {
                    let sel = st.timer_secs == m * 60;
                    if pill(ui, &format!("{m}m"), 62.0, sel).clicked() {
                        st.timer_secs = m * 60;
                    }
                }
            });
            ui.add_space(20.0);
            centered_row(ui, 220.0, |ui| {
                let start = egui::Button::new(
                    egui::RichText::new(format!("{}  Start", icon::PLAY)).size(20.0).color(egui::Color32::BLACK),
                )
                .fill(theme::PRIMARY)
                .corner_radius(12)
                .min_size(egui::vec2(220.0, 54.0));
                if st.timer_secs > 0 && ui.add(start).clicked() {
                    st.timer_toggle_request = true;
                    st.sound_tab = true;
                }
            });
        } else {
            centered_row(ui, 350.0, |ui| {
                let (label, glyph) = if st.timer_running {
                    ("Pause", icon::PAUSE)
                } else {
                    ("Resume", icon::PLAY)
                };
                let toggle = egui::Button::new(
                    egui::RichText::new(format!("{glyph}  {label}")).size(19.0).color(egui::Color32::BLACK),
                )
                .fill(theme::PRIMARY)
                .corner_radius(12)
                .min_size(egui::vec2(170.0, 52.0));
                if ui.add(toggle).clicked() {
                    st.timer_toggle_request = true;
                    st.sound_tab = true;
                }
                let reset = egui::Button::new(
                    egui::RichText::new(format!("{}  Reset", icon::ARROW_COUNTER_CLOCKWISE)).size(19.0).color(theme::ON_SURFACE),
                )
                .fill(theme::SURFACE_CONTAINER_HIGH)
                .corner_radius(12)
                .min_size(egui::vec2(170.0, 52.0));
                if ui.add(reset).clicked() {
                    st.timer_reset_request = true;
                    st.sound_tab = true;
                }
            });
        }
    });
}

/// Lay out `content` as a horizontal row centred within the available width.
fn centered_row(ui: &mut egui::Ui, total_w: f32, content: impl FnOnce(&mut egui::Ui)) {
    let pad = ((ui.available_width() - total_w) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(pad);
        ui.spacing_mut().item_spacing.x = 10.0;
        content(ui);
    });
}

/// A circular track with a progress arc sweeping clockwise from 12 o'clock,
/// rounded at both ends.
fn timer_ring(painter: &egui::Painter, rect: egui::Rect, frac: f32, accent: egui::Color32) {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.5 - 14.0;
    let width = 15.0;
    painter.circle_stroke(center, radius, egui::Stroke::new(width, egui::Color32::from_rgb(34, 40, 48)));
    if frac <= 0.0 {
        return;
    }
    let start = -FRAC_PI_2;
    let sweep = frac * TAU;
    let n = 96;
    let pts: Vec<egui::Pos2> = (0..=n)
        .map(|i| {
            let a = start + sweep * (i as f32 / n as f32);
            egui::pos2(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect();
    painter.add(egui::Shape::line(pts, egui::Stroke::new(width, accent)));
    // Rounded caps at both ends.
    let cap = |a: f32| egui::pos2(center.x + radius * a.cos(), center.y + radius * a.sin());
    painter.circle_filled(cap(start), width * 0.5, accent);
    painter.circle_filled(cap(start + sweep), width * 0.5, accent);
}

fn pill(ui: &mut egui::Ui, label: &str, w: f32, selected: bool) -> egui::Response {
    let (fg, fill) = if selected {
        (egui::Color32::BLACK, theme::PRIMARY)
    } else {
        (theme::ON_SURFACE, theme::SURFACE_CONTAINER_HIGH)
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(16.0).color(fg))
            .fill(fill)
            .corner_radius(10)
            .min_size(egui::vec2(w, 42.0)),
    )
}

/// Per-notification icon + accent colour.
#[derive(Clone, Copy)]
#[allow(dead_code)] // `Info` is the generic fallback for future toasts.
pub enum ToastKind {
    Timer,
    Battery,
    Info,
}

impl ToastKind {
    fn style(self) -> (&'static str, egui::Color32) {
        match self {
            ToastKind::Timer => (icon::TIMER, theme::PRIMARY),
            ToastKind::Battery => (icon::BATTERY_WARNING, FAV_GOLD),
            ToastKind::Info => (icon::BELL_RINGING, theme::PRIMARY),
        }
    }
}

/// The floating notification card (its own layer; shows over a game too). The
/// quad is cleared transparent, so the card hugs its content and floats centred.
pub fn build_toast(ctx: &egui::Context, title: &str, body: &str, kind: ToastKind) {
    let (glyph, accent) = kind.style();
    let card = egui::Frame::default()
        .fill(egui::Color32::from_rgb(24, 28, 35))
        .corner_radius(20)
        .inner_margin(egui::Margin::symmetric(20, 16))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(46, 54, 64)));
    egui::Area::new(egui::Id::new("toast-card"))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            card.show(ui, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // Tinted icon chip.
                    let (chip, _) = ui.allocate_exact_size(egui::vec2(52.0, 52.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        chip,
                        egui::CornerRadius::same(14),
                        egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 38),
                    );
                    ui.painter().text(
                        chip.center(),
                        egui::Align2::CENTER_CENTER,
                        glyph,
                        egui::FontId::proportional(27.0),
                        accent,
                    );
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(title).size(21.0).strong().color(egui::Color32::WHITE));
                        if !body.is_empty() {
                            ui.add_space(3.0);
                            ui.label(egui::RichText::new(body).size(15.0).color(theme::ON_SURFACE_VAR));
                        }
                    });
                });
            });
        });
}

fn settings_view(ui: &mut egui::Ui, st: &mut LibState) {
    ui.label(egui::RichText::new("Settings").size(30.0).strong().color(egui::Color32::WHITE));
    ui.add_space(18.0);

    ui.label(egui::RichText::new("Panel").size(19.0).strong().color(theme::ON_SURFACE));
    ui.add_space(8.0);
    let recenter = egui::Button::new(
        egui::RichText::new(format!("{}  Recenter panel", icon::CROSSHAIR_SIMPLE)).size(17.0).color(theme::ON_SURFACE),
    )
    .min_size(egui::vec2(240.0, 46.0));
    if ui.add(recenter).clicked() {
        st.recenter_request = true;
        st.sound_tab = true;
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Brings the panel back in front of you. Grip to grab and move it.")
            .size(14.0)
            .color(theme::ON_SURFACE_VAR),
    );
    ui.add_space(10.0);
    ui.checkbox(
        &mut st.summon_tilt,
        egui::RichText::new("Tilt to match headset angle on summon").size(16.0).color(theme::ON_SURFACE),
    );

    ui.add_space(24.0);
    ui.label(egui::RichText::new("Sound").size(19.0).strong().color(theme::ON_SURFACE));
    ui.add_space(8.0);
    ui.checkbox(
        &mut st.audio_enabled,
        egui::RichText::new("UI sounds (select, launch, tabs)").size(16.0).color(theme::ON_SURFACE),
    );
    ui.add_space(8.0);
    ui.add_enabled_ui(st.audio_enabled, |ui| {
        ui.add(
            egui::Slider::new(&mut st.audio_volume, 0.0..=1.0)
                .text(egui::RichText::new("Volume").size(15.0).color(theme::ON_SURFACE))
                .show_value(false),
        );
    });

    ui.add_space(24.0);
    ui.label(
        egui::RichText::new(format!("{} games in your library", st.games.len()))
            .size(15.0)
            .color(theme::ON_SURFACE_VAR),
    );
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
