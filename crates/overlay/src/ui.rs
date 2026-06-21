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
    Playspace,
    Monado,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Recent,
    Name,
    Playtime,
    Size,
}

/// All mutable UI state for the launcher panel.
pub struct LibState {
    pub games: Vec<LibGame>,
    pub scanning: bool,
    pub search: String,
    pub nav: Nav,
    /// Sort order for the Library / Favorites / Categories lists (not Home).
    pub sort: SortMode,
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
    /// Toggle the selected game's UEVR ("VR Mod") flag.
    pub uevr_toggle_request: Option<usize>,
    /// User collections (names, mirrored from the loop) + drained op requests.
    pub collections: Vec<String>,
    pub collection_toggle: Option<usize>, // toggle the selected game in collection #
    pub collection_create: Option<String>, // create a new collection with this name
    pub collection_delete: Option<usize>, // delete collection #
    /// Naming a new collection: the keyboard targets `name_buf` instead of search.
    pub naming: bool,
    pub name_buf: String,
    pub recenter_request: bool,
    pub recenter_playspace_request: bool,
    /// Re-scan the catalogue + re-probe artwork (picks up covers added at runtime).
    pub refresh_request: bool,
    pub keyboard_open: bool,
    /// Name of the game being launched (shows the "Launching…" overlay), set by
    /// the loop for ~1.5 s after Play before the dashboard auto-hides.
    pub launching_name: Option<String>,
    /// Header text for the launching card — "Launching" by default, swapped to
    /// "Waiting for UEVR injection…" while a VR-Mod game injects. Set by the loop.
    pub launching_status: Option<String>,
    /// Summon fade-in amount (1 = fully dark, 0 = clear), set by the loop.
    pub fade_in: f32,
    /// One-shot UI-sound requests, drained by the loop.
    pub sound_select: bool,
    pub sound_tab: bool,
    /// Settings (mirrored to/from the persisted overlay config by the loop).
    pub audio_enabled: bool,
    pub audio_volume: f32,
    /// Seconds chihuahua waits before injecting a UEVR game (Settings slider).
    pub uevr_delay: u32,
    /// Whether protontricks-launch is installed — the UEVR UI is hidden if not.
    pub uevr_available: bool,
    pub summon_tilt: bool,
    /// Panel placement comfort knobs (mirrored to/from the overlay config).
    pub panel_dist: f32,
    pub panel_scale: f32,
    pub panel_curve: f32,
    /// Playspace offset (OVRAS-style): metres + yaw in degrees, with the chosen
    /// nudge steps. Mirrored to/from the overlay config; applied via libmonado.
    pub playspace_x: f32,
    pub playspace_y: f32,
    pub playspace_z: f32,
    pub playspace_yaw: f32,
    pub playspace_step: f32,     // metres per nudge
    pub playspace_yaw_step: f32, // degrees per nudge
    /// Per-game playspace override editing. The steppers edit the running game's
    /// override (`ps_game_*`) when `ps_target_game` is set and a game is running,
    /// otherwise the global offset above. Maintained + persisted by the loop.
    pub ps_target_game: bool, // editor target: false = Global, true = running game
    pub ps_game_active: bool, // a game is running (override target available)
    pub ps_game_name: String, // running game's name (for the target switch label)
    pub ps_game_override: bool, // the running game currently has a saved override
    pub ps_game_x: f32,
    pub ps_game_y: f32,
    pub ps_game_z: f32,
    pub ps_game_yaw: f32,
    pub ps_game_save_request: bool,  // persist ps_game_* for the running game
    pub ps_game_clear_request: bool, // drop the running game's override
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
    /// Monado page: running app clients (set by the loop) + drained per-row
    /// action requests (freeze toggle / set-active by id, kill by app name).
    pub monado_clients: Vec<crate::monado::ClientInfo>,
    /// Whether the active runtime's libmonado supports controller freezing (our
    /// fork). Hides the per-row freeze button when false.
    pub monado_freeze_supported: bool,
    pub freeze_toggle_request: Option<u32>,
    pub set_active_request: Option<u32>,
    pub kill_request: Option<String>,
    /// Seconds to count down before a freeze applies (Settings; mirrored from the
    /// overlay config). 0 = freeze immediately.
    pub freeze_delay_secs: f32,
    /// A freeze counting down: (client id, seconds remaining). Set by the loop,
    /// drives the button's countdown label.
    pub freeze_pending: Option<(u32, f32)>,
    /// Minutes the currently-running game has been up this session (for the splash).
    pub session_minutes: Option<u32>,
    /// Central-view fade-in animation (resets when the tab / splash changes).
    last_nav: Nav,
    last_splash: bool,
    view_anim: f32,
}

impl LibState {
    pub fn new() -> Self {
        Self {
            games: Vec::new(),
            scanning: true,
            search: String::new(),
            nav: Nav::Home,
            sort: SortMode::Recent,
            selected: None,
            visible_now: Vec::new(),
            running_index: None,
            hovered_index: None,
            launch_request: None,
            stop_request: None,
            favorite_toggle_request: None,
            uevr_toggle_request: None,
            collections: Vec::new(),
            collection_toggle: None,
            collection_create: None,
            collection_delete: None,
            naming: false,
            name_buf: String::new(),
            recenter_request: false,
            recenter_playspace_request: false,
            refresh_request: false,
            keyboard_open: false,
            launching_name: None,
            launching_status: None,
            fade_in: 0.0,
            sound_select: false,
            sound_tab: false,
            audio_enabled: true,
            audio_volume: 0.55,
            uevr_delay: 30,
            uevr_available: false,
            summon_tilt: false,
            panel_dist: 1.5,
            panel_scale: 1.0,
            panel_curve: 1.0,
            playspace_x: 0.0,
            playspace_y: 0.0,
            playspace_z: 0.0,
            playspace_yaw: 0.0,
            playspace_step: 0.05,
            playspace_yaw_step: 15.0,
            ps_target_game: false,
            ps_game_active: false,
            ps_game_name: String::new(),
            ps_game_override: false,
            ps_game_x: 0.0,
            ps_game_y: 0.0,
            ps_game_z: 0.0,
            ps_game_yaw: 0.0,
            ps_game_save_request: false,
            ps_game_clear_request: false,
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
            monado_clients: Vec::new(),
            monado_freeze_supported: false,
            freeze_toggle_request: None,
            set_active_request: None,
            kill_request: None,
            freeze_delay_secs: 3.0,
            freeze_pending: None,
            session_minutes: None,
            last_nav: Nav::Home,
            last_splash: false,
            view_anim: 1.0,
        }
    }
}

const TILE_W: f32 = 168.0;
const TILE_H: f32 = 252.0; // 2:3 portrait capsule.

/// The main (centre) panel: search bar, the active view (or active-game splash),
/// the on-screen keyboard, and the launching/fade overlays.
pub fn build_main(ctx: &egui::Context, st: &mut LibState) {
    let searchable = !st.show_splash && !matches!(st.nav, Nav::Settings | Nav::Tools | Nav::Playspace);
    if (searchable || st.naming) && st.keyboard_open {
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
                        ui.label(
                            egui::RichText::new(st.launching_status.as_deref().unwrap_or("Launching"))
                                .size(15.0)
                                .color(theme::ON_SURFACE_VAR),
                        );
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
            ] {
                let active = st.nav == nav && !st.show_splash;
                if rail_button(ui, glyph, active).clicked() && !active {
                    st.nav = nav;
                    st.show_splash = false;
                    st.sound_tab = true;
                }
                ui.add_space(8.0);
            }
            // Tools (timer) · Playspace · Freeze · Settings pinned to the bottom.
            // Reserve enough for the icons PLUS item-spacing + the rounded-corner
            // margin, or the last icon overruns the panel's rounded bottom (clipped).
            // ~64 px per icon (48 button + 8 add_space + spacing) — bump when adding.
            let avail = ui.available_height();
            ui.add_space((avail - 272.0).max(0.0));
            let bottom = [
                (icon::TIMER, Nav::Tools),
                (icon::ARROWS_OUT_CARDINAL, Nav::Playspace),
                (icon::STACK, Nav::Monado),
                (icon::GEAR, Nav::Settings),
            ];
            for (k, &(glyph, nav)) in bottom.iter().enumerate() {
                let active = st.nav == nav && !st.show_splash;
                if rail_button(ui, glyph, active).clicked() && !active {
                    st.nav = nav;
                    st.show_splash = false;
                    st.sound_tab = true;
                }
                if k + 1 < bottom.len() {
                    ui.add_space(8.0);
                }
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
            // Recenter playspace — transparent at rest, fades to a hover highlight
            // with the icon brightening to white.
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(46.0, 40.0), egui::Sense::click());
            let t = ui.ctx().animate_bool(resp.id, resp.hovered());
            if t > 0.001 {
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(10),
                    egui::Color32::from_rgba_unmultiplied(48, 70, 74, (t * 255.0) as u8),
                );
            }
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                icon::CROSSHAIR,
                egui::FontId::proportional(22.0),
                lerp_color(theme::ON_SURFACE_VAR, egui::Color32::WHITE, t),
            );
            if resp.on_hover_text("Recenter playspace").clicked() {
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
    let naming = st.naming;
    egui::TopBottomPanel::bottom("keyboard").frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        if naming {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{}  New collection:", icon::FOLDER_PLUS)).size(14.0).color(theme::ON_SURFACE_VAR));
                ui.add_space(6.0);
                let shown = if st.name_buf.is_empty() { "…" } else { st.name_buf.as_str() };
                ui.label(egui::RichText::new(shown).size(16.0).strong().color(egui::Color32::WHITE));
            });
            ui.add_space(4.0);
        }
        for row in ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"] {
            key_row(ui, row, if naming { &mut st.name_buf } else { &mut st.search });
        }
        let sp = 6.0;
        let total = 96.0 + 240.0 + 96.0 + 130.0 + 3.0 * sp;
        let pad = ((ui.available_width() - total) * 0.5).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.spacing_mut().item_spacing.x = sp;
            if fkey(ui, &format!("{}  Back", icon::BACKSPACE), 96.0, false).clicked() {
                if naming { st.name_buf.pop(); } else { st.search.pop(); }
            }
            if fkey(ui, "Space", 240.0, false).clicked() {
                if naming { st.name_buf.push(' '); } else { st.search.push(' '); }
            }
            if fkey(ui, "Clear", 96.0, false).clicked() {
                if naming { st.name_buf.clear(); } else { st.search.clear(); }
            }
            let commit = if naming { "Create" } else { "Done" };
            if fkey(ui, commit, 130.0, true).clicked() {
                if naming {
                    let name = st.name_buf.trim().to_string();
                    if !name.is_empty() {
                        st.collection_create = Some(name);
                    }
                    st.name_buf.clear();
                    st.naming = false;
                }
                st.keyboard_open = false;
            }
        });
        if naming {
            let cancel_pad = ((ui.available_width() - 110.0) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(cancel_pad);
                if fkey(ui, "Cancel", 110.0, false).clicked() {
                    st.name_buf.clear();
                    st.naming = false;
                    st.keyboard_open = false;
                }
            });
        }
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
        // Quick fade-in when the view changes (tab switch / splash toggle).
        if st.nav != st.last_nav || st.show_splash != st.last_splash {
            st.last_nav = st.nav;
            st.last_splash = st.show_splash;
            st.view_anim = 0.0;
        }
        st.view_anim = (st.view_anim + 0.14).min(1.0);
        ui.set_opacity(st.view_anim);
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
            Nav::Playspace => {
                st.visible_now.clear();
                st.hovered_index = None;
                playspace_view(ui, st);
            }
            Nav::Monado => {
                st.visible_now.clear();
                st.hovered_index = None;
                monado_view(ui, st);
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
    collection_chips(ui, st);
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

/// Membership chips for the selected game: tap to add/remove it from a collection,
/// or "＋ New" to create one. Shown under the Home hero.
fn collection_chips(ui: &mut egui::Ui, st: &mut LibState) {
    let Some(sel) = st.selected.filter(|&i| i < st.games.len()) else {
        return;
    };
    ui.add_space(8.0);
    let cols = st.collections.clone();
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(format!("{}  Collections", icon::FOLDERS)).size(13.0).color(theme::ON_SURFACE_VAR));
        ui.add_space(4.0);
        for (ci, name) in cols.iter().enumerate() {
            let member = st.games[sel].collections.contains(&ci);
            if chip(ui, name, member).clicked() {
                st.collection_toggle = Some(ci);
                st.sound_tab = true;
            }
        }
        if chip(ui, &format!("{}  New", icon::PLUS), false).clicked() {
            st.naming = true;
            st.name_buf.clear();
            st.keyboard_open = true;
        }
    });
}

fn chip(ui: &mut egui::Ui, label: &str, on: bool) -> egui::Response {
    let (fg, fill) = if on {
        (egui::Color32::BLACK, theme::PRIMARY)
    } else {
        (theme::ON_SURFACE, theme::SURFACE_CONTAINER_HIGH)
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(13.0).color(fg))
            .fill(fill)
            .corner_radius(8)
            .min_size(egui::vec2(0.0, 28.0)),
    )
}

fn grid_view(ui: &mut egui::Ui, st: &mut LibState, title: &str) {
    view_header(ui, st, title);
    ui.add_space(10.0);
    let mut shown = filtered(st);
    apply_sort(st, &mut shown);
    if shown.is_empty() {
        st.visible_now.clear();
        st.hovered_index = None;
        empty_note(ui, st);
        return;
    }
    game_grid(ui, st, &shown, "grid");
}

/// A view header: the title on the left, the sort selector on the right.
fn view_header(ui: &mut egui::Ui, st: &mut LibState, title: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).heading().strong().color(egui::Color32::WHITE));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Added right-to-left, so list reversed to read Recent · Name · … left-to-right.
            for (label, mode) in [
                (icon::HARD_DRIVES, SortMode::Size),
                (icon::HOURGLASS_MEDIUM, SortMode::Playtime),
                (icon::TEXT_AA, SortMode::Name),
                (icon::CLOCK_COUNTER_CLOCKWISE, SortMode::Recent),
            ] {
                if sort_pill(ui, label, sort_label(mode), st.sort == mode).clicked() {
                    st.sort = mode;
                    st.sound_tab = true;
                }
            }
            ui.label(egui::RichText::new("Sort").size(13.0).color(theme::ON_SURFACE_VAR));
        });
    });
}

fn sort_label(mode: SortMode) -> &'static str {
    match mode {
        SortMode::Recent => "Recent",
        SortMode::Name => "Name",
        SortMode::Playtime => "Played",
        SortMode::Size => "Size",
    }
}

fn sort_pill(ui: &mut egui::Ui, glyph: &str, label: &str, selected: bool) -> egui::Response {
    let (fg, fill) = if selected {
        (egui::Color32::BLACK, theme::PRIMARY)
    } else {
        (theme::ON_SURFACE_VAR, theme::SURFACE_CONTAINER)
    };
    ui.add(
        egui::Button::new(egui::RichText::new(format!("{glyph}  {label}")).size(13.0).color(fg))
            .fill(fill)
            .corner_radius(8)
            .min_size(egui::vec2(0.0, 30.0)),
    )
}

/// Sort game indices in place by the active mode (Recent keeps the recency order
/// the catalogue already arrives in).
fn apply_sort(st: &LibState, idxs: &mut [usize]) {
    let key_play = |g: &LibGame| g.playtime_minutes.or(g.tracked_minutes).unwrap_or(0);
    match st.sort {
        SortMode::Recent => {}
        SortMode::Name => idxs.sort_by(|&a, &b| {
            st.games[a].name.to_lowercase().cmp(&st.games[b].name.to_lowercase())
        }),
        SortMode::Playtime => {
            idxs.sort_by(|&a, &b| key_play(&st.games[b]).cmp(&key_play(&st.games[a])))
        }
        SortMode::Size => idxs.sort_by(|&a, &b| {
            st.games[b].size_on_disk.unwrap_or(0).cmp(&st.games[a].size_on_disk.unwrap_or(0))
        }),
    }
}

fn favorites_view(ui: &mut egui::Ui, st: &mut LibState) {
    view_header(ui, st, "Favorites");
    ui.add_space(10.0);
    let mut shown: Vec<usize> = filtered(st).into_iter().filter(|&i| st.games[i].is_favorite).collect();
    apply_sort(st, &mut shown);
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
    view_header(ui, st, "Categories");
    ui.add_space(8.0);

    // Create a new collection (works even with no games yet).
    if chip(ui, &format!("{}  New collection", icon::FOLDER_PLUS), false).clicked() {
        st.naming = true;
        st.name_buf.clear();
        st.keyboard_open = true;
    }
    ui.add_space(10.0);

    let shown = filtered(st);
    if shown.is_empty() {
        st.visible_now.clear();
        st.hovered_index = None;
        empty_note(ui, st);
        return;
    }
    let cols = st.collections.clone();
    let groups: [(&str, fn(&LibGame) -> bool); 2] = [
        ("Steam", |g| g.source == "Steam"),
        ("Non-Steam", |g| g.source == "Non-Steam"),
    ];
    let (mut visible, mut newly, mut hovered, mut delete) = (Vec::new(), None, None, None);
    egui::ScrollArea::vertical().id_salt("tags").show(ui, |ui| {
        // User collections first.
        for (ci, name) in cols.iter().enumerate() {
            let mut group: Vec<usize> =
                shown.iter().copied().filter(|&i| st.games[i].collections.contains(&ci)).collect();
            apply_sort(st, &mut group);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{}  {name}  ·  {}", icon::FOLDER, group.len()))
                        .strong()
                        .color(theme::ON_SURFACE_VAR),
                );
                ui.add_space(6.0);
                let del = egui::Button::new(egui::RichText::new(icon::TRASH).size(13.0).color(STOP_RED))
                    .fill(egui::Color32::TRANSPARENT)
                    .min_size(egui::vec2(28.0, 24.0));
                if ui.add(del).on_hover_text("Delete collection").clicked() {
                    delete = Some(ci);
                }
            });
            ui.add_space(6.0);
            if group.is_empty() {
                ui.label(egui::RichText::new("Empty — add games from the Home hero.").size(13.0).color(theme::ON_SURFACE_VAR));
            } else {
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
            }
            ui.add_space(14.0);
        }
        // Auto categories by source.
        for (label, pred) in groups {
            let mut group: Vec<usize> = shown.iter().copied().filter(|&i| pred(&st.games[i])).collect();
            apply_sort(st, &mut group);
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
    if let Some(ci) = delete {
        st.collection_delete = Some(ci);
        st.sound_tab = true;
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
            let running_line = match st.session_minutes {
                Some(m) if m > 0 => {
                    let t = if m < 60 { format!("{m}m") } else { format!("{:.1}h", m as f32 / 60.0) };
                    format!("●  Running  ·  {t} this session")
                }
                _ => "●  Running".to_string(),
            };
            ui.label(egui::RichText::new(running_line).size(17.0).color(RUNNING_GREEN).strong());
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
    page_header(ui, icon::TIMER, "Timer");

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
    });

    ui.add_space(24.0);

    // Controls grouped in a centred card. The card lives in a vertical_centered so
    // its content inherits a VERTICAL layout (a Frame inside a `horizontal` would
    // lay the rows out side-by-side and run off the panel).
    let card_w = 360.0;
    ui.vertical_centered(|ui| {
        egui::Frame::default()
            .fill(theme::SURFACE_CONTAINER)
            .corner_radius(16)
            .inner_margin(egui::Margin::symmetric(16, 16))
            .show(ui, |ui| {
                ui.set_width(card_w);
                ui.vertical(|ui| {
                    if !st.timer_running && !st.timer_paused {
                        centered_row(ui, 336.0, |ui| {
                            if pill(ui, "−1m", 78.0, false).clicked() {
                                st.timer_secs = st.timer_secs.saturating_sub(60);
                            }
                            if pill(ui, "−10s", 78.0, false).clicked() {
                                st.timer_secs = st.timer_secs.saturating_sub(10);
                            }
                            if pill(ui, "+10s", 78.0, false).clicked() {
                                st.timer_secs = (st.timer_secs + 10).min(86_400);
                            }
                            if pill(ui, "+1m", 78.0, false).clicked() {
                                st.timer_secs = (st.timer_secs + 60).min(86_400);
                            }
                        });
                        ui.add_space(10.0);
                        centered_row(ui, 286.0, |ui| {
                            for m in [1u32, 5, 10, 30] {
                                let sel = st.timer_secs == m * 60;
                                if pill(ui, &format!("{m}m"), 64.0, sel).clicked() {
                                    st.timer_secs = m * 60;
                                }
                            }
                        });
                        ui.add_space(16.0);
                        centered_row(ui, card_w, |ui| {
                            let start = egui::Button::new(
                                egui::RichText::new(format!("{}  Start", icon::PLAY)).size(20.0).color(egui::Color32::BLACK),
                            )
                            .fill(theme::PRIMARY)
                            .corner_radius(12)
                            .min_size(egui::vec2(card_w, 52.0));
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
            });
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

// --- modern settings widgets (SteamVR-style: grouped cards, label-left/control-
// right rows, segmented toggles, sliders with a value chip) ------------------

/// A page heading with an accent icon (e.g. ⚙ Settings).
fn page_header(ui: &mut egui::Ui, glyph: &str, title: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(glyph).size(28.0).color(theme::PRIMARY));
        ui.add_space(12.0);
        ui.label(egui::RichText::new(title).size(28.0).strong().color(egui::Color32::WHITE));
    });
    ui.add_space(16.0);
}

/// A titled group of rows inside a rounded card.
fn section(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    if !title.is_empty() {
        ui.label(egui::RichText::new(title).size(14.0).strong().color(theme::ON_SURFACE_VAR));
        ui.add_space(7.0);
    }
    egui::Frame::default()
        .fill(theme::SURFACE_CONTAINER)
        .corner_radius(16)
        .inner_margin(egui::Margin::symmetric(18, 8))
        .show(ui, |ui| {
            contents(ui);
        });
    ui.add_space(18.0);
}

/// One settings row: a label (+ optional sub-line) on the left, a control on the
/// right. The classic SteamVR layout.
fn setting_row(ui: &mut egui::Ui, label: &str, sub: Option<&str>, control: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        ui.vertical(|ui| {
            ui.add_space(9.0);
            ui.label(egui::RichText::new(label).size(16.0).color(theme::ON_SURFACE));
            if let Some(s) = sub {
                ui.add_space(2.0);
                ui.label(egui::RichText::new(s).size(12.0).color(theme::ON_SURFACE_VAR));
            }
            ui.add_space(9.0);
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}

/// A faint full-width separator between rows in a card.
fn divider(ui: &mut egui::Ui) {
    ui.add_space(2.0);
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 16)),
    );
    ui.add_space(2.0);
}

/// A two-segment Off / On switch. Returns true if the value changed.
fn seg_toggle(ui: &mut egui::Ui, value: &mut bool) -> bool {
    let before = *value;
    egui::Frame::default()
        .fill(egui::Color32::from_rgb(22, 26, 32))
        .corner_radius(11)
        .inner_margin(egui::Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                if seg_btn(ui, "Off", !*value, false).clicked() {
                    *value = false;
                }
                if seg_btn(ui, "On", *value, true).clicked() {
                    *value = true;
                }
            });
        });
    *value != before
}

fn seg_btn(ui: &mut egui::Ui, label: &str, selected: bool, accent: bool) -> egui::Response {
    let (fg, fill) = if selected {
        if accent {
            (egui::Color32::BLACK, theme::PRIMARY)
        } else {
            (egui::Color32::WHITE, theme::SURFACE_CONTAINER_HIGH)
        }
    } else {
        (theme::ON_SURFACE_VAR, egui::Color32::TRANSPARENT)
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(14.0).color(fg))
            .fill(fill)
            .corner_radius(8)
            .min_size(egui::vec2(58.0, 32.0)),
    )
}

/// A SteamVR-style horizontal slider: rounded track, teal fill, round thumb, with
/// the value shown in a chip on the right. `width` spans the track + chip. A laser
/// click on the track jumps to that value; drag fine-tunes. Returns true if changed.
fn modern_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    width: f32,
    fmt: impl Fn(f32) -> String,
) -> bool {
    let (lo, hi) = (*range.start(), *range.end());
    let h = 36.0;
    let chip_w = 74.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, h), egui::Sense::click_and_drag());
    let track_left = rect.left() + 10.0;
    let track_right = rect.right() - chip_w - 12.0;
    let cy = rect.center().y;
    let th = 4.0;

    let mut changed = false;
    if (resp.dragged() || resp.clicked()) && (track_right > track_left) {
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

    let painter = ui.painter();
    painter.rect_filled(
        egui::Rect::from_min_max(egui::pos2(track_left, cy - th), egui::pos2(track_right, cy + th)),
        th,
        egui::Color32::from_rgb(26, 31, 38),
    );
    painter.rect_filled(
        egui::Rect::from_min_max(egui::pos2(track_left, cy - th), egui::pos2(tx, cy + th)),
        th,
        theme::PRIMARY,
    );
    painter.circle_filled(egui::pos2(tx, cy), 10.0, theme::PRIMARY);
    painter.circle_filled(egui::pos2(tx, cy), 5.0, egui::Color32::WHITE);
    let chip = egui::Rect::from_min_size(egui::pos2(rect.right() - chip_w, cy - 14.0), egui::vec2(chip_w, 28.0));
    painter.rect_filled(chip, 8.0, theme::SURFACE_CONTAINER_HIGH);
    painter.text(
        chip.center(),
        egui::Align2::CENTER_CENTER,
        fmt(*value),
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
    );
    changed
}

/// An inline −/value/+ stepper (right side of a row) for values a slider suits
/// poorly (e.g. panel size, whose slider would sit on the panel it resizes).
fn stepper_inline(
    ui: &mut egui::Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    step: f32,
    fmt: impl Fn(f32) -> String,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if step_btn(ui, icon::MINUS).clicked() {
            *value = (*value - step).max(min);
        }
        ui.add_sized(
            egui::vec2(70.0, 34.0),
            egui::Label::new(egui::RichText::new(fmt(*value)).size(16.0).strong().color(egui::Color32::WHITE)),
        );
        if step_btn(ui, icon::PLUS).clicked() {
            *value = (*value + step).min(max);
        }
    });
}

/// A neutral pill button for a row's right-hand action (Recenter, Refresh, …).
fn action_button(ui: &mut egui::Ui, glyph: &str, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(format!("{glyph}  {label}")).size(15.0).color(theme::ON_SURFACE),
        )
        .fill(theme::SURFACE_CONTAINER_HIGH)
        .corner_radius(10)
        .min_size(egui::vec2(150.0, 42.0)),
    )
}

/// An outlined "reset" button (SteamVR's RESET PAGE TO DEFAULT vibe).
fn reset_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(format!("{}  {label}", icon::ARROW_COUNTER_CLOCKWISE))
                .size(15.0)
                .color(theme::ON_SURFACE_VAR),
        )
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(1.0, theme::SURFACE_CONTAINER_HIGH))
        .corner_radius(10)
        .min_size(egui::vec2(200.0, 42.0)),
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

/// OVRAS-style playspace page: nudge the floor / play area on each axis (and
/// rotate) with selectable steps. Applied live via libmonado and persisted.
fn playspace_view(ui: &mut egui::Ui, st: &mut LibState) {
    page_header(ui, icon::ARROWS_OUT_CARDINAL, "Playspace");
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.label(
            egui::RichText::new("Nudge where your floor and play area sit.")
                .size(14.0)
                .color(theme::ON_SURFACE_VAR),
        );
        ui.add_space(14.0);

        let editing_game = st.ps_target_game && st.ps_game_active;

        // Per-game vs global target — only while a game runs (a per-game offset
        // only applies, and is only felt, during play).
        if st.ps_game_active {
            section(ui, "Apply to", |ui| {
                setting_row(ui, "Target", None, |ui| {
                    let label = if st.ps_game_name.chars().count() > 14 {
                        format!("{}…", st.ps_game_name.chars().take(14).collect::<String>())
                    } else {
                        st.ps_game_name.clone()
                    };
                    if pill(ui, &label, 150.0, st.ps_target_game).clicked() {
                        st.ps_target_game = true;
                        st.sound_tab = true;
                    }
                    if pill(ui, "Global", 88.0, !st.ps_target_game).clicked() {
                        st.ps_target_game = false;
                        st.sound_tab = true;
                    }
                });
            });
        }

        // Step sizes.
        section(ui, "Step size", |ui| {
            setting_row(ui, "Move", None, |ui| {
                ui.horizontal(|ui| {
                    for m in [0.01_f32, 0.05, 0.10] {
                        let sel = (st.playspace_step - m).abs() < 0.001;
                        if pill(ui, &format!("{} cm", (m * 100.0).round() as i32), 66.0, sel).clicked() {
                            st.playspace_step = m;
                            st.sound_tab = true;
                        }
                    }
                });
            });
            divider(ui);
            setting_row(ui, "Rotate", None, |ui| {
                ui.horizontal(|ui| {
                    for d in [5.0_f32, 15.0, 45.0] {
                        let sel = (st.playspace_yaw_step - d).abs() < 0.1;
                        if pill(ui, &format!("{}°", d as i32), 56.0, sel).clicked() {
                            st.playspace_yaw_step = d;
                            st.sound_tab = true;
                        }
                    }
                });
            });
        });

        let step = st.playspace_step;
        let ystep = st.playspace_yaw_step;
        let mut bumped = false;
        section(ui, "Offset", |ui| {
            if editing_game {
                let t = fmt_m(st.ps_game_y);
                bumped |= ps_row(ui, "Height", &mut st.ps_game_y, step, &t, icon::MINUS, icon::PLUS);
                divider(ui);
                let t = fmt_m(st.ps_game_z);
                bumped |= ps_row(ui, "Forward / back", &mut st.ps_game_z, step, &t, icon::MINUS, icon::PLUS);
                divider(ui);
                let t = fmt_m(st.ps_game_x);
                bumped |= ps_row(ui, "Left / right", &mut st.ps_game_x, step, &t, icon::MINUS, icon::PLUS);
                divider(ui);
                let t = fmt_deg(st.ps_game_yaw);
                bumped |= ps_row(ui, "Rotate", &mut st.ps_game_yaw, ystep, &t, icon::ARROW_COUNTER_CLOCKWISE, icon::ARROW_CLOCKWISE);
                if bumped {
                    st.ps_game_x = st.ps_game_x.clamp(-3.0, 3.0);
                    st.ps_game_y = st.ps_game_y.clamp(-2.0, 2.0);
                    st.ps_game_z = st.ps_game_z.clamp(-3.0, 3.0);
                    st.ps_game_yaw = wrap_deg(st.ps_game_yaw);
                    st.ps_game_override = true;
                    st.ps_game_save_request = true;
                    st.sound_tab = true;
                }
            } else {
                let t = fmt_m(st.playspace_y);
                bumped |= ps_row(ui, "Height", &mut st.playspace_y, step, &t, icon::MINUS, icon::PLUS);
                divider(ui);
                let t = fmt_m(st.playspace_z);
                bumped |= ps_row(ui, "Forward / back", &mut st.playspace_z, step, &t, icon::MINUS, icon::PLUS);
                divider(ui);
                let t = fmt_m(st.playspace_x);
                bumped |= ps_row(ui, "Left / right", &mut st.playspace_x, step, &t, icon::MINUS, icon::PLUS);
                divider(ui);
                let t = fmt_deg(st.playspace_yaw);
                bumped |= ps_row(ui, "Rotate", &mut st.playspace_yaw, ystep, &t, icon::ARROW_COUNTER_CLOCKWISE, icon::ARROW_CLOCKWISE);
                if bumped {
                    st.playspace_x = st.playspace_x.clamp(-3.0, 3.0);
                    st.playspace_y = st.playspace_y.clamp(-2.0, 2.0);
                    st.playspace_z = st.playspace_z.clamp(-3.0, 3.0);
                    st.playspace_yaw = wrap_deg(st.playspace_yaw);
                    st.sound_tab = true;
                }
            }
        });

        ui.horizontal(|ui| {
            let reset_label = if editing_game { "Use global" } else { "Reset" };
            if reset_button(ui, reset_label).clicked() {
                if editing_game {
                    st.ps_game_x = 0.0;
                    st.ps_game_y = 0.0;
                    st.ps_game_z = 0.0;
                    st.ps_game_yaw = 0.0;
                    st.ps_game_override = false;
                    st.ps_game_clear_request = true;
                } else {
                    st.playspace_x = 0.0;
                    st.playspace_y = 0.0;
                    st.playspace_z = 0.0;
                    st.playspace_yaw = 0.0;
                }
                st.sound_tab = true;
            }
            ui.add_space(10.0);
            let rec = egui::Button::new(
                egui::RichText::new(format!("{}  Recenter", icon::CROSSHAIR_SIMPLE)).size(15.0).color(egui::Color32::BLACK),
            )
            .fill(theme::PRIMARY)
            .corner_radius(10)
            .min_size(egui::vec2(170.0, 42.0));
            if ui.add(rec).on_hover_text("Recenter to your current head pose").clicked() {
                st.recenter_playspace_request = true;
                st.sound_tab = true;
            }
        });
        ui.add_space(10.0);
        let note = if editing_game {
            "Overrides your global offset only while this game runs. Persists across restarts."
        } else {
            "Offsets persist and re-apply when the runtime restarts."
        };
        ui.label(egui::RichText::new(note).size(13.0).color(theme::ON_SURFACE_VAR));
    });
}

/// Wrap an angle (degrees) into (-180, 180].
fn wrap_deg(mut v: f32) -> f32 {
    if v > 180.0 {
        v -= 360.0;
    } else if v < -180.0 {
        v += 360.0;
    }
    v
}

fn ps_row(ui: &mut egui::Ui, label: &str, value: &mut f32, step: f32, value_text: &str, minus: &str, plus: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        ui.add_sized(
            egui::vec2(60.0, 44.0),
            egui::Label::new(egui::RichText::new(label).size(16.0).color(theme::ON_SURFACE)),
        );
        // Stepper right-aligned: in a right-to-left layout, add +, value, − so they
        // read −  value  + left-to-right.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ps_btn(ui, plus).clicked() {
                *value += step;
                changed = true;
            }
            ui.add_sized(
                egui::vec2(104.0, 44.0),
                egui::Label::new(egui::RichText::new(value_text).size(18.0).strong().color(egui::Color32::WHITE)),
            );
            if ps_btn(ui, minus).clicked() {
                *value -= step;
                changed = true;
            }
        });
    });
    changed
}

fn ps_btn(ui: &mut egui::Ui, glyph: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(glyph).size(19.0).color(theme::ON_SURFACE))
            .fill(theme::SURFACE_CONTAINER_HIGH)
            .corner_radius(10)
            .min_size(egui::vec2(66.0, 44.0)),
    )
}

fn fmt_m(v: f32) -> String {
    if v.abs() < 0.005 {
        "0.00 m".to_string()
    } else {
        format!("{v:+.2} m")
    }
}

fn fmt_deg(v: f32) -> String {
    if v.abs() < 0.5 {
        "0°".to_string()
    } else {
        format!("{v:+.0}°")
    }
}

/// A labelled +/- stepper (for knobs that resize the panel the control sits on,
/// where a slider's grab point would jump out from under you).
fn step_btn(ui: &mut egui::Ui, glyph: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(glyph).size(15.0).color(theme::ON_SURFACE))
            .fill(theme::SURFACE_CONTAINER_HIGH)
            .corner_radius(8)
            .min_size(egui::vec2(42.0, 30.0)),
    )
}


/// Monado page: Monado runs several VR apps at once. Per running app, switch which
/// one your headset displays ("Set active" = primary), freeze its hands in place, or
/// close it. Overlays (monadeck itself, WayVR) are excluded.
fn monado_view(ui: &mut egui::Ui, st: &mut LibState) {
    page_header(ui, icon::STACK, "Monado");
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.label(
            egui::RichText::new(
                "Manage your running Monado apps — set which one is active, freeze \
                 controllers, or kill apps.",
            )
            .size(14.0)
            .color(theme::ON_SURFACE_VAR),
        );
        ui.add_space(14.0);

        let clients = st.monado_clients.clone();
        if clients.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(icon::STACK).size(40.0).color(theme::ON_SURFACE_VAR));
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("No running apps.")
                        .size(15.0)
                        .color(theme::ON_SURFACE_VAR),
                );
            });
            return;
        }

        section(ui, "Running apps", |ui| {
            let n = clients.len();
            for (i, c) in clients.iter().enumerate() {
                let label = if c.name.chars().count() > 14 {
                    format!("{}…", c.name.chars().take(14).collect::<String>())
                } else {
                    c.name.clone()
                };
                let sub = if !c.is_app { Some("backgrounded") } else { None };
                // Laid out right-to-left, so add Kill, then Freeze, then Set active
                // to read [Set active] [Freeze] [Kill] left-to-right.
                setting_row(ui, &label, sub, |ui| {
                    let kill = egui::Button::new(
                        egui::RichText::new(format!("{}  Kill", icon::X)).size(16.0).color(egui::Color32::WHITE),
                    )
                    .fill(egui::Color32::from_rgb(176, 64, 64))
                    .corner_radius(10)
                    .min_size(egui::vec2(78.0, 42.0));
                    if ui.add(kill).clicked() {
                        st.kill_request = Some(c.name.clone());
                        st.sound_tab = true;
                    }

                    // Freeze only shows on a fork that supports it (stock Monado lacks
                    // the symbol, so the button would no-op). A freeze counts down
                    // first (Settings) so you can settle into position; tapping during
                    // the countdown cancels it.
                    if st.monado_freeze_supported {
                        let pending_secs = match st.freeze_pending {
                            Some((pid, secs)) if pid == c.id => Some(secs),
                            _ => None,
                        };
                        let txt = if c.frozen {
                            "Unfreeze controllers".to_string()
                        } else if let Some(secs) = pending_secs {
                            format!("Cancel ({}s)", secs.ceil() as u32)
                        } else {
                            "Freeze controllers".to_string()
                        };
                        let selected = c.frozen || pending_secs.is_some();
                        if pill(ui, &txt, 192.0, selected).clicked() {
                            st.freeze_toggle_request = Some(c.id);
                            st.sound_tab = true;
                        }
                    }

                    if c.is_primary {
                        let _ = pill(ui, &format!("{}  Active", icon::MONITOR), 104.0, true);
                    } else if pill(ui, "Set active", 104.0, false).clicked() {
                        st.set_active_request = Some(c.id);
                        st.sound_tab = true;
                    }
                });
                if i + 1 < n {
                    divider(ui);
                }
            }
        });
    });
}

fn settings_view(ui: &mut egui::Ui, st: &mut LibState) {
    page_header(ui, icon::GEAR, "Settings");
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        section(ui, "Panel", |ui| {
            setting_row(ui, "Recenter panel", Some("Bring it back in front of you · grip to grab & move"), |ui| {
                if action_button(ui, icon::CROSSHAIR_SIMPLE, "Recenter").clicked() {
                    st.recenter_request = true;
                    st.sound_tab = true;
                }
            });
            divider(ui);
            let mut t = false;
            setting_row(ui, "Tilt on summon", Some("Match the panel to your headset's pitch"), |ui| {
                t = seg_toggle(ui, &mut st.summon_tilt);
            });
            if t {
                st.sound_tab = true;
            }
            divider(ui);
            setting_row(ui, "Distance", None, |ui| {
                modern_slider(ui, &mut st.panel_dist, 0.8..=2.5, 360.0, |v| format!("{v:.2} m"));
            });
            divider(ui);
            // Size is a stepper, not a slider: the slider would sit on the panel it
            // resizes, so dragging makes the grab point jump as it grows under you.
            setting_row(ui, "Size", None, |ui| {
                stepper_inline(ui, &mut st.panel_scale, 0.7, 1.4, 0.05, |v| format!("{:.0}%", v * 100.0));
            });
            divider(ui);
            setting_row(ui, "Curve", None, |ui| {
                modern_slider(ui, &mut st.panel_curve, 1.0..=3.0, 360.0, |v| format!("{v:.2}"));
            });
        });
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if reset_button(ui, "Reset placement").clicked() {
                    st.panel_dist = 1.5;
                    st.panel_scale = 1.0;
                    st.panel_curve = 1.0;
                    st.sound_tab = true;
                }
            });
        });
        ui.add_space(18.0);

        section(ui, "Sound", |ui| {
            let mut t = false;
            setting_row(ui, "UI sounds", Some("Select, launch and tab clicks"), |ui| {
                t = seg_toggle(ui, &mut st.audio_enabled);
            });
            if t {
                st.sound_tab = true;
            }
            divider(ui);
            let enabled = st.audio_enabled;
            setting_row(ui, "Volume", None, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    modern_slider(ui, &mut st.audio_volume, 0.0..=1.0, 360.0, |v| format!("{:.0}%", v * 100.0));
                });
            });
        });

        if st.uevr_available {
            section(ui, "VR Mod (UEVR)", |ui| {
                setting_row(
                    ui,
                    "Injection delay",
                    Some("Wait after launch before injecting · raise for slow games"),
                    |ui| {
                        let mut d = st.uevr_delay as f32;
                        if modern_slider(ui, &mut d, 0.0..=120.0, 360.0, |v| format!("{v:.0} s")) {
                            st.uevr_delay = d.round() as u32;
                        }
                    },
                );
            });
        }

        section(ui, "Controllers", |ui| {
            setting_row(
                ui,
                "Freeze delay",
                Some("Countdown before a freeze applies · time to get into position"),
                |ui| {
                    modern_slider(ui, &mut st.freeze_delay_secs, 0.0..=10.0, 360.0, |v| format!("{v:.0} s"));
                },
            );
        });

        let n = st.games.len();
        section(ui, "Library", |ui| {
            setting_row(ui, "Refresh library", Some(&format!("{n} games · re-scan to pick up new artwork")), |ui| {
                if action_button(ui, icon::ARROW_CLOCKWISE, "Refresh").clicked() {
                    st.refresh_request = true;
                    st.sound_tab = true;
                }
            });
        });
    });
}

// --- hero banner ------------------------------------------------------------

enum HeroAction {
    None,
    Launch,
    Stop,
    ToggleFavorite,
    ToggleUevr,
}

const RUNNING_GREEN: egui::Color32 = egui::Color32::from_rgb(90, 220, 120);
const STOP_RED: egui::Color32 = egui::Color32::from_rgb(224, 78, 78);
const FAV_GOLD: egui::Color32 = egui::Color32::from_rgb(255, 200, 70);

fn hero(ui: &mut egui::Ui, st: &mut LibState) {
    let sel = st.selected.filter(|&i| i < st.games.len());
    let running = sel.is_some() && sel == st.running_index;
    let action = match sel {
        Some(i) => hero_banner(ui, &st.games[i], running, st.uevr_available),
        None => {
            hero_empty(ui);
            HeroAction::None
        }
    };
    match action {
        HeroAction::Launch => st.launch_request = sel,
        HeroAction::Stop => st.stop_request = sel,
        HeroAction::ToggleFavorite => st.favorite_toggle_request = sel,
        HeroAction::ToggleUevr => st.uevr_toggle_request = sel,
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
fn hero_banner(ui: &mut egui::Ui, g: &LibGame, running: bool, uevr_available: bool) -> HeroAction {
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

    // VR-Mod (UEVR) toggle, left of the star — only for non-Steam games we can
    // inject (v1 scope: they carry the launch exe via shortcuts.vdf).
    // Offer the VR-Mod toggle only for Unreal Engine games we can inject: non-Steam
    // UE shortcuts and Proton UE Steam games (detected by their install layout).
    let can_uevr = uevr_available && g.uevr_capable;
    let uevr_clicked = can_uevr && {
        let uevr_rect = egui::Rect::from_min_size(
            egui::pos2(star_rect.left() - 12.0 - 104.0, star_rect.top()),
            egui::vec2(104.0, 46.0),
        );
        ui.put(uevr_rect, uevr_button(g.uevr)).clicked()
    };

    if uevr_clicked {
        HeroAction::ToggleUevr
    } else if star_clicked {
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

/// The VR-Mod (UEVR) toggle: headset glyph + "UEVR" label, teal when enabled.
fn uevr_button(enabled: bool) -> egui::Button<'static> {
    let color = if enabled { theme::PRIMARY } else { theme::ON_SURFACE_VAR };
    egui::Button::new(egui::RichText::new(format!("{}  UEVR", icon::VIRTUAL_REALITY)).size(16.0).color(color))
        .fill(egui::Color32::from_black_alpha(120))
        .min_size(egui::vec2(104.0, 46.0))
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
    let (glyph, title, hint) = if st.games.is_empty() {
        (icon::GAME_CONTROLLER, "No games found", "Add games to Steam or your non-Steam shortcuts.")
    } else {
        (icon::MAGNIFYING_GLASS, "No matches", "Try a different search.")
    };
    ui.add_space(56.0);
    ui.vertical_centered(|ui| {
        ui.label(egui::RichText::new(glyph).size(40.0).color(theme::SURFACE_CONTAINER_HIGH));
        ui.add_space(10.0);
        ui.label(egui::RichText::new(title).size(19.0).strong().color(theme::ON_SURFACE));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(hint).size(14.0).color(theme::ON_SURFACE_VAR));
    });
}

fn sub_label(g: &LibGame) -> String {
    let mut parts = vec![g.source.clone()];
    // Steam's own playtime when known, else our tracked total (non-Steam games).
    if let Some(m) = g.playtime_minutes.or(g.tracked_minutes) {
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
    // Smoothly fade the hover highlight + pop the cover slightly on hover.
    let hover_t = ui.ctx().animate_bool(resp.id, resp.hovered());
    let r = rect.expand(hover_t * 5.0);
    if hover_t > 0.001 {
        // Soft shadow behind the lifted tile.
        ui.painter().rect_filled(
            r.translate(egui::vec2(0.0, 2.0)).expand(2.0),
            egui::CornerRadius::same(10),
            egui::Color32::from_black_alpha((hover_t * 70.0) as u8),
        );
    }
    draw_art(ui.painter(), r, &game.cover, &game.name);
    if hover_t > 0.001 {
        ui.painter().rect_filled(r, egui::CornerRadius::same(8), egui::Color32::from_white_alpha((hover_t * 24.0) as u8));
    }
    if selected {
        ui.painter().rect_stroke(
            r,
            egui::CornerRadius::same(8),
            egui::Stroke::new(3.0, theme::PRIMARY),
            egui::StrokeKind::Inside,
        );
    }
    if running {
        let c = egui::pos2(r.left() + 13.0, r.top() + 13.0);
        ui.painter().circle_filled(c, 6.0, egui::Color32::from_black_alpha(140));
        ui.painter().circle_filled(c, 4.0, RUNNING_GREEN);
    }
    if game.is_favorite {
        let c = egui::pos2(r.right() - 15.0, r.top() + 15.0);
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
