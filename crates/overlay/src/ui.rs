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
    /// Timer · Playspace · Monado, as tabs.
    System,
    Desktop,
    Photos,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SystemTab {
    Timer,
    Playspace,
    Monado,
}

/// The wrist card's content (a queued screenshot / QR), mirrored from `photos`.
pub struct WristShot {
    pub thumb: Option<egui::TextureHandle>,
    pub qr: Option<String>,
    pub when: String,
    pub idx: usize,
    pub total: usize,
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
    /// Library: flat grid (false) or grouped by collection / source (true).
    pub library_grouped: bool,
    pub system_tab: SystemTab,
    /// Watch timer chip tapped: open the dashboard on System → Timer.
    pub watch_timer_request: bool,
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
    /// Naming something new: the keyboard targets `name_buf` instead of search.
    pub naming: bool,
    pub naming_layout: bool, // true = a desktop layout, false = a collection
    pub name_buf: String,
    // Desktop layouts (named screen arrangements).
    pub layouts: Vec<(String, usize)>, // (name, screens shown)
    pub layout_active: Option<String>,
    pub layout_apply: Option<usize>,
    pub layout_overwrite: Option<usize>,
    pub layout_delete: Option<usize>,
    pub layout_delete_arm: Option<(usize, f64)>,
    pub layout_create: Option<String>,
    pub layout_rename: Option<usize>,          // naming flow target when renaming
    pub layout_follow: Vec<bool>,               // per layout: double-B restore follows the head
    pub layout_follow_toggle: Option<(usize, bool)>,
    pub layout_renamed: Option<(usize, String)>,
    pub layout_move: Option<(usize, i32)>,
    pub restore_layout: bool,
    pub restore_layout_hidden: bool,
    pub scroll_speed: f32,
    pub drag_threshold_px: f32,
    pub controls_open: bool, // Settings: gesture reference card expanded
    pub gaze_pause: bool,
    pub recenter_on_toggle: bool,
    pub screen_restore_tilt: bool,
    pub keyboard_scale: f32,
    pub capture_max_fps: u32,
    pub capture_max_height: u32,
    pub skybox_enabled: bool,
    pub skybox_source: String,
    // Photos / gestures (monado-frame).
    pub wrist_shot: Option<WristShot>,
    pub wrist_req: crate::photos::WristRequests,
    pub gallery_items: Vec<(egui::TextureHandle, String)>,
    pub gallery_page: usize,
    pub gallery_pages: usize,
    pub gallery_total: usize,
    pub gallery_loading: bool,
    pub gallery_req: crate::photos::GalleryRequests,
    pub gesture_enabled: bool,
    pub gesture_hold_ms: f32,
    pub gesture_feedback: bool,
    pub photo_qr_detect: bool,
    pub photo_qr_autodelete: bool,
    pub photo_skip_wrist: bool,
    pub photo_skip_wrist_qr: bool,
    pub photo_cleanup_days: f32,
    pub photo_crop_margin: f32,
    pub photo_translate_ok: bool,
    pub photo_share_ok: bool,
    pub photo_dir: String,
    // Notifications.
    pub notif_enabled: bool,
    pub notif_sound: bool,
    pub notif_volume: f32,
    pub notif_xso: bool,
    pub notif_dbus_ok: bool,
    pub notif_udp_ok: bool,
    pub notif_test_request: bool,
    /// Last few notifications (title, body, "3 min ago") + unseen count.
    pub notif_history: Vec<(String, String, String)>,
    pub notif_unseen: usize,
    pub watch_history_menu: bool,
    pub watch_media_menu: bool,
    pub notif_clear_request: bool,
    // Media (MPRIS).
    pub media: Option<crate::media::MediaState>,
    pub media_request: Option<crate::media::MediaCmd>,
    // Configurable quick buttons (ids) + their requests.
    pub watch_buttons: Vec<String>,
    pub watch_button_cycle: Option<usize>, // settings: cycle slot N to the next option
    pub screenshot_request: bool,
    pub screens_toggle_request: bool,
    pub watch_photos_request: bool,
    pub keyboard_auto: bool,
    pub desktop_opacity_request: Option<(usize, f32)>,
    // Wrist watch.
    pub watch_enabled: bool,
    pub watch_24h: bool,
    pub watch_locked: bool,
    pub watch_reset_request: bool,
    /// Extra time zones (IANA ids) mirrored from the config, edited in Settings.
    pub watch_zone_ids: Vec<String>,
    pub watch_zone_cycle: Option<(usize, i32)>, // (slot, ±1 through ZONE_PRESETS)
    pub watch_zone_remove: Option<usize>,
    pub watch_zone_add: bool,
    pub skybox_reload_request: bool,
    pub skybox_custom_hint: String, // where a custom panorama is picked up from
    pub watch_date: String,
    pub watch_times: Vec<(String, String)>, // (label, HH:MM)
    pub watch_menu_request: bool,
    pub layout_cycle_request: bool,
    /// The watch's layout picker is open (replaces the clock area).
    pub watch_layout_menu: bool,
    /// Running game's client (id, frozen) for the watch's freeze button.
    pub watch_freeze_client: Option<(u32, bool)>,
    pub recenter_request: bool,
    pub recenter_playspace_request: bool,
    /// Re-scan the catalogue + re-probe artwork (picks up covers added at runtime).
    pub refresh_request: bool,
    pub keyboard_open: bool,
    /// Summon fade-in amount (1 = fully dark, 0 = clear), set by the loop.
    pub fade_in: f32,
    /// One-shot UI-sound requests, drained by the loop.
    pub sound_select: bool,
    pub sound_tab: bool,
    /// Some widget was clicked this frame (set by `interaction_pass`): the loop
    /// plays the generic click unless a more specific sound was requested.
    pub click_pulse: bool,
    /// An in-panel confirmation ("Layout saved"): text + when it appeared. Shown
    /// as a pill at the bottom of the main panel, or as a short toast while the
    /// dashboard is hidden. `flash_sound` asks the loop for the confirm chime.
    pub flash: Option<(String, std::time::Instant)>,
    pub flash_sound: bool,
    /// Two-tap confirmation for destructive buttons: (key, armed at).
    pub confirm_arm: Option<(String, std::time::Instant)>,
    /// Widgets that opt out of the hover glow this frame (rects), e.g. the
    /// watch's corner icons which sit on the card's edge.
    pub no_glow: Vec<egui::Rect>,
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
    // Playspace drag (hold trackpad / A+B on gloves, move the hand).
    pub ps_drag_hands: String,  // both | left | right | off
    pub ps_drag_button: String, // auto | pad | ab
    pub ps_drag_vertical: bool,
    pub ps_drag_follow: bool,
    pub ps_drag_offset: [f32; 3], // live session offset from dragging (readout)
    pub ps_drag_reset_request: bool,
    pub gloves: (bool, bool), // hand roles that are UdCap gloves (for the hint)
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
    // Desktop viewer (WayVR-style screen mirror).
    pub desktop_rows: Vec<crate::desktop::ScreenRow>,
    pub desktop_status: String,
    pub desktop_hid_error: Option<String>,
    pub desktop_dmabuf: bool,
    pub desktop_shown: usize,
    pub desktop_ready: bool,   // portal approved at least once
    pub desktop_pending: bool, // portal dialog in flight
    pub desktop_setup_request: bool,
    pub desktop_reselect_request: bool,
    pub desktop_move_request: Option<(usize, i32)>, // reorder approved screen (row, ±1)
    /// Bottom bar: approved screens (name, shown) in user order + keyboard state.
    pub desktop_bar: Vec<(String, bool)>,
    pub desktop_bar_toggle: Option<usize>,
    pub keyboard_shown: bool,
    pub keyboard_toggle_request: bool,
    pub keyboard_layout: String,
    /// Physical width of mirrored screens, metres.
    pub screen_width_m: f32,
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
            library_grouped: false,
            system_tab: SystemTab::Timer,
            watch_timer_request: false,
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
            naming_layout: false,
            name_buf: String::new(),
            layouts: Vec::new(),
            layout_active: None,
            layout_apply: None,
            layout_overwrite: None,
            layout_delete: None,
            layout_delete_arm: None,
            layout_create: None,
            layout_rename: None,
            layout_follow: Vec::new(),
            layout_follow_toggle: None,
            layout_renamed: None,
            layout_move: None,
            restore_layout: true,
            restore_layout_hidden: true,
            scroll_speed: 1.0,
            drag_threshold_px: 14.0,
            controls_open: false,
            gaze_pause: true,
            recenter_on_toggle: true,
            screen_restore_tilt: false,
            keyboard_scale: 1.0,
            capture_max_fps: 90,
            capture_max_height: 0,
            skybox_enabled: true,
            skybox_source: String::new(),
            wrist_shot: None,
            wrist_req: Default::default(),
            gallery_items: Vec::new(),
            gallery_page: 0,
            gallery_pages: 1,
            gallery_total: 0,
            gallery_loading: false,
            gallery_req: Default::default(),
            gesture_enabled: true,
            gesture_hold_ms: 2000.0,
            gesture_feedback: true,
            photo_qr_detect: false,
            photo_qr_autodelete: false,
            photo_skip_wrist: false,
            photo_skip_wrist_qr: false,
            photo_cleanup_days: 0.0,
            photo_crop_margin: 0.0,
            photo_translate_ok: false,
            photo_share_ok: false,
            photo_dir: String::new(),
            notif_enabled: true,
            notif_sound: true,
            notif_volume: 0.7,
            notif_xso: true,
            notif_dbus_ok: false,
            notif_udp_ok: false,
            notif_test_request: false,
            notif_history: Vec::new(),
            notif_unseen: 0,
            watch_history_menu: false,
            watch_media_menu: false,
            notif_clear_request: false,
            media: None,
            media_request: None,
            watch_buttons: vec!["keyboard".into(), "recenter".into(), "layouts".into(), "freeze".into()],
            watch_button_cycle: None,
            screenshot_request: false,
            screens_toggle_request: false,
            watch_photos_request: false,
            keyboard_auto: false,
            desktop_opacity_request: None,
            watch_enabled: true,
            watch_24h: false,
            watch_locked: true,
            watch_reset_request: false,
            watch_zone_ids: Vec::new(),
            watch_zone_cycle: None,
            watch_zone_remove: None,
            watch_zone_add: false,
            skybox_reload_request: false,
            skybox_custom_hint: String::new(),
            watch_date: String::new(),
            watch_times: Vec::new(),
            watch_menu_request: false,
            layout_cycle_request: false,
            watch_layout_menu: false,
            watch_freeze_client: None,
            recenter_request: false,
            recenter_playspace_request: false,
            refresh_request: false,
            keyboard_open: false,
            fade_in: 0.0,
            sound_select: false,
            sound_tab: false,
            click_pulse: false,
            flash: None,
            flash_sound: false,
            confirm_arm: None,
            no_glow: Vec::new(),
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
            ps_drag_hands: "both".into(),
            ps_drag_button: "auto".into(),
            ps_drag_vertical: true,
            ps_drag_follow: true,
            ps_drag_offset: [0.0; 3],
            ps_drag_reset_request: false,
            gloves: (false, false),
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
            desktop_rows: Vec::new(),
            desktop_status: String::new(),
            desktop_hid_error: None,
            desktop_dmabuf: false,
            desktop_shown: 0,
            desktop_ready: false,
            desktop_pending: false,
            desktop_setup_request: false,
            desktop_reselect_request: false,
            desktop_move_request: None,
            desktop_bar: Vec::new(),
            desktop_bar_toggle: None,
            keyboard_shown: false,
            keyboard_toggle_request: false,
            keyboard_layout: String::new(),
            screen_width_m: 1.35,
            session_minutes: None,
            last_nav: Nav::Home,
            last_splash: false,
            view_anim: 1.0,
        }
    }
}

impl LibState {
    /// Show a short confirmation ("Layout “Standing” saved") with the confirm
    /// chime — in the panel when the dashboard is up, as a toast otherwise.
    pub fn flash(&mut self, msg: impl Into<String>) {
        self.flash = Some((msg.into(), std::time::Instant::now()));
        self.flash_sound = true;
    }

    /// Two-tap guard for destructive buttons. First tap arms `key` for 3 s and
    /// returns false (the caller shows "tap again"); a second tap within that
    /// window returns true and clears the arm.
    pub fn confirm_tap(&mut self, key: &str) -> bool {
        if self.is_armed(key) {
            self.confirm_arm = None;
            true
        } else {
            self.confirm_arm = Some((key.to_string(), std::time::Instant::now()));
            self.sound_tab = true;
            false
        }
    }

    pub fn is_armed(&self, key: &str) -> bool {
        self.confirm_arm.as_ref().is_some_and(|(k, t)| k == key && t.elapsed().as_secs_f32() < 3.0)
    }
}

/// How long the confirmation pill stays up.
const FLASH_SECS: f32 = 1.9;

/// After a panel's UI is built: universal interaction feedback. Hovered
/// clickable widgets get a faint highlight (stronger while pressed) whatever
/// their fill colour, and any click sets `click_pulse` so the loop can play the
/// generic click when nothing more specific asked for a sound.
pub fn interaction_pass(ctx: &egui::Context, st: &mut LibState) {
    let snap = ctx.viewport(|v| v.interact_widgets.clone());
    if snap.clicked.is_some() {
        st.click_pulse = true;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("hover-glow")));
    for id in snap.hovered.iter() {
        let Some(resp) = ctx.read_response(*id) else { continue };
        if !resp.sense.senses_click() {
            continue;
        }
        let r = resp.rect;
        // Buttons, pills, chips, steppers — not tiles, sliders' wide tracks or the
        // search field (those have their own hover treatment or none by design).
        if r.height() > 80.0 || r.width() > 420.0 || st.no_glow.iter().any(|n| n.contains(r.center())) {
            continue;
        }
        let down = resp.is_pointer_button_down_on();
        let alpha = if down { 52 } else { 18 };
        painter.rect_filled(r, egui::CornerRadius::same(10), egui::Color32::from_white_alpha(alpha));
    }
    st.no_glow.clear();
}

const TILE_W: f32 = 168.0;
const TILE_H: f32 = 252.0; // 2:3 portrait capsule.

/// The main (centre) panel: search bar, the active view (or active-game splash),
/// the on-screen keyboard, and the launching/fade overlays.
pub fn build_main(ctx: &egui::Context, st: &mut LibState) {
    let searchable = !st.show_splash && !matches!(st.nav, Nav::Settings | Nav::System);
    if (searchable || st.naming) && st.keyboard_open {
        keyboard(ctx, st);
    }
    if searchable {
        top_bar(ctx, st);
    }
    central(ctx, st);
    overlays(ctx, st);
}

/// Foreground overlay: the confirmation pill and the summon fade-in. (The
/// launch/loading card is a standalone composition layer — see
/// `build_launch_popup` — so it survives the dashboard closing.)
fn overlays(ctx: &egui::Context, st: &mut LibState) {
    if st.flash.as_ref().is_some_and(|(_, t)| t.elapsed().as_secs_f32() > FLASH_SECS) {
        st.flash = None;
    }
    if let Some((msg, since)) = &st.flash {
        let age = since.elapsed().as_secs_f32();
        let a = if age < 0.12 { age / 0.12 } else if age > FLASH_SECS - 0.35 { ((FLASH_SECS - age) / 0.35).clamp(0.0, 1.0) } else { 1.0 };
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("flash-pill")));
        let avail = ctx.available_rect(); // above the on-panel keyboard when it's open
        let galley = ctx.fonts(|f| f.layout_no_wrap(format!("{}  {msg}", icon::CHECK), egui::FontId::proportional(16.0), egui::Color32::WHITE));
        let size = galley.size() + egui::vec2(38.0, 18.0);
        // Slides up a touch as it fades in.
        let center = egui::pos2(avail.center().x, avail.bottom() - 36.0 + (1.0 - a) * 8.0);
        let rect = egui::Rect::from_center_size(center, size);
        painter.rect_filled(rect.translate(egui::vec2(0.0, 2.0)), egui::CornerRadius::same(16), egui::Color32::from_black_alpha((a * 90.0) as u8));
        painter.rect_filled(rect, egui::CornerRadius::same(16), egui::Color32::from_rgba_unmultiplied(22, 96, 90, (a * 240.0) as u8));
        painter.rect_stroke(rect, egui::CornerRadius::same(16), egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(64, 224, 208, (a * 120.0) as u8)), egui::StrokeKind::Inside);
        painter.galley(rect.min + egui::vec2(19.0, 9.0), galley, egui::Color32::from_white_alpha((a * 255.0) as u8));
    }
    if st.fade_in > 0.001 {
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("overlay-dim")));
        painter.rect_filled(screen, egui::CornerRadius::ZERO, egui::Color32::from_black_alpha((st.fade_in * 255.0) as u8));
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
                (icon::WRENCH, Nav::System),
                (icon::MONITOR, Nav::Desktop),
                (icon::IMAGES, Nav::Photos),
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
                st.sound_tab = true;
            }
        });
    });
}

/// The wrist watch (its own layer on the left controller): batteries, clock +
/// extra time zones, quick buttons, and the menu + screen toggles like WayVR.
pub fn build_watch(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgba_unmultiplied(14, 18, 24, 235))
        .corner_radius(18)
        .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(40, 110, 120)))
        .inner_margin(egui::Margin::same(10));
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        // Batteries + position lock (top right).
        ui.horizontal(|ui| {
            if st.batteries.is_empty() {
                ui.label(egui::RichText::new("no batteries").size(12.0).color(theme::ON_SURFACE_VAR));
            }
            // Controllers individually; every other kind collapsed to one chip
            // showing its lowest charge (gloves, trackers…) so a full-body rig
            // doesn't run off the wrist.
            use crate::monado::BatteryKind;
            for b in st.batteries.iter().filter(|b| b.kind == BatteryKind::Controller) {
                battery_widget(ui, b);
                ui.add_space(4.0);
            }
            for kind in [BatteryKind::Glove, BatteryKind::Tracker, BatteryKind::Other] {
                let group: Vec<&crate::monado::BatteryInfo> = st.batteries.iter().filter(|b| b.kind == kind).collect();
                if !group.is_empty() {
                    battery_group_widget(ui, kind, &group);
                    ui.add_space(4.0);
                }
            }
            // A running/paused timer: small chip with the time left; tap to open it.
            if st.timer_running || st.timer_paused {
                let rem = st.timer_remaining;
                let txt = if rem >= 3600 {
                    format!("{}:{:02}:{:02}", rem / 3600, (rem / 60) % 60, rem % 60)
                } else {
                    format!("{}:{:02}", rem / 60, rem % 60)
                };
                let accent = if st.timer_paused { FAV_GOLD } else { theme::PRIMARY };
                let btn = egui::Button::new(egui::RichText::new(format!("{} {txt}", icon::TIMER)).size(13.0).color(accent))
                    .fill(egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 30))
                    .min_size(egui::vec2(0.0, 24.0));
                if ui.add(btn).on_hover_text(if st.timer_paused { "Timer paused · tap to open" } else { "Timer running · tap to open" }).clicked() {
                    st.watch_timer_request = true;
                    st.sound_tab = true;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Top-right: lock · media · notifications (the last two swap the
                // clock card's content, so the watch never changes size).
                let corner = |ui: &mut egui::Ui, glyph: String, on: bool, hot: bool, tip: &str| -> bool {
                    let fg = if on { egui::Color32::BLACK } else if hot { egui::Color32::from_rgb(150, 190, 255) } else { theme::ON_SURFACE_VAR };
                    let fill = if on { theme::PRIMARY } else if hot { egui::Color32::from_rgba_unmultiplied(150, 190, 255, 30) } else { egui::Color32::TRANSPARENT };
                    ui.add(egui::Button::new(egui::RichText::new(glyph).size(14.0).color(fg)).fill(fill).min_size(egui::vec2(28.0, 24.0)))
                        .on_hover_text(tip)
                        .clicked()
                };
                let (glyph, tip) = if st.watch_locked {
                    (icon::LOCK, "Position locked · tap to unlock, then grip the watch to move it")
                } else {
                    (icon::LOCK_OPEN, "Grip the watch with the right hand to move it · tap to lock")
                };
                let fg = if st.watch_locked { theme::ON_SURFACE_VAR } else { egui::Color32::BLACK };
                let btn = egui::Button::new(egui::RichText::new(glyph).size(15.0).color(fg))
                    .fill(if st.watch_locked { egui::Color32::TRANSPARENT } else { theme::PRIMARY })
                    .min_size(egui::vec2(28.0, 24.0));
                if ui.add(btn).on_hover_text(tip).clicked() {
                    st.watch_locked = !st.watch_locked;
                    st.sound_tab = true;
                }
                if !st.watch_locked {
                    ui.label(egui::RichText::new("grip to move").size(11.0).color(theme::ON_SURFACE_VAR));
                }
                let _ = &corner;
            });
        });
        // Clock + zones (or the layout picker) | quick buttons — each in its own
        // card. A queued screenshot takes the whole row (bigger preview).
        // A queued screenshot or the notification history takes the whole row.
        let wide = (st.wrist_shot.is_some() || st.watch_history_menu) && !st.watch_layout_menu && !st.watch_media_menu;
        let row_w = ui.available_width();
        ui.horizontal(|ui| {
            watch_card(ui, |ui| {
                // (the row width includes the card's own margins — keep it inside)
                ui.set_width(if wide { row_w - 30.0 } else { 214.0 });
                ui.set_min_height(120.0);
                // Corner icons inside the clock card: music (toggles the player view)
                // and the notification bell (toggles the history). Drawn at fixed
                // rects so they never push the content around.
                {
                    let r = ui.max_rect();
                    let mut no_glow: Vec<egui::Rect> = Vec::new();
                    // Tinted glyphs only (no fill): music top-right, bell top-left.
                    // `on` = its view is open (tap again to close); `badge` = a
                    // small count bubble on the glyph's shoulder.
                    // `slot`: 0 = leftmost, 1 = rightmost, 2 = second from the right.
                    let mut corner_btn = |ui: &mut egui::Ui, slot: u8, glyph: &str, on: bool, hot: bool, badge: usize, tip: &str| -> bool {
                        // The left edge sits a little into the margin so both glyphs
                        // end up the same distance from their card edge.
                        let left = slot == 0;
                        let x = match slot {
                            0 => r.left() - 8.0,
                            1 => r.right() - 28.0,
                            _ => r.right() - 56.0,
                        };
                        let rect = egui::Rect::from_min_size(egui::pos2(x, r.top() - 2.0), egui::vec2(26.0, 22.0));
                        let fg = if on { theme::PRIMARY } else if hot { egui::Color32::from_rgb(150, 190, 255) } else { theme::ON_SURFACE_VAR };
                        // A child ui at a fixed rect: nothing is allocated in the
                        // card's own layout, so the clock doesn't move.
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
                        let resp = child
                            .add(egui::Button::new(egui::RichText::new(glyph).size(13.0).color(fg)).fill(egui::Color32::TRANSPARENT).corner_radius(8).min_size(rect.size()))
                            .on_hover_text(tip);
                        no_glow.push(rect);
                        if badge > 0 {
                            // Beside the glyph (outside the button's rect), not over it.
                            let c = if left { egui::pos2(rect.right() + 6.0, rect.center().y) } else { egui::pos2(rect.left() - 6.0, rect.center().y) };
                            let p = ui.painter();
                            p.circle_filled(c, 6.5, egui::Color32::from_rgb(150, 190, 255));
                            p.text(c, egui::Align2::CENTER_CENTER, badge.min(9).to_string(), egui::FontId::proportional(9.0), egui::Color32::BLACK);
                        }
                        resp.clicked()
                    };
                    if st.media.is_some() {
                        let playing = st.media.as_ref().is_some_and(|m| m.playing);
                        if corner_btn(ui, 1, icon::MUSIC_NOTES, st.watch_media_menu, playing, 0, "Now playing") {
                            st.watch_media_menu = !st.watch_media_menu;
                            st.watch_history_menu = false;
                            st.watch_layout_menu = false;
                            st.sound_tab = true;
                        }
                    } else {
                        st.watch_media_menu = false;
                    }
                    if !st.notif_history.is_empty() {
                        let glyph = if st.notif_unseen > 0 { icon::BELL_RINGING } else { icon::BELL };
                        if corner_btn(ui, 0, glyph, st.watch_history_menu, st.notif_unseen > 0, st.notif_unseen, "Recent notifications") {
                            st.watch_history_menu = !st.watch_history_menu;
                            st.watch_media_menu = false;
                            st.watch_layout_menu = false;
                            st.notif_unseen = 0;
                            st.sound_tab = true;
                        }
                        // Clear lives in the corner too while the history is open,
                        // so the list itself needs no header row.
                        if st.watch_history_menu {
                            let slot = if st.media.is_some() { 2 } else { 1 };
                            if corner_btn(ui, slot, icon::TRASH, false, false, 0, "Clear notifications") {
                                st.notif_clear_request = true;
                                st.watch_history_menu = false;
                                st.sound_tab = true;
                            }
                        }
                    }
                    st.no_glow.extend(no_glow);
                }
                if st.watch_media_menu {
                    match &st.media {
                        Some(m) => {
                            ui.add_space(4.0);
                            let title: String = if m.title.chars().count() > 24 { format!("{}…", m.title.chars().take(23).collect::<String>()) } else { m.title.clone() };
                            ui.label(egui::RichText::new(if title.is_empty() { m.player.clone() } else { title }).size(16.0).strong().color(egui::Color32::WHITE));
                            let sub = if m.artist.is_empty() { m.player.clone() } else { format!("{} · {}", m.artist, m.player) };
                            let sub: String = if sub.chars().count() > 34 { format!("{}…", sub.chars().take(33).collect::<String>()) } else { sub };
                            ui.label(egui::RichText::new(sub).size(11.0).color(theme::ON_SURFACE_VAR));
                            ui.add_space(6.0);
                            let b = 44.0;
                            centered_row(ui, b * 3.0 + 12.0, |ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                let tb = |ui: &mut egui::Ui, g: &str, tip: &str| -> bool {
                                    ui.add(egui::Button::new(egui::RichText::new(g).size(18.0).color(theme::ON_SURFACE)).fill(theme::SURFACE_CONTAINER_HIGH).min_size(egui::vec2(b, 36.0)))
                                        .on_hover_text(tip)
                                        .clicked()
                                };
                                if tb(ui, icon::SKIP_BACK, "Previous") {
                                    st.media_request = Some(crate::media::MediaCmd::Previous);
                                }
                                if tb(ui, if m.playing { icon::PAUSE } else { icon::PLAY }, if m.playing { "Pause" } else { "Play" }) {
                                    st.media_request = Some(crate::media::MediaCmd::PlayPause);
                                }
                                if tb(ui, icon::SKIP_FORWARD, "Next") {
                                    st.media_request = Some(crate::media::MediaCmd::Next);
                                }
                            });
                        }
                        None => {
                            ui.label(egui::RichText::new("Nothing is playing").size(12.0).color(theme::ON_SURFACE_VAR));
                        }
                    }
                } else if st.watch_history_menu {
                    // No header: the lit bell (top-left) and the trash (top-right)
                    // are in the corners. One line per notification, full width:
                    // title · body, age on the right; the body truncates to fit.
                    ui.add_space(24.0);
                    for (title, body, age) in &st.notif_history {
                        ui.horizontal(|ui| {
                            let t: String = if title.chars().count() > 24 { format!("{}…", title.chars().take(23).collect::<String>()) } else { title.clone() };
                            ui.label(egui::RichText::new(t).size(12.0).strong().color(egui::Color32::WHITE));
                            let age_w = 58.0;
                            let body_w = (ui.available_width() - age_w).max(40.0);
                            if !body.is_empty() {
                                let b: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
                                ui.add_sized(
                                    egui::vec2(body_w, 16.0),
                                    egui::Label::new(egui::RichText::new(b).size(11.0).color(theme::ON_SURFACE_VAR)).truncate(),
                                );
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(age).size(10.0).color(theme::ON_SURFACE_VAR));
                            });
                        });
                    }
                } else if let (Some(shot), false) = (&st.wrist_shot, st.watch_layout_menu) {
                    let req = crate::photos::wrist_card(ui, shot.thumb.as_ref(), shot.qr.as_deref(), &shot.when, shot.idx, shot.total);
                    if req.open || req.dismiss || req.older || req.newer {
                        st.wrist_req = req;
                        st.sound_tab = true;
                    }
                } else if st.watch_layout_menu {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{}  Layouts", icon::SQUARES_FOUR)).size(14.0).strong().color(egui::Color32::WHITE));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(egui::RichText::new(icon::X).size(13.0)).min_size(egui::vec2(26.0, 22.0))).clicked() {
                                st.watch_layout_menu = false;
                            }
                        });
                    });
                    let mut apply = None;
                    egui::ScrollArea::vertical().max_height(92.0).auto_shrink([false, true]).show(ui, |ui| {
                        for (i, (name, _)) in st.layouts.iter().enumerate() {
                            let active = st.layout_active.as_deref() == Some(name.as_str());
                            let fg = if active { egui::Color32::BLACK } else { theme::ON_SURFACE };
                            let b = egui::Button::new(egui::RichText::new(name).size(14.0).color(fg))
                                .fill(if active { theme::PRIMARY } else { theme::SURFACE_CONTAINER_HIGH })
                                .min_size(egui::vec2(ui.available_width(), 28.0));
                            if ui.add(b).clicked() {
                                apply = Some(i);
                            }
                        }
                        if st.layouts.is_empty() {
                            ui.label(egui::RichText::new("No layouts saved yet").size(12.0).color(theme::ON_SURFACE_VAR));
                        }
                    });
                    if let Some(i) = apply {
                        st.layout_apply = Some(i);
                        st.watch_layout_menu = false;
                        st.sound_tab = true;
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new(&st.clock).size(40.0).strong().color(egui::Color32::WHITE));
                        ui.label(egui::RichText::new(&st.watch_date).size(14.0).color(theme::ON_SURFACE_VAR));
                    });
                    ui.add_space(2.0);
                    // Zones: fixed-width cells, centred as a row.
                    let cell = 92.0;
                    let n = st.watch_times.len() as f32;
                    centered_row(ui, (cell * n).max(0.0), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        for (label, time) in &st.watch_times {
                            ui.vertical(|ui| {
                                ui.set_width(cell);
                                ui.vertical_centered(|ui| {
                                    ui.label(egui::RichText::new(label).size(11.0).color(theme::ON_SURFACE_VAR));
                                    ui.label(egui::RichText::new(time).size(20.0).strong().color(theme::PRIMARY));
                                });
                            });
                        }
                    });
                }
            });
            if !wide {
            watch_card(ui, |ui| {
                ui.set_min_height(120.0);
                let b = 57.0;
                let quick = |ui: &mut egui::Ui, glyph: &str, on: bool, tip: &str| -> bool {
                    let fg = if on { egui::Color32::BLACK } else { theme::ON_SURFACE };
                    let btn = egui::Button::new(egui::RichText::new(glyph).size(24.0).color(fg))
                        .fill(if on { theme::PRIMARY } else { theme::SURFACE_CONTAINER_HIGH })
                        .min_size(egui::vec2(b, b));
                    ui.add(btn).on_hover_text(tip).clicked()
                };
                let ids = st.watch_buttons.clone();
                for row in ids.chunks(2) {
                    ui.horizontal(|ui| {
                        for id in row {
                            watch_quick_button(ui, st, id, &quick);
                        }
                    });
                }
            });
            }
        });
        // Menu + screens (fixed numbering, same as the bottom bar), in a card.
        watch_card(ui, |ui| {
            // (inside the frame, available width already excludes the margins;
            // a hair narrower still, so the stroke never touches the panel edge)
            ui.set_width(ui.available_width() - 4.0);
            ui.horizontal(|ui| {
                let menu = egui::Button::new(egui::RichText::new(icon::LIST).size(22.0).color(theme::ON_SURFACE))
                    .fill(theme::SURFACE_CONTAINER_HIGH)
                    .min_size(egui::vec2(56.0, 42.0));
                if ui.add(menu).on_hover_text("Monadeck menu").clicked() {
                    st.watch_menu_request = true;
                    st.sound_tab = true;
                }
                ui.add_space(6.0);
                // A thin separator between the menu and the screens.
                let (r, _) = ui.allocate_exact_size(egui::vec2(1.0, 30.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 0.0, egui::Color32::from_white_alpha(28));
                ui.add_space(6.0);
                let mut toggle = None;
                for (i, (name, shown)) in st.desktop_bar.iter().enumerate() {
                    let fg = if *shown { egui::Color32::BLACK } else { theme::ON_SURFACE };
                    let btn = egui::Button::new(egui::RichText::new(format!("{} {}", icon::MONITOR, i + 1)).size(14.0).color(fg))
                        .fill(if *shown { theme::PRIMARY } else { theme::SURFACE_CONTAINER_HIGH })
                        .min_size(egui::vec2(56.0, 42.0));
                    if ui.add(btn).on_hover_text(name).clicked() {
                        toggle = Some(i);
                    }
                }
                if st.desktop_bar.is_empty() {
                    ui.label(egui::RichText::new("no screens approved").size(12.0).color(theme::ON_SURFACE_VAR));
                }
                if let Some(i) = toggle {
                    st.desktop_bar_toggle = Some(i);
                    st.sound_tab = true;
                }
            });
        });
    });
}

/// Time zones offered by the watch's zone picker (◀ ▶ cycles through these;
/// a zone set by hand in the config that isn't listed still works).
pub const ZONE_PRESETS: &[&str] = &[
    "Pacific/Honolulu",
    "America/Anchorage",
    "America/Los_Angeles",
    "America/Denver",
    "America/Chicago",
    "America/New_York",
    "America/Toronto",
    "America/Sao_Paulo",
    "Atlantic/Reykjavik",
    "Europe/London",
    "Europe/Paris",
    "Europe/Berlin",
    "Europe/Madrid",
    "Europe/Rome",
    "Europe/Warsaw",
    "Europe/Helsinki",
    "Europe/Moscow",
    "Asia/Dubai",
    "Asia/Kolkata",
    "Asia/Bangkok",
    "Asia/Singapore",
    "Asia/Hong_Kong",
    "Asia/Shanghai",
    "Asia/Seoul",
    "Asia/Tokyo",
    "Australia/Perth",
    "Australia/Sydney",
    "Pacific/Auckland",
    "UTC",
];

/// Everything a watch quick button can do, in cycle order.
pub const WATCH_BUTTON_IDS: [&str; 9] = ["keyboard", "recenter", "layouts", "freeze", "timer", "screenshot", "screens", "mute", "photos"];

pub fn watch_button_info(id: &str) -> (&'static str, &'static str) {
    match id {
        "keyboard" => (icon::KEYBOARD, "VR keyboard"),
        "recenter" => (icon::CROSSHAIR, "Recenter playspace"),
        "layouts" => (icon::SQUARES_FOUR, "Screen layouts"),
        "freeze" => (icon::SNOWFLAKE, "Freeze game controllers"),
        "timer" => (icon::TIMER, "Timer"),
        "screenshot" => (icon::CAMERA, "Take a screenshot"),
        "screens" => (icon::MONITOR, "Hide / restore all screens"),
        "mute" => (icon::BELL_SLASH, "Mute notifications"),
        "photos" => (icon::IMAGES, "Photos"),
        _ => (icon::QUESTION, "Unassigned"),
    }
}

fn watch_quick_button(ui: &mut egui::Ui, st: &mut LibState, id: &str, quick: &dyn Fn(&mut egui::Ui, &str, bool, &str) -> bool) {
    let (glyph, tip) = watch_button_info(id);
    match id {
        "keyboard" => {
            if quick(ui, glyph, st.keyboard_shown, tip) {
                st.keyboard_toggle_request = true;
                st.sound_tab = true;
            }
        }
        "recenter" => {
            if quick(ui, glyph, false, tip) {
                st.recenter_playspace_request = true;
                st.sound_tab = true;
            }
        }
        "layouts" => {
            if quick(ui, glyph, st.watch_layout_menu, tip) {
                st.watch_layout_menu = !st.watch_layout_menu;
                st.watch_history_menu = false;
                st.watch_media_menu = false;
                st.sound_tab = true;
            }
        }
        "freeze" => {
            let (frozen, enabled) = match st.watch_freeze_client {
                Some((_, f)) => (f, true),
                None => (false, false),
            };
            ui.add_enabled_ui(enabled, |ui| {
                if quick(ui, glyph, frozen, tip) {
                    if let Some((id, _)) = st.watch_freeze_client {
                        st.freeze_toggle_request = Some(id);
                        st.sound_tab = true;
                    }
                }
            });
        }
        "timer" => {
            if quick(ui, glyph, st.timer_running, tip) {
                st.watch_timer_request = true;
                st.sound_tab = true;
            }
        }
        "screenshot" => {
            if quick(ui, glyph, false, tip) {
                st.screenshot_request = true;
                st.sound_tab = true;
            }
        }
        "screens" => {
            if quick(ui, glyph, st.desktop_shown > 0, tip) {
                st.screens_toggle_request = true;
            }
        }
        "mute" => {
            if quick(ui, glyph, !st.notif_sound, tip) {
                st.notif_sound = !st.notif_sound;
                st.sound_tab = true;
            }
        }
        "photos" => {
            if quick(ui, glyph, false, tip) {
                st.watch_photos_request = true;
                st.sound_tab = true;
            }
        }
        _ => {
            quick(ui, glyph, false, tip);
        }
    }
}

/// A subtle inset card used to group the watch's areas.
fn watch_card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::default()
        .fill(egui::Color32::from_rgb(22, 28, 36))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(16)))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            // Cards sit in a horizontal row; their contents stack vertically.
            ui.vertical(contents);
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
            // Mirrored screens + keyboard, centred in the bar (fixed order so a
            // screen is always in the same spot — the WayVR wrist-bar problem).
            if !st.desktop_bar.is_empty() {
                let count = st.desktop_bar.len();
                let pill_w = 64.0;
                let kb_w = 48.0;
                let total = count as f32 * pill_w + kb_w + count as f32 * 8.0;
                let bar = ui.max_rect();
                let rect = egui::Rect::from_center_size(bar.center(), egui::vec2(total, 40.0));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
                child.spacing_mut().item_spacing.x = 8.0;
                let mut toggle = None;
                for (i, (name, shown)) in st.desktop_bar.iter().enumerate() {
                    let fg = if *shown { egui::Color32::BLACK } else { theme::ON_SURFACE };
                    // Numbered, not named: the number is the position, which never moves.
                    let btn = egui::Button::new(
                        egui::RichText::new(format!("{}  {}", icon::MONITOR, i + 1)).size(15.0).color(fg),
                    )
                    .fill(if *shown { theme::PRIMARY } else { theme::SURFACE_CONTAINER_HIGH })
                    .min_size(egui::vec2(pill_w, 40.0));
                    let tip = format!("{} · {}", name, if *shown { "hide" } else { "show" });
                    if child.add(btn).on_hover_text(tip).clicked() {
                        toggle = Some(i);
                    }
                }
                let kfg = if st.keyboard_shown { egui::Color32::BLACK } else { theme::ON_SURFACE };
                let kbtn = egui::Button::new(egui::RichText::new(icon::KEYBOARD).size(20.0).color(kfg))
                    .fill(if st.keyboard_shown { theme::PRIMARY } else { theme::SURFACE_CONTAINER_HIGH })
                    .min_size(egui::vec2(kb_w, 40.0));
                if child.add(kbtn).on_hover_text("VR keyboard").clicked() {
                    st.keyboard_toggle_request = true;
                    st.sound_tab = true;
                }
                if let Some(i) = toggle {
                    st.desktop_bar_toggle = Some(i);
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

/// One chip for a whole kind of device: the lowest charge in the group (tinted
/// by it), "×N" when several, and every member's charge on hover.
fn battery_group_widget(ui: &mut egui::Ui, kind: crate::monado::BatteryKind, group: &[&crate::monado::BatteryInfo]) {
    use crate::monado::BatteryKind;
    let Some(lowest) = group.iter().filter(|b| !b.charging).min_by(|a, b| a.charge.total_cmp(&b.charge)).or(group.first()) else {
        return;
    };
    let pct = (lowest.charge * 100.0).round() as i32;
    let bat = if lowest.charging {
        icon::BATTERY_CHARGING
    } else if lowest.charge > 0.66 {
        icon::BATTERY_FULL
    } else if lowest.charge > 0.33 {
        icon::BATTERY_MEDIUM
    } else if lowest.charge > 0.1 {
        icon::BATTERY_LOW
    } else {
        icon::BATTERY_WARNING
    };
    let color = if lowest.charge > 0.33 {
        RUNNING_GREEN
    } else if lowest.charge > 0.15 {
        FAV_GOLD
    } else {
        STOP_RED
    };
    let (dev, name) = match kind {
        BatteryKind::Glove => (icon::HAND, "Gloves"),
        BatteryKind::Tracker => (icon::CIRCLE, "Trackers"),
        BatteryKind::Controller => (icon::GAME_CONTROLLER, "Controllers"),
        BatteryKind::Other => (icon::CIRCLE, "Devices"),
    };
    let count = if group.len() > 1 { format!(" ×{}", group.len()) } else { String::new() };
    let tip = group
        .iter()
        .enumerate()
        .map(|(i, b)| format!("{name} {}: {}%{}", i + 1, (b.charge * 100.0).round() as i32, if b.charging { " (charging)" } else { "" }))
        .collect::<Vec<_>>()
        .join("\n");
    ui.label(egui::RichText::new(format!("{dev} {bat} {pct}%{count}")).size(14.0).color(color))
        .on_hover_text(format!("Lowest of {}:\n{tip}", name.to_lowercase()));
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
                let what = if st.layout_rename.is_some() {
                    "Rename layout:"
                } else if st.naming_layout {
                    "New layout:"
                } else {
                    "New collection:"
                };
                ui.label(egui::RichText::new(format!("{}  {what}", icon::FOLDER_PLUS)).size(14.0).color(theme::ON_SURFACE_VAR));
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
            let commit = if !naming {
                "Done"
            } else if st.layout_rename.is_some() {
                "Rename"
            } else {
                "Create"
            };
            let can_commit = !naming || !st.name_buf.trim().is_empty();
            let commit_resp = ui.add_enabled_ui(can_commit, |ui| fkey(ui, commit, 130.0, true)).inner;
            if !can_commit {
                commit_resp.on_hover_text("Type a name first");
            } else if commit_resp.clicked() {
                if naming {
                    let name = st.name_buf.trim().to_string();
                    if !name.is_empty() {
                        if let Some(i) = st.layout_rename.take() {
                            st.layout_renamed = Some((i, name));
                        } else if st.naming_layout {
                            st.layout_create = Some(name);
                        } else {
                            st.collection_create = Some(name);
                        }
                    }
                    st.name_buf.clear();
                    st.naming = false;
                    st.naming_layout = false;
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
                    st.naming_layout = false;
                    st.layout_rename = None;
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
            Nav::Library => library_view(ui, st),
            Nav::Favorites => favorites_view(ui, st),
            Nav::System => {
                st.visible_now.clear();
                st.hovered_index = None;
                system_view(ui, st);
            }
            Nav::Desktop => {
                st.visible_now.clear();
                st.hovered_index = None;
                desktop_view(ui, st);
            }
            Nav::Photos => {
                st.visible_now.clear();
                st.hovered_index = None;
                photos_view(ui, st);
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
            st.sound_tab = true;
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
    game_grid(ui, st, &shown, "grid", false);
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
    game_grid(ui, st, &shown, "favs", true);
}

/// A vertical wrapped grid of the given game indices, with hover/select/launch
/// tracking. Shared by Library + Favorites. With `home_on_select`, picking a
/// game also jumps to Home so the hero's Play button is one tap away (wanted
/// for Favorites/Tags; Library stays put as the browsing view).
fn game_grid(ui: &mut egui::Ui, st: &mut LibState, shown: &[usize], salt: &str, home_on_select: bool) {
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
        if home_on_select {
            st.nav = Nav::Home;
        }
    }
    if launch.is_some() {
        st.launch_request = launch;
    }
}

/// Library: a mode row (all games / grouped by collection) over the grid or
/// the former Categories view.
fn library_view(ui: &mut egui::Ui, st: &mut LibState) {
    ui.horizontal(|ui| {
        if chip(ui, &format!("{}  All games", icon::SQUARES_FOUR), !st.library_grouped).clicked() {
            st.library_grouped = false;
            st.sound_tab = true;
        }
        if chip(ui, &format!("{}  Collections", icon::TAG), st.library_grouped).clicked() {
            st.library_grouped = true;
            st.sound_tab = true;
        }
    });
    ui.add_space(6.0);
    if st.library_grouped {
        tags_view(ui, st);
    } else {
        grid_view(ui, st, "Library");
    }
}

/// System: Timer · Playspace · Monado as tabs on one page.
fn system_view(ui: &mut egui::Ui, st: &mut LibState) {
    ui.horizontal(|ui| {
        for (glyph, label, tab) in [
            (icon::TIMER, "Timer", SystemTab::Timer),
            (icon::ARROWS_OUT_CARDINAL, "Playspace", SystemTab::Playspace),
            (icon::STACK, "Monado", SystemTab::Monado),
        ] {
            if chip(ui, &format!("{glyph}  {label}"), st.system_tab == tab).clicked() && st.system_tab != tab {
                st.system_tab = tab;
                st.sound_tab = true;
            }
        }
    });
    ui.add_space(8.0);
    match st.system_tab {
        SystemTab::Timer => tools_view(ui, st),
        SystemTab::Playspace => playspace_view(ui, st),
        SystemTab::Monado => monado_view(ui, st),
    }
}

fn tags_view(ui: &mut egui::Ui, st: &mut LibState) {
    view_header(ui, st, "Collections");
    ui.add_space(8.0);

    // Create a new collection (works even with no games yet).
    if chip(ui, &format!("{}  New collection", icon::FOLDER_PLUS), false).clicked() {
        st.naming = true;
        st.name_buf.clear();
        st.keyboard_open = true;
        st.sound_tab = true;
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
                let key = format!("col-del:{ci}");
                let armed = st.is_armed(&key);
                let del = egui::Button::new(
                    egui::RichText::new(if armed { format!("{}  Tap again to delete", icon::TRASH) } else { icon::TRASH.to_string() })
                        .size(13.0)
                        .color(if armed { egui::Color32::BLACK } else { STOP_RED }),
                )
                .fill(if armed { STOP_RED } else { egui::Color32::TRANSPARENT })
                .corner_radius(8)
                .min_size(egui::vec2(28.0, 24.0));
                if ui.add(del).on_hover_text("Delete collection").clicked() && st.confirm_tap(&key) {
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
        // Picking a game from a category means "play this": land on the Home
        // hero where the Play button is, instead of leaving the user here.
        st.nav = Nav::Home;
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
                st.sound_tab = true;
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

/// The gesture reference (Settings → Controllers → Help): one line per gesture,
/// grouped by what the laser is on.
fn controls_card(ui: &mut egui::Ui) {
    const GROUPS: &[(&str, &[(&str, &str)])] = &[
        (
            "Anywhere",
            &[
                ("Left system button", "summon / dismiss the dashboard (it re-centres in front of you)"),
                ("Double-B (left hand)", "hide every screen + the keyboard, or bring them back"),
                ("Hold trackpad, move hand", "drag the playspace (A + B on a UdCap glove) · System → Playspace → Drag"),
                ("Trackpad twice", "snap the playspace back (A + B twice on a glove)"),
                ("Trigger", "click on the dashboard, the watch, the keyboard, photo windows"),
            ],
        ),
        (
            "On a screen",
            &[
                ("Point", "moves the mouse"),
                ("Trigger", "left click · keep holding and move past the drag threshold to drag"),
                ("A", "right click"),
                ("B", "left click without moving the cursor (fiddly targets)"),
                ("Thumbstick", "scroll (speed in Desktop → Behaviour)"),
                ("Grip", "move the screen (a docked group moves as one)"),
                ("Grip + trigger, push / pull", "resize"),
                ("Grip + stick ▲▼", "push it away / pull it closer"),
                ("Grip + trigger + stick ◀▶", "curve it"),
                ("Release next to another screen", "dock to that edge (teal bar shows the spot)"),
                ("Aim just above the top edge", "shows the swap island (also for 2 s when a screen appears) · tap another number to put that screen here"),
                ("B while gripping", "undock"),
            ],
        ),
        (
            "On the keyboard",
            &[
                ("Trigger", "type · hold to repeat"),
                ("Tap a modifier", "one-shot latch · tap it again within 1.5 s to send it alone (Super opens the launcher)"),
                ("Shift twice", "lock · a third tap clears"),
                ("Other hand on a screen", "that hand keeps the mouse; both hands can type"),
                ("Grip", "move · release under a screen's dock spot to attach it"),
                ("Top bar", "layout · clipboard · latched modifiers · screen pills · dock / undock · close"),
            ],
        ),
        (
            "Watch (left wrist)",
            &[
                ("Point with the right hand", "it wins over whatever is behind it"),
                ("Trigger", "tap a button · corner icons switch the card (media, bell)"),
                ("Grip (right hand, unlocked)", "move it · the spot is remembered"),
                ("Grip + trigger, push / pull", "resize it"),
            ],
        ),
        (
            "Photos",
            &[
                ("Finger frame (both hands)", "screenshot, if the gesture is enabled in Photos"),
                ("Grip a photo window", "move it"),
                ("Wrist card ‹ ›", "browse new shots · open puts one in a window"),
            ],
        ),
    ];
    ui.add_space(4.0);
    for (i, (title, rows)) in GROUPS.iter().enumerate() {
        if i > 0 {
            ui.add_space(8.0);
        }
        ui.label(egui::RichText::new(*title).size(13.0).strong().color(theme::PRIMARY));
        ui.add_space(3.0);
        for (keys, what) in rows.iter() {
            ui.horizontal(|ui| {
                ui.add_space(2.0);
                ui.add_sized(
                    egui::vec2(230.0, 22.0),
                    egui::Label::new(egui::RichText::new(*keys).size(14.0).color(theme::ON_SURFACE)).wrap_mode(egui::TextWrapMode::Truncate),
                );
                ui.add_space(6.0);
                ui.add(egui::Label::new(egui::RichText::new(*what).size(13.0).color(theme::ON_SURFACE_VAR)).wrap());
            });
        }
    }
    ui.add_space(6.0);
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
/// A compact square icon button (rows with several actions). `hot` = danger
/// state (e.g. delete armed).
fn icon_button(ui: &mut egui::Ui, glyph: &str, tip: &str, hot: bool) -> egui::Response {
    let fg = if hot { egui::Color32::BLACK } else { theme::ON_SURFACE };
    ui.add(
        egui::Button::new(egui::RichText::new(glyph).size(18.0).color(fg))
            .fill(if hot { STOP_RED } else { theme::SURFACE_CONTAINER_HIGH })
            .corner_radius(10)
            .min_size(egui::vec2(42.0, 42.0)),
    )
    .on_hover_text(tip)
}

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
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // `Info` is the generic fallback for future toasts.
pub enum ToastKind {
    Timer,
    Battery,
    Info,
    /// A desktop / XSOverlay notification.
    Notification,
    /// "Done" feedback for an action taken while the dashboard was hidden.
    Confirm,
    /// The boot greeting.
    Welcome,
}

impl ToastKind {
    fn style(self) -> (&'static str, egui::Color32) {
        match self {
            ToastKind::Timer => (icon::TIMER, theme::PRIMARY),
            ToastKind::Battery => (icon::BATTERY_WARNING, FAV_GOLD),
            ToastKind::Info => (icon::BELL_RINGING, theme::PRIMARY),
            ToastKind::Notification => (icon::BELL, egui::Color32::from_rgb(150, 190, 255)),
            ToastKind::Confirm => (icon::CHECK_CIRCLE, theme::PRIMARY),
            ToastKind::Welcome => (icon::HAND_WAVING, theme::PRIMARY),
        }
    }
}

/// The floating notification card (its own layer; shows over a game too). The
/// quad is cleared transparent, so the card hugs its content and floats centred.
pub fn build_toast(ctx: &egui::Context, title: &str, body: &str, kind: ToastKind, icon_tex: Option<&egui::TextureHandle>) {
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
                    match icon_tex {
                        Some(t) => {
                            egui::Image::new(egui::load::SizedTexture::new(t.id(), egui::vec2(44.0, 44.0)))
                                .corner_radius(10)
                                .paint_at(ui, egui::Rect::from_center_size(chip.center(), egui::vec2(44.0, 44.0)));
                        }
                        None => {
                            ui.painter().text(
                                chip.center(),
                                egui::Align2::CENTER_CENTER,
                                glyph,
                                egui::FontId::proportional(27.0),
                                accent,
                            );
                        }
                    }
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        const TEXT_W: f32 = 780.0;
                        ui.set_max_width(TEXT_W);
                        // Hard row caps + break-anywhere so a long title, a
                        // multi-line body or an unbroken URL can't spill past the
                        // panel's edges (Discord loves all three).
                        let clamp = |text: &str, size: f32, color: egui::Color32, rows: usize| {
                            let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                            let mut job = egui::text::LayoutJob::simple(one_line, egui::FontId::proportional(size), color, TEXT_W);
                            job.wrap = egui::text::TextWrapping { max_width: TEXT_W, max_rows: rows, break_anywhere: true, overflow_character: Some('…') };
                            job
                        };
                        ui.add(egui::Label::new(clamp(title, 21.0, egui::Color32::WHITE, 1)));
                        if !body.is_empty() {
                            ui.add_space(3.0);
                            ui.add(egui::Label::new(clamp(body, 15.0, theme::ON_SURFACE_VAR, 2)));
                        }
                    });
                });
            });
        });
}

/// The game-launch popup (its own composition layer, SteamVR-style): the game's
/// hero art — or a name-tinted gradient when there's none — behind a centred
/// spinner, title, and status line. Shown while a game starts up, independent of
/// the dashboard, so closing the overlay doesn't hide it. The panel is cleared
/// transparent, so the rounded card is the whole visible popup.
pub fn build_launch_popup(ctx: &egui::Context, name: &str, status: &str, hero: &ArtState) {
    egui::Area::new(egui::Id::new("launch-popup")).fixed_pos(egui::pos2(0.0, 0.0)).show(ctx, |ui| {
        let rect = ctx.screen_rect().shrink(5.0);
        let radius = egui::CornerRadius::same(26);
        let painter = ui.painter().clone();
        // Rounded base fill, so the card's corners stay transparent.
        painter.rect_filled(rect, radius, egui::Color32::from_rgb(11, 14, 18));
        match hero {
            ArtState::Ready(tex) => {
                egui::Image::new(egui::load::SizedTexture::new(tex.id(), rect.size()))
                    .uv(cover_uv(tex.size(), rect))
                    .corner_radius(radius)
                    .paint_at(ui, rect);
                // Scrim for legible text over the art.
                painter.rect_filled(rect, radius, egui::Color32::from_black_alpha(170));
            }
            _ => {
                painter.rect_filled(rect, radius, hero_tint(name));
                painter.rect_filled(rect, radius, egui::Color32::from_black_alpha(60));
            }
        }
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(44, 52, 62)),
            egui::StrokeKind::Inside,
        );
        // Centred content: spinner, title, status.
        let c = rect.center();
        let spin = egui::Rect::from_center_size(egui::pos2(c.x, c.y - 66.0), egui::vec2(46.0, 46.0));
        ui.put(spin, egui::Spinner::new().size(44.0).color(theme::PRIMARY));
        painter.text(
            egui::pos2(c.x, c.y),
            egui::Align2::CENTER_CENTER,
            name,
            egui::FontId::proportional(34.0),
            egui::Color32::WHITE,
        );
        painter.text(
            egui::pos2(c.x, c.y + 40.0),
            egui::Align2::CENTER_CENTER,
            status,
            egui::FontId::proportional(17.0),
            theme::ON_SURFACE_VAR,
        );
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
                    st.flash(format!("{} uses the global offset", st.ps_game_name));
                } else {
                    st.playspace_x = 0.0;
                    st.playspace_y = 0.0;
                    st.playspace_z = 0.0;
                    st.playspace_yaw = 0.0;
                    st.flash("Playspace offset reset");
                }
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
        ui.add_space(18.0);

        section(ui, "Drag", |ui| {
            let mut t = false;
            let glove_hint = match st.gloves {
                (true, true) => "gloves detected on both hands: A + B",
                (true, false) => "left glove: A + B · right: trackpad",
                (false, true) => "right glove: A + B · left: trackpad",
                _ => "trackpad press (A + B on UdCap gloves)",
            };
            setting_row(ui, "Hold to move", Some(&format!("Hold the button and move your hand to pull the world along · press it twice to snap back · {glove_hint}")), |ui| {
                for (label, id) in [("Off", "off"), ("Right", "right"), ("Left", "left"), ("Both", "both")] {
                    if pill(ui, label, 74.0, st.ps_drag_hands == id).clicked() && st.ps_drag_hands != id {
                        st.ps_drag_hands = id.into();
                        t = true;
                    }
                }
            });
            divider(ui);
            setting_row(ui, "Button", Some("Auto picks the trackpad, or A + B on a hand that is a glove"), |ui| {
                for (label, id) in [("A + B", "ab"), ("Trackpad", "pad"), ("Auto", "auto")] {
                    if pill(ui, label, 92.0, st.ps_drag_button == id).clicked() && st.ps_drag_button != id {
                        st.ps_drag_button = id.into();
                        t = true;
                    }
                }
            });
            divider(ui);
            setting_row(ui, "Up & down too", Some("Off = horizontal only (no accidental floor changes)"), |ui| {
                t |= seg_toggle(ui, &mut st.ps_drag_vertical);
            });
            divider(ui);
            setting_row(ui, "Overlays follow you", Some("Screens, keyboard, photos and the menu keep their place around you while you drag"), |ui| {
                t |= seg_toggle(ui, &mut st.ps_drag_follow);
            });
            divider(ui);
            let o = st.ps_drag_offset;
            let moved = o.iter().any(|v| v.abs() > 1e-4);
            let sub = if moved {
                format!("{:+.2} m · {:+.2} m · {:+.2} m on top of the offset above · this session only", o[0], o[1], o[2])
            } else {
                "Nothing dragged yet · this session only".to_string()
            };
            setting_row(ui, "Dragged so far", Some(&sub), |ui| {
                if moved && action_button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Snap back").clicked() {
                    st.ps_drag_reset_request = true;
                    t = true;
                }
            });
            if t {
                st.sound_tab = true;
            }
        });
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
                    let key = format!("kill:{}", c.id);
                    let armed = st.is_armed(&key);
                    let kill = egui::Button::new(
                        egui::RichText::new(if armed { format!("{}  Sure?", icon::WARNING) } else { format!("{}  Kill", icon::X) })
                            .size(16.0)
                            .color(if armed { egui::Color32::BLACK } else { egui::Color32::WHITE }),
                    )
                    .fill(if armed { STOP_RED } else { egui::Color32::from_rgb(176, 64, 64) })
                    .corner_radius(10)
                    .min_size(egui::vec2(78.0, 42.0));
                    if ui.add(kill).on_hover_text(if armed { "Tap again to close it" } else { "Close this app (two taps)" }).clicked() && st.confirm_tap(&key) {
                        st.kill_request = Some(c.name.clone());
                        st.flash(format!("Closing {}…", c.name));
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

/// The Photos page: screenshot gallery + finger-frame gesture and photo settings.
fn photos_view(ui: &mut egui::Ui, st: &mut LibState) {
    page_header(ui, icon::IMAGES, "Photos");
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        section(ui, "Gallery", |ui| {
            let req = crate::photos::gallery_ui(ui, &st.gallery_items, st.gallery_page, st.gallery_pages, st.gallery_total, st.gallery_loading);
            if req.open.is_some() || req.delete.is_some() || req.prev || req.next || req.refresh {
                st.gallery_req = req;
                st.sound_tab = true;
            }
        });
        ui.add_space(6.0);
        section(ui, "Finger-frame gesture", |ui| {
            let mut t = false;
            setting_row(ui, "Gesture enabled", Some("Frame a shot with both hands to take a screenshot"), |ui| {
                t |= seg_toggle(ui, &mut st.gesture_enabled);
            });
            divider(ui);
            setting_row(ui, "Hold delay", Some("Hold the frame this long before the viewfinder appears; then curl an index finger to shoot"), |ui| {
                modern_slider(ui, &mut st.gesture_hold_ms, 500.0..=4000.0, 360.0, |v| format!("{:.1} s", v / 1000.0));
            });
            divider(ui);
            setting_row(ui, "Show viewfinder in headset", Some("Off = arm silently"), |ui| {
                t |= seg_toggle(ui, &mut st.gesture_feedback);
            });
            if t {
                st.sound_tab = true;
            }
        });
        ui.add_space(6.0);
        section(ui, "New screenshots", |ui| {
            let mut t = false;
            setting_row(ui, "Detect QR codes", Some("A QR in the shot shows its content on the wrist instead of the photo"), |ui| {
                t |= seg_toggle(ui, &mut st.photo_qr_detect);
            });
            divider(ui);
            ui.add_enabled_ui(st.photo_qr_detect, |ui| {
                setting_row(ui, "Delete the screenshot, keep only the code", None, |ui| {
                    t |= seg_toggle(ui, &mut st.photo_qr_autodelete);
                });
            });
            divider(ui);
            setting_row(ui, "Open screenshots directly", Some("Skip the wrist card, open a photo window right away"), |ui| {
                t |= seg_toggle(ui, &mut st.photo_skip_wrist);
            });
            divider(ui);
            setting_row(ui, "Open QR codes directly", Some("Links open on the desktop, text in a window"), |ui| {
                t |= seg_toggle(ui, &mut st.photo_skip_wrist_qr);
            });
            divider(ui);
            setting_row(ui, "Crop margin", Some("Trim this much off each edge of new shots — hides stray fingers"), |ui| {
                stepper_inline(ui, &mut st.photo_crop_margin, 0.0, 25.0, 5.0, |v| format!("{v:.0} %"));
            });
            divider(ui);
            setting_row(ui, "Auto-cleanup", Some("Delete screenshots older than this on launch (0 = keep forever)"), |ui| {
                stepper_inline(ui, &mut st.photo_cleanup_days, 0.0, 90.0, 5.0, |v| if v < 1.0 { "off".into() } else { format!("{v:.0} days") });
            });
            if t {
                st.sound_tab = true;
            }
        });
        ui.add_space(6.0);
        section(ui, "Integrations", |ui| {
            let yn = |b: bool| if b { "configured" } else { "not configured (crates/overlay/*.env at build time)" };
            setting_row(ui, "Translate (vision model)", Some(yn(st.photo_translate_ok)), |_| {});
            divider(ui);
            setting_row(ui, "Share (Picsur)", Some(yn(st.photo_share_ok)), |_| {});
            divider(ui);
            setting_row(ui, "Folder", Some(&st.photo_dir), |_| {});
        });
    });
}

/// The Desktop page: mirror monitors into VR (WayVR-style) and tune them.
fn desktop_view(ui: &mut egui::Ui, st: &mut LibState) {
    page_header(ui, icon::MONITOR, "Desktop");
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        section(ui, "Screens", |ui| {
            ui.label(egui::RichText::new(&st.desktop_status).color(theme::ON_SURFACE_VAR));
            ui.add_space(6.0);
            if !st.desktop_ready {
                ui.horizontal(|ui| {
                    let (glyph, label) = if st.desktop_pending {
                        (icon::HOURGLASS, "Waiting for approval…")
                    } else {
                        (icon::MONITOR, "Set up screens")
                    };
                    if action_button(ui, glyph, label).clicked() && !st.desktop_pending {
                        st.desktop_setup_request = true;
                        st.sound_tab = true;
                    }
                });
                divider(ui);
            }
            if st.desktop_rows.is_empty() {
                ui.label(egui::RichText::new("No screens detected.").color(theme::ON_SURFACE_VAR));
            }
            let approved = st.desktop_rows.iter().filter(|r| r.approved).count();
            let mut mv = None;
            for (i, row) in st.desktop_rows.iter().enumerate() {
                if i > 0 {
                    divider(ui);
                }
                let sub = match &row.hint {
                    Some(h) => format!("{} · {h}", row.detail),
                    None => row.detail.clone(),
                };
                let mut op = row.opacity;
                setting_row(ui, &row.name, Some(&sub), |ui| {
                    if row.approved {
                        // Order in the bottom bar: ◀ / ▶ (left = earlier).
                        ui.add_enabled_ui(i + 1 < approved, |ui| {
                            if action_button(ui, icon::CARET_RIGHT, "").clicked() {
                                mv = Some((i, 1));
                            }
                        });
                        ui.add_enabled_ui(i > 0, |ui| {
                            if action_button(ui, icon::CARET_LEFT, "").clicked() {
                                mv = Some((i, -1));
                            }
                        });
                        ui.label(
                            egui::RichText::new(if row.shown { "shown" } else { "hidden" })
                                .size(13.0)
                                .color(theme::ON_SURFACE_VAR),
                        );
                        ui.add_space(8.0);
                        stepper_inline(ui, &mut op, 0.2, 1.0, 0.1, |v| format!("{:.0}%", v * 100.0));
                    }
                });
                if row.approved && (op - row.opacity).abs() > 1e-3 {
                    st.desktop_opacity_request = Some((i, op));
                }
            }
            if let Some(m) = mv {
                st.desktop_move_request = Some(m);
                st.sound_tab = true;
            }
        });
        if st.desktop_ready {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let armed = st.is_armed("repick");
                    if reset_button(ui, if armed { "Tap again to re-pick" } else { "Re-pick screens" }).clicked() && st.confirm_tap("repick") {
                        st.desktop_reselect_request = true;
                        st.flash("Pick your screens in the desktop dialog");
                    }
                });
            });
        }
        ui.add_space(6.0);
        section(ui, "Layouts", |ui| {
            ui.label(
                egui::RichText::new("Named arrangements of your screens + keyboard (e.g. Standing, Lying down).")
                    .color(theme::ON_SURFACE_VAR),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if action_button(ui, icon::PLUS, "Save current as new layout").clicked() {
                    st.naming = true;
                    st.naming_layout = true;
                    st.name_buf.clear();
                    st.keyboard_open = true;
                    st.sound_tab = true;
                }
            });
            let now = ui.input(|i| i.time);
            if st.layout_delete_arm.is_some_and(|(_, t)| now - t > 3.0) {
                st.layout_delete_arm = None;
            }
            let (mut apply, mut overwrite, mut delete, mut arm) = (None, None, None, None);
            let (mut rename, mut mv) = (None, None);
            let n_layouts = st.layouts.len();
            for (i, (name, shown)) in st.layouts.iter().enumerate() {
                divider(ui);
                let active = st.layout_active.as_deref() == Some(name.as_str());
                let title = if active { format!("{} {name}", icon::CHECK_CIRCLE) } else { name.clone() };
                let sub = format!("{shown} screen(s) shown{}", if active { " · active" } else { "" });
                let mut follow = st.layout_follow.get(i).copied().unwrap_or(false);
                let follow_before = follow;
                setting_row(ui, &title, Some(&sub), |ui| {
                    let armed = st.layout_delete_arm.is_some_and(|(j, _)| j == i);
                    if icon_button(ui, icon::TRASH, if armed { "Tap again to delete" } else { "Delete" }, armed).clicked() {
                        if armed {
                            delete = Some(i);
                        } else {
                            arm = Some(i);
                        }
                    }
                    if icon_button(ui, icon::FLOPPY_DISK, "Save current arrangement over this layout", false).clicked() {
                        overwrite = Some(i);
                    }
                    if icon_button(ui, icon::PENCIL_SIMPLE, "Rename", false).clicked() {
                        rename = Some(i);
                    }
                    ui.add_enabled_ui(i + 1 < n_layouts, |ui| {
                        if icon_button(ui, icon::CARET_DOWN, "Move down", false).clicked() {
                            mv = Some((i, 1));
                        }
                    });
                    ui.add_enabled_ui(i > 0, |ui| {
                        if icon_button(ui, icon::CARET_UP, "Move up", false).clicked() {
                            mv = Some((i, -1));
                        }
                    });
                    if icon_button(ui, icon::PLAY, "Apply", false).clicked() {
                        apply = Some(i);
                    }
                    ui.add_space(10.0);
                    seg_toggle(ui, &mut follow);
                    ui.label(egui::RichText::new("follows head").size(12.0).color(theme::ON_SURFACE_VAR))
                        .on_hover_text("Double-B off/on re-centres this layout on your head, like unsaved arrangements");
                });
                if follow != follow_before {
                    st.layout_follow_toggle = Some((i, follow));
                }
            }
            if let Some(i) = rename {
                st.layout_rename = Some(i);
                st.naming = true;
                st.naming_layout = true;
                st.name_buf = st.layouts[i].0.clone();
                st.keyboard_open = true;
                st.sound_tab = true;
            }
            if let Some(m) = mv {
                st.layout_move = Some(m);
                st.sound_tab = true;
            }
            if st.layouts.is_empty() {
                ui.label(egui::RichText::new("No layouts yet.").color(theme::ON_SURFACE_VAR));
            }
            if let Some(i) = arm {
                st.layout_delete_arm = Some((i, now));
            }
            if let Some(i) = delete {
                st.layout_delete = Some(i);
                st.layout_delete_arm = None;
                st.sound_tab = true;
            }
            if let Some(i) = overwrite {
                st.layout_overwrite = Some(i);
                st.sound_tab = true;
            }
            if let Some(i) = apply {
                st.layout_apply = Some(i);
                st.sound_tab = true;
            }
            divider(ui);
            let mut t = false;
            setting_row(ui, "Restore last layout on start", Some("Bring the screens back where they were"), |ui| {
                t = seg_toggle(ui, &mut st.restore_layout);
            });
            if st.restore_layout {
                divider(ui);
                setting_row(ui, "Start hidden", Some("Loaded but out of sight: nothing on screen until a double-B (left hand) brings it up"), |ui| {
                    t |= seg_toggle(ui, &mut st.restore_layout_hidden);
                });
            }
            if t {
                st.sound_tab = true;
            }
        });
        ui.add_space(6.0);
        section(ui, "Behaviour", |ui| {
            let mut t = false;
            setting_row(ui, "Pause capture when not looking", Some("Frees GPU/CPU after ~2 s out of view; resumes instantly"), |ui| {
                t |= seg_toggle(ui, &mut st.gaze_pause);
            });
            divider(ui);
            setting_row(
                ui,
                "Double-B restore follows your head",
                Some("One screen or a docked group comes back centred in view; several loose screens keep their place around you — except an untouched loaded layout"),
                |ui| {
                    t |= seg_toggle(ui, &mut st.recenter_on_toggle);
                },
            );
            divider(ui);
            setting_row(ui, "Tilt restored screens to match headset angle", Some("Off = upright, like the menu's own toggle"), |ui| {
                t |= seg_toggle(ui, &mut st.screen_restore_tilt);
            });
            divider(ui);
            setting_row(ui, "Docking", Some("Drop a screen next to another to dock them edge-to-edge (they then move as one) · B while gripping detaches it"), |_| {});
            divider(ui);
            setting_row(
                ui,
                "Keyboard follows screens & text fields",
                Some("Hides with its docked screen and returns with it; pops up under the last-used screen when a text field gets focus on the desktop (accessibility bus)"),
                |ui| {
                    t |= seg_toggle(ui, &mut st.keyboard_auto);
                },
            );
            divider(ui);
            let mut fps = st.capture_max_fps as f32;
            setting_row(ui, "Capture frame-rate cap", Some("Frames above the headset rate are never seen; capping saves compositor GPU work (0 = unlimited)"), |ui| {
                stepper_inline(ui, &mut fps, 0.0, 240.0, 30.0, |v| if v < 1.0 { "unlimited".into() } else { format!("{v:.0} fps") });
            });
            st.capture_max_fps = fps.round() as u32;
            divider(ui);
            let mut mh = st.capture_max_height as f32;
            setting_row(ui, "Screen resolution cap", Some("Downscale mirrored screens in VR (0 = native)"), |ui| {
                stepper_inline(ui, &mut mh, 0.0, 2160.0, 360.0, |v| if v < 1.0 { "native".into() } else { format!("{v:.0} px tall") });
            });
            st.capture_max_height = mh.round() as u32;
            divider(ui);
            setting_row(ui, "Keyboard size", Some("Also saved in layouts"), |ui| {
                stepper_inline(ui, &mut st.keyboard_scale, 0.6, 1.6, 0.1, |v| format!("{:.0}%", v * 100.0));
            });
            divider(ui);
            setting_row(ui, "Scroll speed", Some("Thumbstick scrolling on a screen"), |ui| {
                modern_slider(ui, &mut st.scroll_speed, 0.25..=4.0, 300.0, |v| format!("{v:.2}×"));
            });
            divider(ui);
            setting_row(ui, "Drag threshold", Some("Trigger-held cursor motion below this stays a click; beyond it, it's a drag"), |ui| {
                modern_slider(ui, &mut st.drag_threshold_px, 0.0..=60.0, 300.0, |v| format!("{v:.0} px"));
            });
            if t {
                st.sound_tab = true;
            }
        });
        ui.add_space(6.0);
        section(ui, "Placement", |ui| {
            setting_row(
                ui,
                "Default screen width",
                Some("Applies to all screens · while gripping: trigger + push/pull resizes, stick pushes it away/closer, trigger + stick ◀▶ curves it"),
                |ui| {
                stepper_inline(ui, &mut st.screen_width_m, 0.6, 3.0, 0.1, |v| format!("{v:.1} m"));
            });
            divider(ui);
            setting_row(ui, "Mouse", Some("Trigger clicks & drags · A right-clicks · B clicks without moving (for tricky targets) · stick scrolls · double-B on the left hand hides/restores all screens + keyboard"), |_| {});
        });
        ui.add_space(6.0);
        section(ui, "Status", |ui| {
            let cap = if st.desktop_dmabuf { "GPU zero-copy (DMA-BUF)" } else { "CPU copy (SHM) — slower" };
            setting_row(ui, "Capture path", Some(cap), |_| {});
            divider(ui);
            match &st.desktop_hid_error {
                None => setting_row(ui, "Mouse & keyboard", Some("Virtual input device ready (uinput)"), |_| {}),
                Some(e) => setting_row(
                    ui,
                    "Mouse & keyboard unavailable",
                    Some(&format!("{e} — add yourself to the `input` group and re-login")),
                    |_| {},
                ),
            }
            divider(ui);
            setting_row(ui, "Shown", Some(&format!("{} screen(s) in VR", st.desktop_shown)), |_| {});
            divider(ui);
            setting_row(
                ui,
                "Keyboard layout",
                Some(&format!("{} · tap a modifier then a key · grip to move · dock under a screen", st.keyboard_layout)),
                |_| {},
            );
        });
    });
}

fn settings_view(ui: &mut egui::Ui, st: &mut LibState) {
    page_header(ui, icon::GEAR, "Settings");
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        section(ui, "Wrist watch", |ui| {
            let mut t = false;
            setting_row(ui, "Show the watch", Some("Clock, time zones, batteries and quick buttons on your left controller"), |ui| {
                t |= seg_toggle(ui, &mut st.watch_enabled);
            });
            divider(ui);
            setting_row(ui, "24-hour clock", Some("Also the bottom bar clock"), |ui| {
                t |= seg_toggle(ui, &mut st.watch_24h);
            });
            divider(ui);
            setting_row(ui, "Position locked", Some("Unlock, then grip the watch with the right hand to move it; the spot is remembered"), |ui| {
                t |= seg_toggle(ui, &mut st.watch_locked);
            });
            divider(ui);
            for slot in 0..4 {
                divider(ui);
                let id = st.watch_buttons.get(slot).cloned().unwrap_or_default();
                let (glyph, label) = watch_button_info(&id);
                setting_row(ui, &format!("Quick button {}", slot + 1), Some("Tap to cycle through the available actions"), |ui| {
                    if action_button(ui, glyph, label).clicked() {
                        st.watch_button_cycle = Some(slot);
                        st.sound_tab = true;
                    }
                });
            }
            divider(ui);
            setting_row(ui, "Reset position", Some("Back to the default wrist spot"), |ui| {
                if action_button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Reset").clicked() {
                    st.watch_reset_request = true;
                    st.sound_tab = true;
                }
            });
            // Extra time zones: up to two, cycled through a preset list.
            let zones = st.watch_zone_ids.clone();
            for (slot, id) in zones.iter().enumerate() {
                divider(ui);
                let now = st.watch_times.get(slot).map(|(_, t)| t.clone()).unwrap_or_default();
                let sub = if now.is_empty() { id.clone() } else { format!("{id} · {now} now") };
                setting_row(ui, &format!("Time zone {}", slot + 1), Some(&sub), |ui| {
                    if icon_button(ui, icon::TRASH, "Remove this zone", false).clicked() {
                        st.watch_zone_remove = Some(slot);
                    }
                    ui.add_space(6.0);
                    if icon_button(ui, icon::CARET_RIGHT, "Next zone", false).clicked() {
                        st.watch_zone_cycle = Some((slot, 1));
                    }
                    if icon_button(ui, icon::CARET_LEFT, "Previous zone", false).clicked() {
                        st.watch_zone_cycle = Some((slot, -1));
                    }
                });
            }
            if zones.len() < 2 {
                divider(ui);
                setting_row(ui, "Extra time zone", Some("Shown under the clock on the watch · up to two"), |ui| {
                    if action_button(ui, icon::PLUS, "Add zone").clicked() {
                        st.watch_zone_add = true;
                    }
                });
            }
            if t {
                st.sound_tab = true;
            }
        });
        section(ui, "Notifications", |ui| {
            let mut t = false;
            let st_desk = if st.notif_dbus_ok { "listening" } else { "unavailable" };
            setting_row(ui, "Desktop notifications", Some(&format!("Mirror what your desktop shows (D-Bus monitor) · {st_desk}")), |ui| {
                t |= seg_toggle(ui, &mut st.notif_enabled);
            });
            divider(ui);
            let st_xso = if st.notif_udp_ok { "listening on udp/42069" } else { "port busy (WayVR/XSOverlay?) — restart the overlay" };
            setting_row(ui, "XSOverlay notifications", Some(&format!("VRCX and friends · {st_xso}")), |ui| {
                t |= seg_toggle(ui, &mut st.notif_xso);
            });
            divider(ui);
            setting_row(ui, "Sound", Some("A soft two-note ding with each notification"), |ui| {
                t |= seg_toggle(ui, &mut st.notif_sound);
            });
            divider(ui);
            setting_row(ui, "Notification volume", Some("On top of the UI volume · 0 = muted"), |ui| {
                stepper_inline(ui, &mut st.notif_volume, 0.0, 1.0, 0.1, |v| if v < 0.05 { "muted".into() } else { format!("{:.0}%", v * 100.0) });
            });
            divider(ui);
            setting_row(ui, "Test", Some("Show a sample notification"), |ui| {
                if action_button(ui, icon::BELL, "Test").clicked() {
                    st.notif_test_request = true;
                }
            });
            if t {
                st.sound_tab = true;
            }
        });
        section(ui, "Background", |ui| {
            let mut t = false;
            setting_row(ui, "360° background when no game runs", Some(&format!("Image: {}", st.skybox_source)), |ui| {
                t |= seg_toggle(ui, &mut st.skybox_enabled);
            });
            divider(ui);
            setting_row(
                ui,
                "Custom panorama",
                Some(&format!("Drop an equirectangular JPEG/PNG (2:1) at {} and reload", st.skybox_custom_hint)),
                |ui| {
                    if action_button(ui, icon::ARROW_CLOCKWISE, "Reload").clicked() {
                        st.skybox_reload_request = true;
                    }
                },
            );
            if t {
                st.sound_tab = true;
            }
        });
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
                    st.flash("Panel placement reset");
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
            divider(ui);
            setting_row(ui, "Controls", Some("Every gesture the overlay understands"), |ui| {
                let (glyph, label) = if st.controls_open { (icon::CARET_UP, "Hide") } else { (icon::QUESTION, "Help") };
                if action_button(ui, glyph, label).clicked() {
                    st.controls_open = !st.controls_open;
                    st.sound_tab = true;
                }
            });
            if st.controls_open {
                controls_card(ui);
            }
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
    painter.rect_filled(rect, egui::CornerRadius::same(8), egui::Color32::from_rgb(12, 14, 18));
    painter.image(tex.id(), rect, cover_uv(tex.size(), rect), egui::Color32::WHITE);
}

/// The UV sub-rect that center-crops a `tw x th` texture to fill `rect` without
/// distortion (object-fit: cover).
fn cover_uv(tex_size: [usize; 2], rect: egui::Rect) -> egui::Rect {
    let [tw, th] = tex_size;
    let img_aspect = tw as f32 / th.max(1) as f32;
    let tile_aspect = rect.width() / rect.height().max(1.0);
    if img_aspect > tile_aspect {
        let keep = tile_aspect / img_aspect;
        let x0 = (1.0 - keep) * 0.5;
        egui::Rect::from_min_max(egui::pos2(x0, 0.0), egui::pos2(x0 + keep, 1.0))
    } else {
        let keep = img_aspect / tile_aspect;
        let y0 = (1.0 - keep) * 0.5;
        egui::Rect::from_min_max(egui::pos2(0.0, y0), egui::pos2(1.0, y0 + keep))
    }
}

/// A distinct, name-seeded tint for the launch popup's gradient fallback (used
/// when a game has no hero art), so each game still gets a recognisable colour.
fn hero_tint(name: &str) -> egui::Color32 {
    const PALETTE: [egui::Color32; 6] = [
        egui::Color32::from_rgb(33, 44, 62),
        egui::Color32::from_rgb(46, 33, 58),
        egui::Color32::from_rgb(26, 50, 46),
        egui::Color32::from_rgb(54, 40, 28),
        egui::Color32::from_rgb(56, 31, 42),
        egui::Color32::from_rgb(33, 48, 35),
    ];
    PALETTE[name_hash(name) as usize % PALETTE.len()]
}

fn short(name: &str) -> String {
    if name.chars().count() > 28 {
        format!("{}…", name.chars().take(27).collect::<String>())
    } else {
        name.to_string()
    }
}
