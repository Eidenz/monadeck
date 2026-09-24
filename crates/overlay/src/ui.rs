// The in-headset library UI. Art is lazy: each view records which tiles are
// on-screen (`visible_now`) and which game is selected; the render loop decodes
// only those. Drawing reads each game's `ArtState` (Ready / loading / Missing).
use egui_phosphor::regular as icon;

use crate::games::{ArtState, LibGame};
use crate::gfx::theme;

// The tool pages (Settings, System, Desktop, Photos): a category list on the
// left, cards on the right, all built from the shared `kit`.
mod desktop_page;
mod kit;
mod photos_page;
mod settings_page;
mod system_page;
pub use desktop_page::DesktopTab;
pub use photos_page::PhotosTab;
pub use settings_page::SettingsTab;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    // Minimal (clock-only) watch, typing haptics, OSC control.
    pub watch_mini: bool,
    /// Watch on the right wrist (the left hand then points at it).
    pub watch_right_hand: bool,
    pub keyboard_haptics: bool,
    // --- gaming mode (see gamemode.rs) ---
    pub game_mode: bool,
    pub game_mode_request: Option<bool>,
    pub game_dock: crate::desktop::DockMode,
    pub game_dock_request: Option<crate::desktop::DockMode>,
    pub game_pointer: [bool; 2],
    pub game_pointer_request: bool, // toggle the mouse for the hand tapping the watch
    pub game_guide_request: bool,
    pub game_profile: String,
    pub game_profiles: Vec<String>,
    pub game_profile_select: Option<usize>,
    pub game_profiles_reload: bool,
    pub game_rumble: bool,
    pub game_hide_pad: bool,       // keep the virtual pad out of VR games (protonfixes local fix)
    pub game_protonfixes_ok: bool, // a GE-style Proton is installed (the fix is only read by those)
    pub game_pad_ok: bool,
    pub game_pad_error: Option<String>,
    pub game_handheld_width: f32,
    pub watch_game_menu: bool, // clock card shows the profile picker
    pub osc_enabled: bool,
    /// Port as a float for the stepper (1024..=65535).
    pub osc_port: f32,
    /// The listener is bound (else its port is busy).
    pub osc_ok: bool,
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
    /// Switched-off controllers hold their last pose (`Some(true)`, the fork's
    /// default) or report untracked; `None` when the runtime lacks the switch.
    /// Live service state, mirrored each frame.
    pub hold_pose: Option<bool>,
    pub hold_pose_request: Option<bool>,
    /// The Freeze / Let go choice to keep (persisted, re-applied whenever
    /// Monado comes up).
    pub hold_pose_pref: bool,
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
    /// What new screens start with: curve (0..1), opacity, distance (m).
    pub screen_curve: f32,
    pub screen_opacity: f32,
    pub screen_spawn_dist: f32,
    /// Every screen's brightness (0.2..1) and warm tint (0..1).
    pub screen_brightness: f32,
    pub screen_warmth: f32,
    /// The runtime can scale layer colours (opacity, brightness, tint).
    pub desktop_color_scale: bool,
    /// B on a screen sends a middle click (else a cursor-frozen left click).
    pub mouse_b_middle: bool,
    /// The open category of each tool page.
    pub settings_tab: SettingsTab,
    pub desktop_tab: DesktopTab,
    pub photos_tab: PhotosTab,
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
            watch_mini: false,
            watch_right_hand: false,
            keyboard_haptics: true,
            game_mode: false,
            game_mode_request: None,
            game_dock: crate::desktop::DockMode::World,
            game_dock_request: None,
            game_pointer: [false; 2],
            game_pointer_request: false,
            game_guide_request: false,
            game_profile: "Xbox".into(),
            game_profiles: vec!["Xbox".into()],
            game_profile_select: None,
            game_profiles_reload: false,
            game_rumble: true,
            game_hide_pad: true,
            game_protonfixes_ok: true,
            game_pad_ok: false,
            game_pad_error: None,
            game_handheld_width: 0.6,
            watch_game_menu: false,
            osc_enabled: false,
            osc_port: 9001.0,
            osc_ok: false,
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
            hold_pose: None,
            hold_pose_request: None,
            hold_pose_pref: true,
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
            screen_curve: 0.0,
            screen_opacity: 1.0,
            screen_spawn_dist: 1.6,
            screen_brightness: 1.0,
            screen_warmth: 0.0,
            desktop_color_scale: false,
            mouse_b_middle: false,
            settings_tab: SettingsTab::Dashboard,
            desktop_tab: DesktopTab::Screens,
            photos_tab: PhotosTab::Gallery,
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
    // Kit widgets animate their own hover.
    st.no_glow.extend(kit::take_kit_hovered(ctx));
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
/// The name under a tile.
const TILE_CAPTION_H: f32 = 32.0;

/// The main (centre) panel: search bar, the active view (or active-game splash),
/// the on-screen keyboard, and the launching/fade overlays.
pub fn build_main(ctx: &egui::Context, st: &mut LibState) {
    // The game search only belongs on the game pages.
    let searchable = !st.show_splash && !matches!(st.nav, Nav::Settings | Nav::System | Nav::Desktop | Nav::Photos);
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

/// The left floating nav rail (its own composition layer): the game pages on
/// top, the tool pages pinned to the bottom, each a glyph with its name.
pub fn build_rail(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(18, 22, 28))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(12)))
        .corner_radius(26)
        .inner_margin(egui::Margin::symmetric(8, 14));
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        let mut go = |ui: &mut egui::Ui, glyph: &str, label: &str, nav: Nav| {
            let active = st.nav == nav && !st.show_splash;
            if rail_item(ui, glyph, label, active).clicked() && !active {
                st.nav = nav;
                st.show_splash = false;
                st.sound_tab = true;
            }
        };
        go(ui, icon::HOUSE, "Home", Nav::Home);
        go(ui, icon::SQUARES_FOUR, "Library", Nav::Library);
        go(ui, icon::STAR, "Favorites", Nav::Favorites);
        // The tool pages pinned to the bottom (4 items + their spacing).
        let reserve = 4.0 * RAIL_ITEM_H + 3.0 * 6.0;
        ui.add_space((ui.available_height() - reserve).max(0.0));
        go(ui, icon::WRENCH, "System", Nav::System);
        go(ui, icon::MONITOR, "Desktop", Nav::Desktop);
        go(ui, icon::IMAGES, "Photos", Nav::Photos);
        go(ui, icon::GEAR, "Settings", Nav::Settings);
    });
}

const RAIL_ITEM_H: f32 = 72.0;

fn rail_item(ui: &mut egui::Ui, glyph: &str, label: &str, active: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), RAIL_ITEM_H), egui::Sense::click());
    let h = kit::hover_t(ui, &resp);
    let on = ui.ctx().animate_bool_with_time(resp.id.with("on"), active, 0.16);
    let p = ui.painter();
    p.rect_filled(rect, egui::CornerRadius::same(18), kit::mix(kit::alpha(egui::Color32::WHITE, 0.05 * h), kit::alpha(theme::PRIMARY, 0.16), on));
    if on > 0.01 {
        let bar = egui::Rect::from_min_size(egui::pos2(rect.left() - 4.0, rect.center().y - 14.0), egui::vec2(4.0, 28.0));
        p.rect_filled(bar, egui::CornerRadius::same(2), kit::alpha(theme::PRIMARY, on));
    }
    let glyph_fg = kit::mix(kit::mix(theme::ON_SURFACE_VAR, egui::Color32::WHITE, h), theme::PRIMARY, on);
    let label_fg = kit::mix(kit::mix(theme::ON_SURFACE_VAR, egui::Color32::WHITE, h), egui::Color32::WHITE, on);
    p.text(egui::pos2(rect.center().x, rect.center().y - 9.0), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(25.0), glyph_fg);
    p.text(egui::pos2(rect.center().x, rect.center().y + 19.0), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(12.0), label_fg);
    resp
}

fn top_bar(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default().fill(theme::SURFACE).inner_margin(egui::Margin { left: 24, right: 24, top: 16, bottom: 2 });
    egui::TopBottomPanel::top("search").exact_height(70.0).show_separator_line(false).frame(frame).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            let (kbd_w, h) = (52.0, 52.0);
            let w = ui.available_width() - kbd_w - 12.0;
            let (rect, field) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
            let open = ui.ctx().animate_bool_with_time(field.id.with("open"), st.keyboard_open, 0.16);
            let p = ui.painter();
            let radius = egui::CornerRadius::same((h / 2.0) as u8);
            p.rect_filled(rect, radius, theme::SURFACE_CONTAINER);
            p.rect_stroke(rect, radius, egui::Stroke::new(1.0 + open, kit::mix(kit::alpha(egui::Color32::WHITE, 0.07), kit::alpha(theme::PRIMARY, 0.7), open)), egui::StrokeKind::Inside);
            p.text(egui::pos2(rect.left() + 26.0, rect.center().y), egui::Align2::CENTER_CENTER, icon::MAGNIFYING_GLASS, egui::FontId::proportional(20.0), kit::mix(theme::ON_SURFACE_VAR, theme::PRIMARY, open));
            let searching = !st.search.is_empty();
            let right_pad = if searching { 150.0 } else { 20.0 };
            // The text row, centred in the field (a TextEdit sits at the top of its rect).
            let inner = egui::Rect::from_min_max(egui::pos2(rect.left() + 52.0, rect.center().y - 12.0), egui::pos2(rect.right() - right_pad, rect.center().y + 14.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
            let resp = child.add_sized(
                inner.size(),
                egui::TextEdit::singleline(&mut st.search).hint_text("Search your games").frame(false).font(egui::FontId::proportional(18.0)),
            );
            if resp.clicked() || resp.gained_focus() || field.clicked() {
                st.keyboard_open = true;
            }
            if searching {
                // "N found" and a clear button at the field's right end.
                let n = filtered(st).len();
                let clear = egui::Rect::from_center_size(egui::pos2(rect.right() - 28.0, rect.center().y), egui::vec2(36.0, 36.0));
                ui.painter().text(egui::pos2(clear.left() - 12.0, rect.center().y), egui::Align2::RIGHT_CENTER, format!("{n} found"), egui::FontId::proportional(14.0), theme::ON_SURFACE_VAR);
                let c = ui.interact(clear, field.id.with("clear"), egui::Sense::click());
                let ch = kit::hover_t(ui, &c);
                ui.painter().circle_filled(clear.center(), 16.0, kit::mix(theme::SURFACE_CONTAINER_HIGH, egui::Color32::from_rgb(56, 66, 78), ch));
                ui.painter().text(clear.center(), egui::Align2::CENTER_CENTER, icon::X, egui::FontId::proportional(15.0), kit::mix(theme::ON_SURFACE_VAR, egui::Color32::WHITE, ch));
                if c.on_hover_text("Clear the search").clicked() {
                    st.search.clear();
                    st.sound_tab = true;
                }
            }
            ui.add_space(12.0);
            let (kr, kresp) = ui.allocate_exact_size(egui::vec2(kbd_w, h), egui::Sense::click());
            let kh = kit::hover_t(ui, &kresp);
            let p = ui.painter();
            let fill = kit::mix(kit::mix(theme::SURFACE_CONTAINER, egui::Color32::from_rgb(44, 54, 64), kh), theme::PRIMARY, open);
            p.rect_filled(kr, egui::CornerRadius::same(16), fill);
            p.text(kr.center(), egui::Align2::CENTER_CENTER, icon::KEYBOARD, egui::FontId::proportional(21.0), kit::mix(kit::mix(theme::ON_SURFACE_VAR, egui::Color32::WHITE, kh), egui::Color32::BLACK, open));
            if kresp.on_hover_text("Keyboard").clicked() {
                st.keyboard_open = !st.keyboard_open;
                st.sound_tab = true;
            }
        });
    });
}

/// The wrist watch (its own layer on a controller, left by default): batteries, clock +
/// extra time zones, quick buttons, and the menu + screen toggles like WayVR.
pub fn build_watch(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgba_unmultiplied(16, 20, 26, 242))
        .corner_radius(24)
        .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(64, 224, 208, 70)))
        .inner_margin(egui::Margin::same(10));
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        // Batteries on the left; the timer (when one runs) and the position
        // lock on the right. The batteries get what's left: controllers one
        // by one and one chip per other kind, or one chip per kind when even
        // that won't fit.
        ui.horizontal(|ui| {
            ui.set_height(28.0);
            let timer = (st.timer_running || st.timer_paused).then(|| {
                let rem = st.timer_remaining;
                if rem >= 3600 {
                    format!("{} {}:{:02}:{:02}", icon::TIMER, rem / 3600, (rem / 60) % 60, rem % 60)
                } else {
                    format!("{} {}:{:02}", icon::TIMER, rem / 60, rem % 60)
                }
            });
            let measure = |ui: &egui::Ui, text: String| ui.fonts(|f| f.layout_no_wrap(text, egui::FontId::proportional(13.5), egui::Color32::WHITE).size().x);
            let timer_w = timer.as_ref().map_or(0.0, |t| measure(ui, t.clone()) + 26.0 + 6.0);
            let avail = (ui.available_width() - 34.0 - timer_w - 10.0).max(0.0);
            // Chip sets from most to least detailed; the first that fits wins
            // (else the last, cut to what fits).
            use crate::monado::{BatteryInfo, BatteryKind};
            let of_kind = |k: BatteryKind| -> Vec<&BatteryInfo> { st.batteries.iter().filter(|b| b.kind == k).collect() };
            let set = |split: bool, compact: bool| -> Vec<(String, egui::Color32, String)> {
                let mut out: Vec<(String, egui::Color32, String)> = Vec::new();
                if split {
                    out.extend(of_kind(BatteryKind::Controller).into_iter().map(|b| one_chip(b, compact)));
                } else {
                    out.extend(group_chip(BatteryKind::Controller, &of_kind(BatteryKind::Controller), compact));
                }
                for kind in [BatteryKind::Glove, BatteryKind::Tracker, BatteryKind::Other] {
                    out.extend(group_chip(kind, &of_kind(kind), compact));
                }
                out
            };
            let width = |ui: &egui::Ui, chips: &[(String, egui::Color32, String)]| chips.iter().map(|c| measure(ui, c.0.clone()) + BATTERY_PILL_PAD + 6.0).sum::<f32>();
            let mut chips = Vec::new();
            for (split, compact) in [(true, false), (false, false), (false, true)] {
                chips = set(split, compact);
                if width(ui, &chips) <= avail {
                    break;
                }
            }
            while !chips.is_empty() && width(ui, &chips) > avail {
                chips.pop();
            }
            ui.allocate_ui_with_layout(egui::vec2(avail, 28.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if st.batteries.is_empty() {
                    ui.label(egui::RichText::new("no batteries").size(12.0).color(theme::ON_SURFACE_VAR));
                }
                for (text, color, tip) in chips {
                    battery_pill(ui, text, color, &tip);
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (glyph, tip) = if st.watch_locked {
                    (icon::LOCK, "Position locked · tap to unlock, then grip the watch to move it")
                } else {
                    (icon::LOCK_OPEN, "Grip the watch with the other hand to move it · tap to lock")
                };
                if watch_btn(ui, egui::vec2(34.0, 28.0), glyph, 15.0, !st.watch_locked, tip) {
                    st.watch_locked = !st.watch_locked;
                    st.sound_tab = true;
                }
                // A running/paused timer: the time left; tap to open it.
                if let Some(t) = timer {
                    let accent = if st.timer_paused { FAV_GOLD } else { theme::PRIMARY };
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(timer_w - 6.0, 26.0), egui::Sense::click());
                    let h = kit::hover_t(ui, &resp);
                    ui.painter().rect_filled(r, egui::CornerRadius::same(13), kit::alpha(accent, 0.16 + 0.12 * h));
                    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, t, egui::FontId::proportional(13.5), accent);
                    if resp.on_hover_text(if st.timer_paused { "Timer paused · tap to open" } else { "Timer running · tap to open" }).clicked() {
                        st.watch_timer_request = true;
                        st.sound_tab = true;
                    }
                }
            });
        });
        // Clock + zones (or the layout picker) | quick buttons — each in its own
        // card. A queued screenshot takes the whole row (bigger preview).
        // A queued screenshot or the notification history takes the whole row.
        let wide = (st.wrist_shot.is_some() || st.watch_history_menu) && !st.watch_layout_menu && !st.watch_media_menu && !st.watch_game_menu;
        let row_w = ui.available_width();
        ui.horizontal(|ui| {
            watch_card(ui, |ui| {
                // (the row width includes the card's own margins — keep it inside)
                ui.set_width(if wide { row_w - 21.5 } else { 212.0 });
                ui.set_min_height(120.0);
                // Corner icons inside the clock card: music (toggles the player view)
                // and the notification bell (toggles the history). Drawn at fixed
                // rects so they never push the content around.
                {
                    let r = ui.max_rect();
                    let mut no_glow: Vec<egui::Rect> = Vec::new();
                    // Tinted glyphs only (no fill): music top-right, bell top-left.
                    // `on` = its view is open (tap again to close); `badge` = a
                    // small count bubble under the glyph.
                    // `slot`: 0 = leftmost, 1 = rightmost, 2 = second from the right.
                    let mut corner_btn = |ui: &mut egui::Ui, slot: u8, glyph: &str, on: bool, hot: bool, badge: usize, tip: &str| -> bool {
                        // The left edge sits a little into the margin so both glyphs
                        // end up the same distance from their card edge.
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
                            // Under the glyph (outside the button's rect), clear of the clock.
                            let c = egui::pos2(rect.center().x, rect.bottom() + 8.0);
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
                        // No count while the history itself is open (it's right there).
                        let badge = if st.watch_history_menu { 0 } else { st.notif_unseen };
                        if corner_btn(ui, 0, glyph, st.watch_history_menu, st.notif_unseen > 0, badge, "Recent notifications") {
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
                            // Below the corner icons.
                            ui.add_space(22.0);
                            let title: String = if m.title.chars().count() > 24 { format!("{}…", m.title.chars().take(23).collect::<String>()) } else { m.title.clone() };
                            ui.label(egui::RichText::new(if title.is_empty() { m.player.clone() } else { title }).size(16.0).strong().color(egui::Color32::WHITE));
                            let sub = if m.artist.is_empty() { m.player.clone() } else { format!("{} · {}", m.artist, m.player) };
                            let sub: String = if sub.chars().count() > 34 { format!("{}…", sub.chars().take(33).collect::<String>()) } else { sub };
                            ui.label(egui::RichText::new(sub).size(11.0).color(theme::ON_SURFACE_VAR));
                            ui.add_space(6.0);
                            let b = 44.0;
                            centered_row(ui, b * 3.0 + 12.0, |ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                let tb = |ui: &mut egui::Ui, g: &str, tip: &str| -> bool { watch_btn(ui, egui::vec2(b, 38.0), g, 18.0, false, tip) };
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
                        // Title, then the message (cut to fit), the age on the right.
                        let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::hover());
                        let age_g = ui.fonts(|f| f.layout_no_wrap(age.clone(), egui::FontId::proportional(11.0), theme::ON_SURFACE_VAR));
                        let title_g = kit::fit_text(ui, title, 13.0, egui::Color32::WHITE, r.width() * 0.4);
                        let body: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
                        let body_w = (r.width() - title_g.size().x - age_g.size().x - 20.0).max(20.0);
                        let body_g = kit::fit_text(ui, &body, 12.0, theme::ON_SURFACE_VAR, body_w);
                        let p = ui.painter();
                        let y = |g: &egui::Galley| r.center().y - g.size().y / 2.0;
                        let body_x = r.left() + title_g.size().x + 10.0;
                        p.galley(egui::pos2(body_x, y(&body_g)), body_g, theme::ON_SURFACE_VAR);
                        p.galley(egui::pos2(r.right() - age_g.size().x, y(&age_g)), age_g.clone(), theme::ON_SURFACE_VAR);
                        p.galley(egui::pos2(r.left(), y(&title_g)), title_g, egui::Color32::WHITE);
                    }
                } else if let (Some(shot), false) = (&st.wrist_shot, st.watch_layout_menu) {
                    let req = crate::photos::wrist_card(ui, shot.thumb.as_ref(), shot.qr.as_deref(), &shot.when, shot.idx, shot.total);
                    if req.open || req.dismiss || req.older || req.newer {
                        st.wrist_req = req;
                        st.sound_tab = true;
                    }
                } else if st.watch_game_menu {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{}  Gaming", icon::GAME_CONTROLLER)).size(14.0).strong().color(egui::Color32::WHITE));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if watch_btn(ui, egui::vec2(28.0, 24.0), icon::X, 13.0, false, "Close") {
                                st.watch_game_menu = false;
                            }
                            if watch_btn(ui, egui::vec2(58.0, 24.0), "Guide", 12.5, false, "Press the pad's Guide button") {
                                st.game_guide_request = true;
                                st.sound_tab = true;
                            }
                        });
                    });
                    let mut pick = None;
                    egui::ScrollArea::vertical().max_height(92.0).auto_shrink([false, true]).show(ui, |ui| {
                        for (i, name) in st.game_profiles.iter().enumerate() {
                            if watch_btn(ui, egui::vec2(ui.available_width(), 30.0), name, 14.0, st.game_profile == *name, name) {
                                pick = Some(i);
                            }
                        }
                    });
                    if let Some(i) = pick {
                        st.game_profile_select = Some(i);
                        st.watch_game_menu = false;
                        st.sound_tab = true;
                    }
                } else if st.watch_layout_menu {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{}  Layouts", icon::SQUARES_FOUR)).size(14.0).strong().color(egui::Color32::WHITE));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if watch_btn(ui, egui::vec2(28.0, 24.0), icon::X, 13.0, false, "Close") {
                                st.watch_layout_menu = false;
                            }
                        });
                    });
                    let mut apply = None;
                    egui::ScrollArea::vertical().max_height(92.0).auto_shrink([false, true]).show(ui, |ui| {
                        for (i, (name, _)) in st.layouts.iter().enumerate() {
                            let active = st.layout_active.as_deref() == Some(name.as_str());
                            if watch_btn(ui, egui::vec2(ui.available_width(), 30.0), name, 14.0, active, name) {
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
                        ui.add_space(-4.0);
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
                let quick = |ui: &mut egui::Ui, glyph: &str, on: bool, tip: &str| -> bool { watch_btn(ui, egui::vec2(b, b), glyph, 24.0, on, tip) };
                if st.game_mode {
                    watch_game_buttons(ui, st, &quick);
                } else {
                    let ids = st.watch_buttons.clone();
                    for row in ids.chunks(2) {
                        ui.horizontal(|ui| {
                            for id in row {
                                watch_quick_button(ui, st, id, &quick);
                            }
                        });
                    }
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
                if watch_btn(ui, egui::vec2(56.0, 42.0), icon::LIST, 22.0, false, "Monadeck menu") {
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
                    if watch_btn(ui, egui::vec2(56.0, 42.0), &format!("{} {}", icon::MONITOR, i + 1), 15.0, *shown, name) {
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
pub const WATCH_BUTTON_IDS: [&str; 11] = ["keyboard", "recenter", "layouts", "freeze", "letgo", "timer", "screenshot", "screens", "mute", "photos", "gaming"];

/// The gaming-mode watch: the quick buttons make way for what you need
/// mid-game — where the screen hangs, the mouse, remap profiles, and the way
/// out. (The clock card keeps the time and hosts the profile picker.)
fn watch_game_buttons(ui: &mut egui::Ui, st: &mut LibState, quick: &dyn Fn(&mut egui::Ui, &str, bool, &str) -> bool) {
    use crate::desktop::DockMode;
    let dock_glyph = match st.game_dock {
        DockMode::World => icon::GLOBE_HEMISPHERE_WEST,
        DockMode::Head => icon::EYE,
        DockMode::Handheld => icon::DEVICE_TABLET,
    };
    ui.horizontal(|ui| {
        let tip = format!("Screens: {} · tap for {}", st.game_dock.label(), st.game_dock.next().label());
        if quick(ui, dock_glyph, st.game_dock != DockMode::World, &tip) {
            st.game_dock_request = Some(st.game_dock.next());
            st.sound_tab = true;
        }
        let mouse_on = st.game_pointer.iter().any(|p| *p);
        if quick(ui, icon::CURSOR, mouse_on, "Mouse for this hand · point at a screen · off again once it leaves") {
            st.game_pointer_request = true;
            st.sound_tab = true;
        }
    });
    ui.horizontal(|ui| {
        let tip = format!("Remap profile · {}", st.game_profile);
        if quick(ui, icon::SLIDERS, st.watch_game_menu, &tip) {
            st.watch_game_menu = !st.watch_game_menu;
            st.watch_layout_menu = false;
            st.watch_history_menu = false;
            st.watch_media_menu = false;
            st.sound_tab = true;
        }
        if quick(ui, icon::SIGN_OUT, false, "Leave gaming mode") {
            st.game_mode_request = Some(false);
            st.sound_tab = true;
        }
    });
}

/// The minimal watch: a clock pill that takes the wrist spot when the full
/// watch is folded away, with one quiet line under the time (the timer when
/// one runs, else the date, and the lowest battery). `hot` = the other hand
/// points at it (a tap peeks at the full watch). Same skin as the watch.
pub fn build_watch_mini(ctx: &egui::Context, st: &LibState, hot: bool) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let rect = ctx.screen_rect().shrink(3.0);
    let radius = rect.height() / 2.0;
    kit::gradient_rect(&painter, rect, radius, egui::Color32::from_rgba_unmultiplied(24, 30, 38, 245), egui::Color32::from_rgba_unmultiplied(14, 18, 24, 245));
    let rim = if hot { theme::PRIMARY } else { egui::Color32::from_rgba_unmultiplied(64, 224, 208, 70) };
    painter.rect_stroke(rect, egui::CornerRadius::same(radius as u8), egui::Stroke::new(1.5, rim), egui::StrokeKind::Inside);
    painter.text(rect.center() - egui::vec2(0.0, rect.height() * 0.12), egui::Align2::CENTER_CENTER, &st.clock, egui::FontId::proportional(rect.height() * 0.44), egui::Color32::WHITE);

    // The line under the clock: timer or date · lowest live battery.
    let mut job = egui::text::LayoutJob::default();
    let font = egui::FontId::proportional(rect.height() * 0.15);
    let add = |job: &mut egui::text::LayoutJob, text: &str, color: egui::Color32| {
        job.append(text, 0.0, egui::TextFormat { font_id: font.clone(), color, ..Default::default() });
    };
    if st.timer_running || st.timer_paused {
        let rem = st.timer_remaining;
        let accent = if st.timer_paused { FAV_GOLD } else { theme::PRIMARY };
        add(&mut job, &format!("{} {}:{:02}", icon::TIMER, rem / 60, rem % 60), accent);
    } else {
        // "Thursday, September 24" → "Thu 24".
        let short = match st.watch_date.split_once(", ") {
            Some((day, rest)) => format!("{} {}", day.chars().take(3).collect::<String>(), rest.rsplit(' ').next().unwrap_or(rest)),
            None => st.watch_date.clone(),
        };
        add(&mut job, &short, theme::ON_SURFACE_VAR);
    }
    let lowest = st
        .batteries
        .iter()
        .filter(|b| b.state != crate::monado::DevState::Off && !b.charging)
        .min_by(|a, b| a.charge.total_cmp(&b.charge));
    if let Some(b) = lowest {
        add(&mut job, "   ", theme::ON_SURFACE_VAR);
        add(&mut job, &format!("{} {}%", battery_glyph(b), (b.charge * 100.0).round() as i32), battery_color(b));
    }
    let galley = ctx.fonts(|f| f.layout_job(job));
    painter.galley(egui::pos2(rect.center().x - galley.size().x / 2.0, rect.center().y + rect.height() * 0.2 - galley.size().y / 2.0), galley, theme::ON_SURFACE_VAR);
    // Unread notifications: a small dot by the rim, nothing more.
    if st.notif_unseen > 0 {
        painter.circle_filled(egui::pos2(rect.right() - radius * 0.55, rect.top() + radius * 0.45), 5.0, egui::Color32::from_rgb(150, 190, 255));
    }
}

pub fn watch_button_info(id: &str) -> (&'static str, &'static str) {
    match id {
        "keyboard" => (icon::KEYBOARD, "VR keyboard"),
        "recenter" => (icon::CROSSHAIR, "Recenter playspace"),
        "layouts" => (icon::SQUARES_FOUR, "Screen layouts"),
        "freeze" => (icon::SNOWFLAKE, "Freeze game controllers"),
        "letgo" => (icon::HAND_ARROW_DOWN, "Let go when off"),
        "timer" => (icon::TIMER, "Timer"),
        "screenshot" => (icon::CAMERA, "Take a screenshot"),
        "screens" => (icon::MONITOR, "Hide / restore all screens"),
        "mute" => (icon::BELL_SLASH, "Mute notifications"),
        "photos" => (icon::IMAGES, "Photos"),
        "gaming" => (icon::GAME_CONTROLLER, "Gaming mode"),
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
        "gaming" => {
            if quick(ui, glyph, st.game_mode, tip) {
                st.game_mode_request = Some(!st.game_mode);
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
        "letgo" => {
            // Lit while switched-off controllers are let go (reported off), the
            // exception to the default freeze-in-place.
            let enabled = st.hold_pose.is_some();
            let letting_go = st.hold_pose == Some(false);
            let tip = if letting_go {
                "Switched-off controllers are reported off · tap to freeze them in place again"
            } else {
                "Switched-off controllers freeze in place · tap to let go (the game sees them off)"
            };
            ui.add_enabled_ui(enabled, |ui| {
                if quick(ui, glyph, letting_go, tip) {
                    st.hold_pose_request = Some(letting_go);
                    st.sound_tab = true;
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

/// A watch button (painted, eases on hover): `lit` fills it with the accent.
/// Disabled ones fade. Returns true when tapped.
fn watch_btn(ui: &mut egui::Ui, size: egui::Vec2, text: &str, font: f32, lit: bool, tip: &str) -> bool {
    let enabled = ui.is_enabled();
    let (r, resp) = ui.allocate_exact_size(size, if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let h = if enabled { kit::hover_t(ui, &resp) } else { 0.0 };
    let on = ui.ctx().animate_bool_with_time(resp.id.with("on"), lit, 0.16);
    let fill = kit::mix(kit::mix(theme::SURFACE_CONTAINER_HIGH, egui::Color32::from_rgb(58, 68, 80), h), theme::PRIMARY, on);
    let fg = kit::mix(kit::mix(theme::ON_SURFACE, egui::Color32::WHITE, h), egui::Color32::BLACK, on);
    let p = ui.painter();
    let radius = egui::CornerRadius::same((size.y * 0.3).min(16.0) as u8);
    p.rect_filled(r, radius, if enabled { fill } else { kit::alpha(fill, 0.4) });
    let g = kit::fit_text(ui, text, font, if enabled { fg } else { kit::alpha(fg, 0.4) }, size.x - 12.0);
    ui.painter().galley(r.center() - g.size() / 2.0, g, fg);
    resp.on_hover_text(tip).clicked()
}

/// A card grouping one of the watch's areas.
fn watch_card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::default()
        .fill(theme::SURFACE_CONTAINER)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(14)))
        .corner_radius(16)
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            // Cards sit in a horizontal row; their contents stack vertically.
            ui.vertical(contents);
        });
}

/// The bottom floating bar (its own layer): recenter · the running game ·
/// the mirrored screens and keyboard (centred) · batteries · clock.
pub fn build_bottom(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(18, 22, 28))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(12)))
        .corner_radius(26)
        .inner_margin(egui::Margin::symmetric(16, 6));
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            // Recenter playspace: a round glyph that lights on hover.
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(48.0, 48.0), egui::Sense::click());
            let h = kit::hover_t(ui, &resp);
            ui.painter().circle_filled(rect.center(), 24.0, kit::mix(theme::SURFACE_CONTAINER, egui::Color32::from_rgb(44, 54, 64), h));
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon::CROSSHAIR, egui::FontId::proportional(22.0), kit::mix(theme::ON_SURFACE_VAR, egui::Color32::WHITE, h));
            if resp.on_hover_text("Recenter playspace").clicked() {
                st.recenter_playspace_request = true;
                st.sound_tab = true;
            }
            // The running game: opens its splash.
            if let Some(i) = st.running_index {
                let name = short(&st.games[i].name);
                let label = format!("{}  {name}", icon::GAME_CONTROLLER);
                let g = ui.fonts(|f| f.layout_no_wrap(label.clone(), egui::FontId::proportional(15.0), egui::Color32::WHITE));
                let (r, resp) = ui.allocate_exact_size(egui::vec2(g.size().x + 44.0, 48.0), egui::Sense::click());
                let h = kit::hover_t(ui, &resp);
                let on = ui.ctx().animate_bool_with_time(resp.id.with("on"), st.show_splash, 0.16);
                let p = ui.painter();
                p.rect_filled(r, egui::CornerRadius::same(24), kit::mix(kit::mix(theme::SURFACE_CONTAINER, egui::Color32::from_rgb(44, 54, 64), h), theme::PRIMARY, on));
                p.circle_filled(egui::pos2(r.left() + 16.0, r.center().y), 4.0, kit::mix(RUNNING_GREEN, egui::Color32::BLACK, on));
                p.text(egui::pos2(r.left() + 28.0, r.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(15.0), kit::mix(theme::ON_SURFACE, egui::Color32::BLACK, on));
                if resp.on_hover_text("The running game").clicked() {
                    st.show_splash = !st.show_splash;
                    st.sound_tab = true;
                }
            }
            // Mirrored screens + keyboard in one tray, centred in the bar (fixed
            // order: a screen is always in the same spot).
            let mut pills_right: Option<f32> = None;
            if !st.desktop_bar.is_empty() {
                let (pill_w, kb_w, gap, pad) = (68.0, 52.0, 6.0, 5.0);
                let count = st.desktop_bar.len() as f32;
                let total = count * pill_w + kb_w + count * gap + pad * 2.0 + 10.0;
                let tray = egui::Rect::from_center_size(ui.max_rect().center(), egui::vec2(total, 54.0));
                pills_right = Some(tray.right());
                ui.painter().rect_filled(tray, egui::CornerRadius::same(27), egui::Color32::from_rgb(24, 29, 36));
                let mut x = tray.left() + pad;
                let mut toggle = None;
                // `lead`: extra room before this pill (sets the keyboard apart).
                let mut pill = |ui: &mut egui::Ui, w: f32, lead: f32, key: usize, text: String, on: bool, tip: &str| -> bool {
                    x += lead;
                    let r = egui::Rect::from_min_size(egui::pos2(x, tray.top() + pad), egui::vec2(w, 44.0));
                    x += w + gap;
                    let resp = ui.interact(r, ui.id().with(("bar-pill", key)), egui::Sense::click());
                    let h = kit::hover_t(ui, &resp);
                    let lit = ui.ctx().animate_bool_with_time(resp.id.with("on"), on, 0.16);
                    let p = ui.painter();
                    p.rect_filled(r, egui::CornerRadius::same(22), kit::mix(kit::alpha(egui::Color32::WHITE, 0.06 * h), theme::PRIMARY, lit));
                    p.text(r.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(16.0), kit::mix(kit::mix(theme::ON_SURFACE_VAR, egui::Color32::WHITE, h), egui::Color32::BLACK, lit));
                    resp.on_hover_text(tip).clicked()
                };
                for (i, (name, shown)) in st.desktop_bar.iter().enumerate() {
                    // Numbered, not named: the number is the position, which never moves.
                    let tip = format!("{} · {}", name, if *shown { "hide" } else { "show" });
                    if pill(ui, pill_w, 0.0, i, format!("{}  {}", icon::MONITOR, i + 1), *shown, &tip) {
                        toggle = Some(i);
                    }
                }
                if pill(ui, kb_w, 10.0, 999, icon::KEYBOARD.to_string(), st.keyboard_shown, "VR keyboard") {
                    st.keyboard_toggle_request = true;
                    st.sound_tab = true;
                }
                if let Some(i) = toggle {
                    st.desktop_bar_toggle = Some(i);
                    st.sound_tab = true;
                }
            }
            // Clock + batteries on the right. The batteries only get the room
            // between the (fixed, centred) screen tray and the clock: a full-body
            // rig's worth of devices would otherwise run over it. They wrap onto a
            // second row first (every device stays readable), and only fold into
            // one chip per kind — the watch's trick — when two rows aren't enough.
            use crate::monado::{BatteryInfo, BatteryKind};
            enum Chip<'a> {
                One(&'a BatteryInfo),
                Group(BatteryKind, Vec<&'a BatteryInfo>),
            }
            let measure = |ui: &egui::Ui, text: String, size: f32| -> f32 {
                ui.fonts(|f| f.layout_no_wrap(text, egui::FontId::proportional(size), egui::Color32::WHITE).size().x)
            };
            let limit = pills_right.unwrap_or(0.0).max(ui.cursor().min.x) + 14.0;
            let clock_w = if st.clock.is_empty() { 0.0 } else { measure(ui, st.clock.clone(), 22.0) + 8.0 };
            let avail = (ui.max_rect().right() - limit - clock_w - 18.0).max(0.0);
            // Widest a chip gets ("100%", and "×N" for a group), with its padding and spacing.
            let w_single = measure(ui, format!("{} {} 100%", icon::GAME_CONTROLLER, icon::BATTERY_FULL), 13.5) + BATTERY_PILL_PAD + 8.0;
            let w_group = measure(ui, format!("{} {} 100% ×9", icon::GAME_CONTROLLER, icon::BATTERY_FULL), 13.5) + BATTERY_PILL_PAD + 8.0;
            let of_kind = |k: BatteryKind| -> Vec<&BatteryInfo> { st.batteries.iter().filter(|b| b.kind == k).collect() };
            let others = [BatteryKind::Glove, BatteryKind::Tracker, BatteryKind::Other];
            let groups = |kinds: &[BatteryKind]| -> Vec<Chip> {
                kinds.iter().filter_map(|k| Some(of_kind(*k)).filter(|g| !g.is_empty()).map(|g| Chip::Group(*k, g))).collect()
            };
            // Candidate layouts, most detailed first: (chips, widest chip).
            let candidates: [(Vec<Chip>, f32); 3] = [
                (st.batteries.iter().map(Chip::One).collect(), w_single),
                (of_kind(BatteryKind::Controller).into_iter().map(Chip::One).chain(groups(&others)).collect(), w_group),
                (groups(&[BatteryKind::Controller, BatteryKind::Glove, BatteryKind::Tracker, BatteryKind::Other]), w_group),
            ];
            let mut chosen: Option<(Vec<Chip>, usize)> = None; // (chips, chips per row)
            let mut last = None;
            for (chips, w) in candidates {
                let per_row = ((avail / w).floor() as usize).max(1);
                if chips.len() <= per_row * 2 {
                    chosen = Some((chips, per_row));
                    break;
                }
                last = Some((chips, per_row));
            }
            // Nothing fits even folded: show what two rows can hold.
            let (mut chips, per_row) = chosen.or(last).unwrap_or((Vec::new(), 1));
            chips.truncate(per_row * 2);
            let draw = |ui: &mut egui::Ui, chip: &Chip| match chip {
                Chip::One(b) => battery_widget(ui, b),
                Chip::Group(kind, group) => battery_group_widget(ui, *kind, group),
            };
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if !st.clock.is_empty() {
                    ui.label(egui::RichText::new(&st.clock).size(22.0).strong().color(egui::Color32::WHITE));
                }
                ui.add_space(14.0);
                if chips.len() <= per_row {
                    for chip in &chips {
                        draw(ui, chip);
                    }
                } else {
                    // Two rows, split evenly (the first devices stay top-right).
                    let top = chips.len().div_ceil(2);
                    let block_w = avail.min(top as f32 * w_single.max(w_group));
                    ui.allocate_ui_with_layout(egui::vec2(block_w, 60.0), egui::Layout::top_down(egui::Align::Max), |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        for row in [&chips[..top], &chips[top..]] {
                            ui.allocate_ui_with_layout(egui::vec2(block_w, 26.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                for chip in row {
                                    draw(ui, chip);
                                }
                            });
                        }
                    });
                }
            });
        });
    });
}

/// Colour for a battery chip: tinted by charge, faded while the device isn't
/// tracking, grey once it's switched off (SteamVR-style).
fn battery_color(b: &crate::monado::BatteryInfo) -> egui::Color32 {
    use crate::monado::DevState;
    let tint = if b.charge > 0.33 {
        RUNNING_GREEN
    } else if b.charge > 0.15 {
        FAV_GOLD
    } else {
        STOP_RED
    };
    match b.state {
        DevState::Live => tint,
        DevState::Lost => tint.gamma_multiply(0.45),
        DevState::Off => theme::ON_SURFACE_VAR.gamma_multiply(0.6),
    }
}

fn battery_state_note(b: &crate::monado::BatteryInfo) -> &'static str {
    match b.state {
        crate::monado::DevState::Live => "",
        crate::monado::DevState::Lost => " · not tracking",
        crate::monado::DevState::Off => " · switched off",
    }
}

/// One chip for a whole kind of device: the lowest charge in the group (tinted
/// by it), "×N" when several, and every member's charge on hover. Switched-off
/// members don't count towards the lowest; a group that's all off goes grey.
fn battery_group_widget(ui: &mut egui::Ui, kind: crate::monado::BatteryKind, group: &[&crate::monado::BatteryInfo]) {
    if let Some((text, color, tip)) = group_chip(kind, group, false) {
        battery_pill(ui, text, color, &tip);
    }
}

fn battery_widget(ui: &mut egui::Ui, b: &crate::monado::BatteryInfo) {
    let (text, color, tip) = one_chip(b, false);
    battery_pill(ui, text, color, &tip);
}

/// A device's chip: (text, colour, hover text). `compact` drops the battery
/// glyph (the watch, when room is short).
fn one_chip(b: &crate::monado::BatteryInfo, compact: bool) -> (String, egui::Color32, String) {
    use crate::monado::BatteryKind;
    let dev = match b.kind {
        BatteryKind::Glove => icon::HAND,
        BatteryKind::Controller => icon::GAME_CONTROLLER,
        _ => icon::CIRCLE,
    };
    // A switched-off device's charge is its last reading: show "off" instead.
    let text = if b.state == crate::monado::DevState::Off {
        format!("{dev}  off")
    } else if compact {
        format!("{dev} {}%", (b.charge * 100.0).round() as i32)
    } else {
        format!("{dev} {} {}%", battery_glyph(b), (b.charge * 100.0).round() as i32)
    };
    let what = match b.kind {
        BatteryKind::Glove => "Glove",
        BatteryKind::Controller => "Controller",
        BatteryKind::Tracker => "Tracker",
        BatteryKind::Other => "Device",
    };
    (text, battery_color(b), format!("{what}{}", battery_state_note(b)))
}

/// A kind's chip (see `battery_group_widget`); `None` for an empty group.
fn group_chip(kind: crate::monado::BatteryKind, group: &[&crate::monado::BatteryInfo], compact: bool) -> Option<(String, egui::Color32, String)> {
    use crate::monado::{BatteryKind, DevState};
    let on: Vec<&&crate::monado::BatteryInfo> = group.iter().filter(|b| b.state != DevState::Off).collect();
    let lowest = on.iter().filter(|b| !b.charging).min_by(|a, b| a.charge.total_cmp(&b.charge)).or(on.first()).map(|b| **b).or(group.first().copied())?;
    let all_off = on.is_empty();
    // Tinted by the lowest live member; faded only when none of them tracks.
    let color = if all_off {
        battery_color(lowest)
    } else {
        let mut c = battery_color(&crate::monado::BatteryInfo { state: DevState::Live, ..lowest.clone() });
        if on.iter().all(|b| b.state == DevState::Lost) {
            c = c.gamma_multiply(0.45);
        }
        c
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
        .map(|(i, b)| {
            if b.state == DevState::Off {
                format!("{name} {}: switched off", i + 1)
            } else {
                format!("{name} {}: {}%{}{}", i + 1, (b.charge * 100.0).round() as i32, if b.charging { " (charging)" } else { "" }, battery_state_note(b))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let pct = (lowest.charge * 100.0).round() as i32;
    let text = if all_off {
        format!("{dev}  off{count}")
    } else if compact {
        format!("{dev} {pct}%{count}")
    } else {
        format!("{dev} {} {pct}%{count}", battery_glyph(lowest))
    };
    Some((text, color, format!("Lowest of {}:\n{tip}", name.to_lowercase())))
}

fn battery_glyph(b: &crate::monado::BatteryInfo) -> &'static str {
    if b.charging {
        icon::BATTERY_CHARGING
    } else if b.charge > 0.66 {
        icon::BATTERY_FULL
    } else if b.charge > 0.33 {
        icon::BATTERY_MEDIUM
    } else if b.charge > 0.1 {
        icon::BATTERY_LOW
    } else {
        icon::BATTERY_WARNING
    }
}

/// Horizontal room a battery pill takes around its text (padding + spacing).
const BATTERY_PILL_PAD: f32 = 20.0;

/// A battery reading as a softly tinted pill.
fn battery_pill(ui: &mut egui::Ui, text: String, color: egui::Color32, tip: &str) {
    let g = ui.fonts(|f| f.layout_no_wrap(text, egui::FontId::proportional(13.5), color));
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(g.size().x + BATTERY_PILL_PAD, 26.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, egui::CornerRadius::same(13), kit::alpha(color, 0.13));
    p.galley(egui::pos2(rect.left() + BATTERY_PILL_PAD / 2.0, rect.center().y - g.size().y / 2.0), g, color);
    resp.on_hover_text(tip);
}

// --- on-panel virtual keyboard ----------------------------------------------

fn keyboard(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(18, 22, 28))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(12)))
        .inner_margin(egui::Margin::symmetric(14, 14));
    let naming = st.naming;
    egui::TopBottomPanel::bottom("keyboard").show_separator_line(false).frame(frame).show(ctx, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        if naming {
            ui.horizontal(|ui| {
                let what = if st.layout_rename.is_some() {
                    "RENAME LAYOUT"
                } else if st.naming_layout {
                    "NEW LAYOUT"
                } else {
                    "NEW COLLECTION"
                };
                ui.add_space(8.0);
                ui.label(egui::RichText::new(what).size(12.5).strong().color(theme::PRIMARY));
                ui.add_space(8.0);
                let shown = if st.name_buf.is_empty() { "…" } else { st.name_buf.as_str() };
                ui.label(egui::RichText::new(shown).size(20.0).strong().color(egui::Color32::WHITE));
            });
            ui.add_space(2.0);
        }
        for row in ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"] {
            key_row(ui, row, if naming { &mut st.name_buf } else { &mut st.search });
        }
        let sp = 8.0;
        let total = 110.0 + 260.0 + 110.0 + 140.0 + 3.0 * sp + if naming { 110.0 + sp } else { 0.0 };
        let pad = ((ui.available_width() - total) * 0.5).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.spacing_mut().item_spacing.x = sp;
            if naming && fkey(ui, "Cancel", 110.0, false).clicked() {
                st.name_buf.clear();
                st.naming = false;
                st.naming_layout = false;
                st.layout_rename = None;
                st.keyboard_open = false;
            }
            if fkey(ui, &format!("{}  Back", icon::BACKSPACE), 110.0, false).clicked() {
                if naming { st.name_buf.pop(); } else { st.search.pop(); }
            }
            if fkey(ui, "Space", 260.0, false).clicked() {
                if naming { st.name_buf.push(' '); } else { st.search.push(' '); }
            }
            if fkey(ui, "Clear", 110.0, false).clicked() {
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
            let commit_resp = ui.add_enabled_ui(can_commit, |ui| fkey(ui, commit, 140.0, true)).inner;
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
    });
}

fn key_row(ui: &mut egui::Ui, chars: &str, target: &mut String) {
    let (kw, sp) = (48.0, 8.0);
    let n = chars.chars().count() as f32;
    let total = n * kw + (n - 1.0).max(0.0) * sp;
    let pad = ((ui.available_width() - total) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(pad);
        ui.spacing_mut().item_spacing.x = sp;
        for ch in chars.chars() {
            if fkey(ui, &ch.to_string(), kw, false).clicked() {
                target.push(ch);
            }
        }
    });
}

/// A key of the on-panel keyboard (painted, so it eases on hover); `accent`
/// fills it with the brand colour (Done / Create).
fn fkey(ui: &mut egui::Ui, label: &str, w: f32, accent: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 48.0), egui::Sense::click());
    let enabled = ui.is_enabled();
    let h = if enabled { kit::hover_t(ui, &resp) } else { 0.0 };
    let down = resp.is_pointer_button_down_on() as u8 as f32;
    let p = ui.painter();
    let fill = if accent {
        kit::mix(theme::PRIMARY, egui::Color32::WHITE, h * 0.14)
    } else {
        kit::mix(kit::mix(theme::SURFACE_CONTAINER_HIGH, egui::Color32::from_rgb(58, 68, 80), h), egui::Color32::from_rgb(72, 84, 98), down)
    };
    p.rect_filled(rect, egui::CornerRadius::same(12), if enabled { fill } else { kit::alpha(fill, 0.45) });
    let fg = if accent { egui::Color32::BLACK } else { kit::mix(theme::ON_SURFACE, egui::Color32::WHITE, h) };
    p.text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(if label.chars().count() == 1 { 19.0 } else { 16.0 }), if enabled { fg } else { kit::alpha(fg, 0.5) });
    resp
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
                system_page::system_page(ui, st);
            }
            Nav::Desktop => {
                st.visible_now.clear();
                st.hovered_index = None;
                desktop_page::desktop_page(ui, st);
            }
            Nav::Photos => {
                st.visible_now.clear();
                st.hovered_index = None;
                photos_page::photos_page(ui, st);
            }
            Nav::Settings => {
                st.visible_now.clear();
                st.hovered_index = None;
                settings_page::settings_page(ui, st);
            }
        }
    });
}

fn home_view(ui: &mut egui::Ui, st: &mut LibState) {
    hero(ui, st);
    collection_chips(ui, st);
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Recent").size(22.0).strong().color(egui::Color32::WHITE));
        ui.add_space(6.0);
        ui.label(egui::RichText::new(format!("{} games", st.games.len())).size(14.0).color(theme::ON_SURFACE_VAR));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if kit::button(ui, icon::SQUARES_FOUR, "See all", kit::Tone::Neutral, 130.0).clicked() {
                st.nav = Nav::Library;
                st.sound_tab = true;
            }
        });
    });
    ui.add_space(6.0);

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
            ui.spacing_mut().item_spacing.x = 16.0;
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
    ui.add_space(10.0);
    let cols = st.collections.clone();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        ui.add_sized(egui::vec2(128.0, 42.0), egui::Label::new(egui::RichText::new(format!("{}  COLLECTIONS", icon::FOLDERS)).size(12.5).strong().color(theme::ON_SURFACE_VAR)));
        for (ci, name) in cols.iter().enumerate() {
            let member = st.games[sel].collections.contains(&ci);
            if kit::choice_chip(ui, name, member).clicked() {
                st.collection_toggle = Some(ci);
                st.sound_tab = true;
            }
        }
        if kit::choice_chip(ui, &format!("{}  New", icon::PLUS), false).clicked() {
            st.naming = true;
            st.name_buf.clear();
            st.keyboard_open = true;
            st.sound_tab = true;
        }
    });
}

fn grid_view(ui: &mut egui::Ui, st: &mut LibState, title: &str) {
    let mut shown = filtered(st);
    view_header(ui, st, title, shown.len(), true);
    ui.add_space(12.0);
    apply_sort(st, &mut shown);
    if shown.is_empty() {
        st.visible_now.clear();
        st.hovered_index = None;
        empty_note(ui, st);
        return;
    }
    game_grid(ui, st, &shown, "grid", false);
}

/// A game page's header: title and count on the left; on the right the sort
/// order and, on the Library, whether it's grouped into collections.
fn view_header(ui: &mut egui::Ui, st: &mut LibState, title: &str, count: usize, library: bool) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).size(28.0).strong().color(egui::Color32::WHITE));
        ui.add_space(10.0);
        ui.label(egui::RichText::new(format!("{count} game{}", if count == 1 { "" } else { "s" })).size(15.0).color(theme::ON_SURFACE_VAR));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let modes = [SortMode::Recent, SortMode::Name, SortMode::Playtime, SortMode::Size];
            let cur = modes.iter().position(|m| *m == st.sort).unwrap_or(0);
            if let Some(i) = kit::segmented(ui, &modes.map(sort_label), cur) {
                st.sort = modes[i];
                st.sound_tab = true;
            }
            if library {
                ui.add_space(14.0);
                if let Some(i) = kit::segmented(ui, &["All games", "Collections"], st.library_grouped as usize) {
                    st.library_grouped = i == 1;
                    st.sound_tab = true;
                }
            }
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
    let mut shown: Vec<usize> = filtered(st).into_iter().filter(|&i| st.games[i].is_favorite).collect();
    view_header(ui, st, "Favorites", shown.len(), false);
    ui.add_space(12.0);
    apply_sort(st, &mut shown);
    if shown.is_empty() {
        st.visible_now.clear();
        st.hovered_index = None;
        kit::empty_state(ui, icon::STAR, "No favorites yet", "Tap the star on a game to pin it here");
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
            ui.spacing_mut().item_spacing = egui::vec2(16.0, 18.0);
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

/// Library: every game in a grid, or grouped by collection (the header's
/// switch picks).
fn library_view(ui: &mut egui::Ui, st: &mut LibState) {
    if st.library_grouped {
        tags_view(ui, st);
    } else {
        grid_view(ui, st, "Library");
    }
}

fn tags_view(ui: &mut egui::Ui, st: &mut LibState) {
    let shown = filtered(st);
    view_header(ui, st, "Library", shown.len(), true);
    ui.add_space(12.0);
    // Create a new collection (works even with no games yet).
    if kit::choice_chip(ui, &format!("{}  New collection", icon::FOLDER_PLUS), false).clicked() {
        st.naming = true;
        st.name_buf.clear();
        st.keyboard_open = true;
        st.sound_tab = true;
    }
    ui.add_space(6.0);
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
    // One group: a header (glyph, name, count, and for collections a two-tap
    // delete), then its tiles.
    let group_header = |ui: &mut egui::Ui, glyph: &str, name: &str, n: usize| {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(glyph).size(20.0).color(theme::PRIMARY));
            ui.add_space(4.0);
            ui.label(egui::RichText::new(name).size(19.0).strong().color(egui::Color32::WHITE));
            ui.add_space(4.0);
            kit::badge(ui, &n.to_string(), theme::ON_SURFACE_VAR);
        });
        ui.add_space(8.0);
    };
    egui::ScrollArea::vertical().id_salt("tags").show(ui, |ui| {
        // User collections first.
        for (ci, name) in cols.iter().enumerate() {
            let mut group: Vec<usize> = shown.iter().copied().filter(|&i| st.games[i].collections.contains(&ci)).collect();
            apply_sort(st, &mut group);
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(icon::FOLDER).size(20.0).color(theme::PRIMARY));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(name).size(19.0).strong().color(egui::Color32::WHITE));
                ui.add_space(4.0);
                kit::badge(ui, &group.len().to_string(), theme::ON_SURFACE_VAR);
                ui.add_space(8.0);
                let key = format!("col-del:{ci}");
                let armed = st.is_armed(&key);
                if kit::icon_btn(ui, icon::TRASH, if armed { "Tap again to delete the collection" } else { "Delete the collection (its games stay)" }, armed, true).clicked()
                    && st.confirm_tap(&key)
                {
                    delete = Some(ci);
                }
            });
            ui.add_space(8.0);
            if group.is_empty() {
                ui.label(egui::RichText::new("Empty · add games from the chips under the Home banner").size(14.0).color(theme::ON_SURFACE_VAR));
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(16.0, 18.0);
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
            ui.add_space(10.0);
        }
        // Auto categories by source.
        for (label, pred) in groups {
            let mut group: Vec<usize> = shown.iter().copied().filter(|&i| pred(&st.games[i])).collect();
            apply_sort(st, &mut group);
            if group.is_empty() {
                continue;
            }
            group_header(ui, icon::GAME_CONTROLLER, label, group.len());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(16.0, 18.0);
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
            ui.add_space(10.0);
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

/// Full-screen splash for the running game: its art behind, the cover, what
/// it is, and Stop.
fn splash_view(ui: &mut egui::Ui, st: &mut LibState) {
    st.hovered_index = None;
    let Some(i) = st.running_index.filter(|&i| i < st.games.len()) else {
        st.visible_now.clear();
        kit::empty_state(ui, icon::GAME_CONTROLLER, "No game is running", "Pick one from Home or the Library");
        return;
    };
    // Keep the running game's cover loaded while the splash is shown.
    st.visible_now = vec![i];
    let g = &st.games[i];

    // Hero art (or its gradient) full-bleed, under a scrim that deepens downward.
    let full = ui.max_rect().expand2(egui::vec2(24.0, 18.0));
    match &g.hero {
        ArtState::Ready(tex) => kit::cover_image(ui, full, tex, 0.0),
        _ => draw_hero_placeholder(ui.painter(), full, &g.name, 0.0),
    }
    kit::gradient_rect(ui.painter(), full, 0.0, egui::Color32::from_black_alpha(120), egui::Color32::from_black_alpha(235));

    ui.add_space(((ui.available_height() - 435.0) / 2.0).max(24.0));
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(290.0, 435.0), egui::Sense::hover());
        ui.painter().rect_filled(rect.translate(egui::vec2(0.0, 8.0)).expand(4.0), egui::CornerRadius::same(24), egui::Color32::from_black_alpha(110));
        draw_art(ui, rect, &g.cover, &g.name, 20.0);
        ui.painter().rect_stroke(rect, egui::CornerRadius::same(20), egui::Stroke::new(1.0, egui::Color32::from_white_alpha(28)), egui::StrokeKind::Inside);
        ui.add_space(52.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.add_space(26.0);
            ui.label(egui::RichText::new("NOW PLAYING").size(13.5).strong().color(theme::PRIMARY));
            ui.label(egui::RichText::new(&g.name).size(46.0).strong().color(egui::Color32::WHITE));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let live = match st.session_minutes {
                    Some(m) if m > 0 => format!("Running · {} this session", if m < 60 { format!("{m}m") } else { format!("{:.1}h", m as f32 / 60.0) }),
                    _ => "Running".to_string(),
                };
                kit::live_badge(ui, &live, RUNNING_GREEN);
                kit::badge(ui, &g.source, theme::ON_SURFACE_VAR);
                if g.vr {
                    kit::badge(ui, "VR", theme::PRIMARY);
                }
            });
            ui.label(egui::RichText::new(meta_label(g)).size(16.0).color(theme::ON_SURFACE_VAR));
            ui.add_space(30.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (r, resp) = ui.allocate_exact_size(egui::vec2(230.0, 58.0), egui::Sense::click());
                let h = kit::hover_t(ui, &resp);
                ui.painter().rect_filled(r, egui::CornerRadius::same(29), kit::mix(STOP_RED, egui::Color32::WHITE, h * 0.12));
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, format!("{}  Stop", icon::STOP), egui::FontId::proportional(21.0), egui::Color32::WHITE);
                if resp.clicked() {
                    st.stop_request = Some(i);
                    st.sound_tab = true;
                }
                if kit::button(ui, icon::SQUARES_FOUR, "Library", kit::Tone::Neutral, 150.0).clicked() {
                    st.show_splash = false;
                    st.nav = Nav::Library;
                    st.sound_tab = true;
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

/// Every gesture the overlay understands, grouped by what the laser is on.
const CONTROLS: &[(&str, &[(&str, &str)])] = &[
    (
        "Anywhere",
        &[
            ("Left system button", "summon / dismiss the dashboard (it re-centres in front of you)"),
            ("Double-B (left hand)", "hide every screen + the keyboard, or bring them back"),
            ("Hold trackpad, move hand", "drag the playspace (A + B on a UdCap glove) · System › Playspace › Drag"),
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
            ("B", "left click without moving the cursor (fiddly targets) · or a middle click (Desktop › Mouse)"),
            ("Thumbstick", "scroll (speed in Desktop › Mouse)"),
            ("Grip", "move the screen (a docked group moves as one)"),
            ("Grip + trigger, push / pull", "resize"),
            ("Grip + stick up / down", "push it away / pull it closer"),
            ("Grip + trigger + stick ◀▶", "curve it"),
            ("Release next to another screen", "dock to that edge (teal bar shows the spot)"),
            ("Aim just above the top edge", "shows the swap island (also for 2 s when a screen appears) · tap another number to put that screen here"),
            ("B while gripping", "undock"),
        ],
    ),
    (
        "Gaming mode",
        &[
            ("Watch › gamepad button, or Settings", "controllers become an Xbox pad; the VR app behind stops seeing them"),
            ("Watch › mouse button", "laser + mouse for the hand that tapped it, until it leaves the screen"),
            ("Left system button", "dashboard as usual — every hand points while it's up"),
            ("Watch › screen button", "World · Head (trails you) · Hands (held like a handheld)"),
            ("Watch › sliders", "remap profile (JSON in ~/.config/monadeck/gamepad_profiles) · Guide button"),
            ("VR game behind reacts to the pad", "a VR game reads the virtual pad as its own gamepad (VRChat mutes, opens menus). Once Monadeck has seen a game running in VR it hides the pad from it on GE-style Protons, from that game's next launch. On Valve's Proton only launch options work: paste the ones from Monadeck's desktop app (they carry SDL_GAMECONTROLLER_IGNORE_DEVICES=0x045e/0x028e) into the game in Steam"),
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
        "Watch (left wrist by default · Settings › Wrist watch)",
        &[
            ("Point with the other hand", "it wins over whatever is behind it"),
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

/// The game-launch popup (its own composition layer, SteamVR-style): the game's
/// hero art — or a name-tinted gradient when there's none — behind a centred
/// spinner, title, and status line. Shown while a game starts up, independent of
/// the dashboard, so closing the overlay doesn't hide it. The panel is cleared
/// transparent, so the rounded card is the whole visible popup.
/// What sits above the title on the launch popup.
#[derive(Clone, Copy, PartialEq)]
pub enum LaunchGlyph {
    Spinner,
    /// The game is up (shown briefly before the popup closes).
    Done,
    /// It didn't make it.
    Failed,
}

pub fn build_launch_popup(ctx: &egui::Context, name: &str, status: &str, hero: &ArtState, glyph: LaunchGlyph) {
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
        match glyph {
            LaunchGlyph::Spinner => {
                ui.put(spin, egui::Spinner::new().size(44.0).color(theme::PRIMARY));
            }
            LaunchGlyph::Done | LaunchGlyph::Failed => {
                let (icon_glyph, color) = if glyph == LaunchGlyph::Done { (icon::CHECK_CIRCLE, RUNNING_GREEN) } else { (icon::X_CIRCLE, STOP_RED) };
                painter.text(spin.center(), egui::Align2::CENTER_CENTER, icon_glyph, egui::FontId::proportional(52.0), color);
            }
        }
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
    let session = if running { st.session_minutes } else { None };
    let action = match sel {
        Some(i) => hero_banner(ui, &st.games[i], running, session, st.uevr_available),
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

/// The selected game's banner: its hero art (or a name-tinted gradient while
/// that loads, so the shape never jumps) under a scrim, badges on top, the
/// logo or title and what's known about it bottom-left, actions bottom-right.
fn hero_banner(ui: &mut egui::Ui, g: &LibGame, running: bool, session: Option<u32>, uevr_available: bool) -> HeroAction {
    let w = ui.available_width();
    let h = (w * 0.24).clamp(200.0, 280.0);
    let (rect, base) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let radius = 22.0;
    match &g.hero {
        ArtState::Ready(tex) => kit::cover_image(ui, rect, tex, radius),
        _ => draw_hero_placeholder(ui.painter(), rect, &g.name, radius),
    }
    let painter = ui.painter();
    kit::gradient_rect(painter, rect, radius, egui::Color32::from_black_alpha(0), egui::Color32::from_black_alpha(215));
    painter.rect_stroke(rect, egui::CornerRadius::same(radius as u8), egui::Stroke::new(1.0, egui::Color32::from_white_alpha(16)), egui::StrokeKind::Inside);

    // Badges, top-left: running, source, VR.
    let mut x = rect.left() + 20.0;
    let mut art_badge = |text: &str, fg: egui::Color32, dot: bool| {
        let g = painter.layout_no_wrap(text.to_string(), egui::FontId::proportional(13.5), fg);
        let pad = if dot { 30.0 } else { 14.0 };
        let r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 18.0), egui::vec2(g.size().x + pad + 14.0, 30.0));
        painter.rect_filled(r, egui::CornerRadius::same(15), egui::Color32::from_black_alpha(150));
        painter.rect_stroke(r, egui::CornerRadius::same(15), egui::Stroke::new(1.0, kit::alpha(fg, 0.35)), egui::StrokeKind::Inside);
        if dot {
            painter.circle_filled(egui::pos2(r.left() + 16.0, r.center().y), 4.5, fg);
        }
        painter.galley(egui::pos2(r.left() + pad, r.center().y - g.size().y / 2.0), g, fg);
        x = r.right() + 8.0;
    };
    if running {
        let t = match session {
            Some(m) if m > 0 => format!("Running · {}", if m < 60 { format!("{m}m") } else { format!("{:.1}h", m as f32 / 60.0) }),
            _ => "Running".to_string(),
        };
        art_badge(&t, RUNNING_GREEN, true);
    }
    art_badge(&g.source, theme::ON_SURFACE, false);
    if g.vr {
        art_badge("VR", theme::PRIMARY, false);
    }

    // Logo art over the scrim, else the title; the details under it.
    let meta = meta_label(g);
    if let ArtState::Ready(logo) = &g.logo {
        let [lw, lh] = logo.size();
        let aspect = lw as f32 / lh.max(1) as f32;
        let (max_h, max_w) = ((h * 0.36).min(104.0), w * 0.40);
        let (mut dw, mut dh) = (max_h * aspect, max_h);
        if dw > max_w {
            dw = max_w;
            dh = dw / aspect;
        }
        let logo_rect = egui::Rect::from_min_size(egui::pos2(rect.left() + 24.0, rect.bottom() - 46.0 - dh), egui::vec2(dw, dh));
        painter.image(logo.id(), logo_rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    } else {
        let title = kit::fit_text(ui, &g.name, 36.0, egui::Color32::WHITE, w * 0.5);
        painter.galley(egui::pos2(rect.left() + 24.0, rect.bottom() - 50.0 - title.size().y), title, egui::Color32::WHITE);
    }
    painter.text(egui::pos2(rect.left() + 26.0, rect.bottom() - 22.0), egui::Align2::LEFT_BOTTOM, meta, egui::FontId::proportional(14.5), theme::ON_SURFACE);

    // Actions, bottom-right: Play / Stop, the favourite star, the UEVR toggle.
    let play = egui::Rect::from_min_size(egui::pos2(rect.right() - 20.0 - 176.0, rect.bottom() - 20.0 - 56.0), egui::vec2(176.0, 56.0));
    let resp = ui.interact(play, base.id.with("play"), egui::Sense::click());
    let ph = kit::hover_t(ui, &resp);
    let (label, fill, fg, action) = if running {
        (format!("{}  Stop", icon::STOP), STOP_RED, egui::Color32::WHITE, HeroAction::Stop)
    } else {
        (format!("{}  Play", icon::PLAY), theme::PRIMARY, egui::Color32::BLACK, HeroAction::Launch)
    };
    let p = ui.painter();
    p.rect_filled(play.translate(egui::vec2(0.0, 4.0)), egui::CornerRadius::same(28), egui::Color32::from_black_alpha(90));
    p.rect_filled(play, egui::CornerRadius::same(28), kit::mix(fill, egui::Color32::WHITE, ph * 0.14));
    p.text(play.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(20.0), fg);
    let play_clicked = resp.clicked();

    let star = egui::Rect::from_min_size(egui::pos2(play.left() - 12.0 - 56.0, play.top()), egui::vec2(56.0, 56.0));
    let star_clicked = kit::glass_button(ui, star, base.id.with("fav"), icon::STAR, "", g.is_favorite, FAV_GOLD)
        .on_hover_text(if g.is_favorite { "Remove from favorites" } else { "Add to favorites" })
        .clicked();

    // VR-Mod (UEVR) toggle, only for Unreal Engine games we can inject.
    let uevr_clicked = uevr_available && g.uevr_capable && {
        let r = egui::Rect::from_min_size(egui::pos2(star.left() - 12.0 - 124.0, play.top()), egui::vec2(124.0, 56.0));
        kit::glass_button(ui, r, base.id.with("uevr"), icon::VIRTUAL_REALITY, "UEVR", g.uevr, theme::PRIMARY)
            .on_hover_text(if g.uevr { "VR Mod on: launches through UEVR" } else { "Launch through UEVR (VR Mod)" })
            .clicked()
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

/// A name-tinted gradient standing in for hero art (none, or still loading),
/// so each game keeps a consistent look instead of a flat block.
fn draw_hero_placeholder(painter: &egui::Painter, rect: egui::Rect, name: &str, radius: f32) {
    const PALETTE: [(egui::Color32, egui::Color32); 6] = [
        (egui::Color32::from_rgb(33, 44, 62), egui::Color32::from_rgb(18, 22, 30)),
        (egui::Color32::from_rgb(46, 33, 58), egui::Color32::from_rgb(24, 19, 30)),
        (egui::Color32::from_rgb(26, 50, 46), egui::Color32::from_rgb(16, 28, 27)),
        (egui::Color32::from_rgb(54, 40, 28), egui::Color32::from_rgb(30, 23, 17)),
        (egui::Color32::from_rgb(56, 31, 42), egui::Color32::from_rgb(30, 18, 23)),
        (egui::Color32::from_rgb(33, 48, 35), egui::Color32::from_rgb(19, 25, 20)),
    ];
    let (top, bottom) = PALETTE[name_hash(name) as usize % PALETTE.len()];
    kit::gradient_rect(painter, rect, radius, top, bottom);
}

fn name_hash(s: &str) -> u32 {
    s.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
}

fn hero_empty(ui: &mut egui::Ui) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 240.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, egui::CornerRadius::same(22), theme::SURFACE_CONTAINER);
    p.rect_stroke(rect, egui::CornerRadius::same(22), egui::Stroke::new(1.0, egui::Color32::from_white_alpha(12)), egui::StrokeKind::Inside);
    p.text(rect.center() - egui::vec2(0.0, 18.0), egui::Align2::CENTER_CENTER, icon::GAME_CONTROLLER, egui::FontId::proportional(40.0), theme::ON_SURFACE_VAR);
    p.text(rect.center() + egui::vec2(0.0, 26.0), egui::Align2::CENTER_CENTER, "Pick a game to get started", egui::FontId::proportional(18.0), theme::ON_SURFACE);
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
    if st.games.is_empty() {
        kit::empty_state(ui, icon::GAME_CONTROLLER, "No games found", "Add games to Steam or your non-Steam shortcuts");
    } else {
        kit::empty_state(ui, icon::MAGNIFYING_GLASS, "No matches", "Try a different search");
    }
}

/// What's known about a game, for the hero and the splash: playtime, when it
/// was last played, size.
fn meta_label(g: &LibGame) -> String {
    let mut parts = Vec::new();
    // Steam's own playtime when known, else our tracked total (non-Steam games).
    if let Some(m) = g.playtime_minutes.or(g.tracked_minutes) {
        parts.push(human_playtime(m));
    }
    if let Some(a) = played_ago(g.last_played) {
        parts.push(format!("played {a}"));
    }
    if let Some(sz) = g.size_on_disk {
        parts.push(human_size(sz));
    }
    if parts.is_empty() {
        parts.push("Never played".to_string());
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

/// A game capsule: 2:3 cover (lifts on hover, accent rim when selected), a
/// "playing" badge and the favourite star on it, and its name underneath.
fn tile(ui: &mut egui::Ui, game: &LibGame, selected: bool, running: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(TILE_W, TILE_H + TILE_CAPTION_H), egui::Sense::click());
    let h = kit::hover_t(ui, &resp);
    let sel = ui.ctx().animate_bool_with_time(resp.id.with("sel"), selected, 0.16);
    let art = egui::Rect::from_min_size(rect.min, egui::vec2(TILE_W, TILE_H)).expand(h * 4.0);
    let radius = 16.0;
    let p = ui.painter();
    if h > 0.001 || sel > 0.001 {
        p.rect_filled(art.translate(egui::vec2(0.0, 6.0)).expand(2.0), egui::CornerRadius::same(18), egui::Color32::from_black_alpha((h.max(sel * 0.6) * 110.0) as u8));
    }
    draw_art(ui, art, &game.cover, &game.name, radius);
    let p = ui.painter();
    let rim = kit::mix(kit::mix(egui::Color32::from_white_alpha(14), egui::Color32::from_white_alpha(70), h), theme::PRIMARY, sel);
    p.rect_stroke(art, egui::CornerRadius::same(radius as u8), egui::Stroke::new(1.0 + 2.0 * sel, rim), egui::StrokeKind::Inside);
    if running {
        let g = p.layout_no_wrap("Playing".to_string(), egui::FontId::proportional(12.0), RUNNING_GREEN);
        let r = egui::Rect::from_min_size(art.min + egui::vec2(10.0, 10.0), egui::vec2(g.size().x + 32.0, 24.0));
        p.rect_filled(r, egui::CornerRadius::same(12), egui::Color32::from_black_alpha(170));
        p.circle_filled(egui::pos2(r.left() + 12.0, r.center().y), 4.0, RUNNING_GREEN);
        p.galley(egui::pos2(r.left() + 22.0, r.center().y - g.size().y / 2.0), g, RUNNING_GREEN);
    }
    if game.is_favorite {
        let c = egui::pos2(art.right() - 20.0, art.top() + 20.0);
        p.circle_filled(c, 13.0, egui::Color32::from_black_alpha(160));
        p.text(c, egui::Align2::CENTER_CENTER, icon::STAR, egui::FontId::proportional(15.0), FAV_GOLD);
    }
    let fg = kit::mix(kit::mix(theme::ON_SURFACE_VAR, egui::Color32::WHITE, h), egui::Color32::WHITE, sel);
    let cap = kit::fit_text(ui, &game.name, 14.0, fg, TILE_W - 4.0);
    ui.painter().galley(egui::pos2(rect.left() + 2.0, rect.top() + TILE_H + 9.0), cap, fg);
    resp.on_hover_text(&game.name)
}

/// Draw a game's cover from its lazy art state: the texture (centre-cropped,
/// rounded) when ready, a quiet tint while loading, or the name on a card when
/// there's none.
fn draw_art(ui: &egui::Ui, rect: egui::Rect, art: &ArtState, name: &str, radius: f32) {
    let r = egui::CornerRadius::same(radius as u8);
    match art {
        ArtState::Ready(tex) => kit::cover_image(ui, rect, tex, radius),
        ArtState::Missing => {
            draw_hero_placeholder(ui.painter(), rect, name, radius);
            let p = ui.painter();
            p.text(rect.center() - egui::vec2(0.0, 26.0), egui::Align2::CENTER_CENTER, icon::GAME_CONTROLLER, egui::FontId::proportional(30.0), kit::alpha(egui::Color32::WHITE, 0.35));
            let g = ui.fonts(|f| f.layout(short(name), egui::FontId::proportional(16.0), theme::ON_SURFACE, rect.width() - 28.0));
            p.galley(egui::pos2(rect.center().x - g.size().x / 2.0, rect.center().y + 6.0), g, theme::ON_SURFACE);
        }
        _ => {
            ui.painter().rect_filled(rect, r, theme::SURFACE_CONTAINER);
        }
    }
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
