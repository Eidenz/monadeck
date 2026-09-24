// The tabbed Settings page: a category list on the left (SteamVR-style), one
// focused page of cards on the right. Built from a small widget kit of its own
// (NemuriXR's: animated switches, toggle tiles, segmented choices), so the
// classic single-scroll page (`settings_view` in the parent) stays untouched and
// either can be dropped without touching the other.
use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Response, Sense, Stroke, StrokeKind};
use egui_phosphor::regular as icon;

use super::{action_button, modern_slider, watch_button_info, LibState, CONTROLS};
use crate::gfx::theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsTab {
    Dashboard,
    Watch,
    Controllers,
    Gaming,
    Notifications,
    Osc,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 6] = [Self::Dashboard, Self::Watch, Self::Controllers, Self::Gaming, Self::Notifications, Self::Osc];

    fn glyph(self) -> &'static str {
        match self {
            Self::Dashboard => icon::LAYOUT,
            Self::Watch => icon::WATCH,
            Self::Controllers => icon::HAND_POINTING,
            Self::Gaming => icon::GAME_CONTROLLER,
            Self::Notifications => icon::BELL,
            Self::Osc => icon::BROADCAST,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::Watch => "Wrist watch",
            Self::Controllers => "Controllers",
            Self::Gaming => "Gaming mode",
            Self::Notifications => "Notifications",
            Self::Osc => "OSC",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Self::Dashboard => "Where the dashboard sits, what's behind it, how it sounds",
            Self::Watch => "What your wrist shows, and which buttons it carries",
            Self::Controllers => "Switched-off controllers, freezing, and every gesture",
            Self::Gaming => "Controllers as an Xbox pad for flat games",
            Self::Notifications => "Desktop and XSOverlay messages as toasts",
            Self::Osc => "Let games and tools drive the overlay",
        }
    }
}

const NAV_W: f32 = 236.0;
/// Right-hand slider width.
const SLIDER_W: f32 = 340.0;
/// Toggle tiles: icon chip on top, title and one line at the bottom.
const TILE_H: f32 = 132.0;
/// Seconds a category takes to fade in.
const TAB_FADE: f64 = 0.18;

pub(super) fn settings_page(ui: &mut egui::Ui, st: &mut LibState) {
    header(ui, st.settings_tab);
    let body_h = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(egui::vec2(NAV_W, body_h), Layout::top_down(Align::Min), |ui| {
            ui.set_min_size(egui::vec2(NAV_W, body_h));
            nav(ui, st);
        });
        ui.add_space(26.0);
        let w = ui.available_width();
        ui.allocate_ui_with_layout(egui::vec2(w, body_h), Layout::top_down(Align::Min), |ui| {
            ui.set_min_size(egui::vec2(w, body_h));
            ui.set_opacity(tab_fade(ui, st.settings_tab));
            let tab = st.settings_tab;
            egui::ScrollArea::vertical().id_salt(("settings-page", tab as u8)).auto_shrink([false, false]).show(ui, |ui| {
                // Leave the scrollbar its own gutter.
                ui.set_width(w - 18.0);
                match tab {
                    SettingsTab::Dashboard => dashboard(ui, st),
                    SettingsTab::Watch => watch(ui, st),
                    SettingsTab::Controllers => controllers(ui, st),
                    SettingsTab::Gaming => gaming(ui, st),
                    SettingsTab::Notifications => notifications(ui, st),
                    SettingsTab::Osc => osc(ui, st),
                }
                ui.add_space(24.0);
            });
        });
    });
}

fn header(ui: &mut egui::Ui, tab: SettingsTab) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon::GEAR).size(28.0).color(theme::PRIMARY));
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Settings").size(28.0).strong().color(Color32::WHITE));
        ui.add_space(14.0);
        ui.label(egui::RichText::new(tab.blurb()).size(15.0).color(theme::ON_SURFACE_VAR));
    });
    ui.add_space(14.0);
}

/// Opacity for the open category: fades in over `TAB_FADE` after a switch.
fn tab_fade(ui: &egui::Ui, tab: SettingsTab) -> f32 {
    let now = ui.input(|i| i.time);
    let id = egui::Id::new("settings-page-fade");
    let (shown, since) = ui.ctx().data_mut(|d| *d.get_temp_mut_or(id, (tab as u8, now)));
    if shown != tab as u8 {
        ui.ctx().data_mut(|d| d.insert_temp(id, (tab as u8, now)));
        return 0.0;
    }
    (((now - since) / TAB_FADE) as f32).clamp(0.0, 1.0)
}

// --- the category list ------------------------------------------------------------

fn nav(ui: &mut egui::Ui, st: &mut LibState) {
    ui.spacing_mut().item_spacing.y = 6.0;
    for tab in SettingsTab::ALL {
        if nav_item(ui, tab.glyph(), tab.label(), st.settings_tab == tab).clicked() && st.settings_tab != tab {
            st.settings_tab = tab;
            st.sound_tab = true;
        }
    }
    // The classic page, pinned to the bottom of the list.
    let reserve = 64.0;
    let room = ui.available_height() - reserve;
    if room > 0.0 {
        ui.add_space(room);
    }
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(NAV_W, 52.0), Sense::click());
    let h = hover_t(ui, &resp);
    let p = ui.painter();
    if h > 0.001 {
        p.rect_filled(rect, CornerRadius::same(12), alpha(Color32::WHITE, 0.05 * h));
    }
    p.text(Pos2::new(rect.left() + 16.0, rect.center().y), Align2::LEFT_CENTER, icon::LIST_BULLETS, FontId::proportional(18.0), theme::ON_SURFACE_VAR);
    p.text(Pos2::new(rect.left() + 44.0, rect.center().y - 9.0), Align2::LEFT_CENTER, "Classic layout", FontId::proportional(14.5), theme::ON_SURFACE);
    p.text(Pos2::new(rect.left() + 44.0, rect.center().y + 10.0), Align2::LEFT_CENTER, "One long page, as before", FontId::proportional(12.0), theme::ON_SURFACE_VAR);
    if resp.clicked() {
        st.settings_classic = true;
        st.sound_tab = true;
    }
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

// --- pages ---------------------------------------------------------------------------

fn dashboard(ui: &mut egui::Ui, st: &mut LibState) {
    group(ui, "Placement");
    card(ui, |ui| {
        row(ui, "Distance", "How far in front of you it opens", SLIDER_W, |ui| {
            modern_slider(ui, &mut st.panel_dist, 0.8..=2.5, SLIDER_W, |v| format!("{v:.2} m"));
        });
        divider(ui);
        // A stepper, not a slider: a slider would sit on the panel it resizes,
        // and the grab point would jump as it grows under you.
        row(ui, "Size", "Every panel, together", 200.0, |ui| {
            stepper(ui, &mut st.panel_scale, 0.7, 1.4, 0.05, |v| format!("{:.0}%", v * 100.0));
        });
        divider(ui);
        row(ui, "Curve", "1 wraps around you · higher is flatter", SLIDER_W, |ui| {
            modern_slider(ui, &mut st.panel_curve, 1.0..=3.0, SLIDER_W, |v| format!("{v:.2}"));
        });
        divider(ui);
        if switch_row(ui, "Tilt on summon", "Match your headset's pitch instead of standing upright", &mut st.summon_tilt) {
            st.sound_tab = true;
        }
        divider(ui);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if action_button(ui, icon::CROSSHAIR_SIMPLE, "Recenter panel").clicked() {
                st.recenter_request = true;
                st.sound_tab = true;
            }
            if action_button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Reset placement").clicked() {
                st.panel_dist = 1.5;
                st.panel_scale = 1.0;
                st.panel_curve = 1.0;
                st.flash("Panel placement reset");
            }
            ui.label(egui::RichText::new("Grip any panel to move it").size(13.0).color(theme::ON_SURFACE_VAR));
        });
        ui.add_space(8.0);
    });

    group(ui, "Background");
    card(ui, |ui| {
        let sub = format!("While no game runs · {}", st.skybox_source);
        if switch_row(ui, "360° background", &sub, &mut st.skybox_enabled) {
            st.sound_tab = true;
        }
        divider(ui);
        let sub = format!("A 2:1 JPEG or PNG at {}", st.skybox_custom_hint);
        row(ui, "Your own panorama", &sub, 150.0, |ui| {
            if action_button(ui, icon::ARROW_CLOCKWISE, "Reload").clicked() {
                st.skybox_reload_request = true;
                st.sound_tab = true;
            }
        });
    });

    group(ui, "Sound");
    card(ui, |ui| {
        if switch_row(ui, "UI sounds", "Select, launch and tab clicks", &mut st.audio_enabled) {
            st.sound_tab = true;
        }
        divider(ui);
        let enabled = st.audio_enabled;
        row(ui, "Volume", "", SLIDER_W, |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                modern_slider(ui, &mut st.audio_volume, 0.0..=1.0, SLIDER_W, |v| format!("{:.0}%", v * 100.0));
            });
        });
    });

    group(ui, "Library");
    card(ui, |ui| {
        let sub = format!("{} games · re-scan for new games and artwork", st.games.len());
        row(ui, "Refresh library", &sub, 150.0, |ui| {
            if action_button(ui, icon::ARROW_CLOCKWISE, "Refresh").clicked() {
                st.refresh_request = true;
                st.sound_tab = true;
            }
        });
        if st.uevr_available {
            divider(ui);
            row(ui, "VR Mod injection delay", "Wait after launch before UEVR injects · raise it for slow games", SLIDER_W, |ui| {
                let mut d = st.uevr_delay as f32;
                if modern_slider(ui, &mut d, 0.0..=120.0, SLIDER_W, |v| format!("{v:.0} s")) {
                    st.uevr_delay = d.round() as u32;
                }
            });
        }
    });
}

fn watch(ui: &mut egui::Ui, st: &mut LibState) {
    let w = ui.available_width();
    let tile = egui::vec2((w - 12.0) / 2.0, TILE_H);
    let mut t = false;
    ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
    ui.horizontal(|ui| {
        t |= toggle_tile(ui, tile, icon::WATCH, "Show the watch", "Clock, batteries and quick buttons", &mut st.watch_enabled, true);
        t |= toggle_tile(ui, tile, icon::CLOCK, "Minimal watch", "Just the clock · tap it to peek", &mut st.watch_mini, st.watch_enabled);
    });
    ui.horizontal(|ui| {
        t |= toggle_tile(ui, tile, icon::TIMER, "24-hour clock", "Also the bottom bar's clock", &mut st.watch_24h, true);
        t |= toggle_tile(ui, tile, icon::LOCK, "Position locked", "Unlock to grip-move it", &mut st.watch_locked, st.watch_enabled);
    });
    ui.spacing_mut().item_spacing = egui::vec2(10.0, 12.0);

    group(ui, "Wrist");
    card(ui, |ui| {
        row(ui, "Worn on", "The other hand points at it · each wrist keeps its own spot", 230.0, |ui| {
            if let Some(i) = segmented(ui, &["Left", "Right"], st.watch_right_hand as usize) {
                let right = i == 1;
                if right != st.watch_right_hand {
                    st.watch_right_hand = right;
                    t = true;
                }
            }
        });
        divider(ui);
        row(ui, "Reset position", "Back to the default spot for this wrist", 150.0, |ui| {
            if action_button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Reset").clicked() {
                st.watch_reset_request = true;
                st.sound_tab = true;
            }
        });
    });

    group(ui, "Quick buttons · tap one to change it");
    let slot_w = (w - 12.0 * 3.0) / 4.0;
    ui.spacing_mut().item_spacing.x = 12.0;
    ui.horizontal(|ui| {
        for slot in 0..4 {
            let id = st.watch_buttons.get(slot).cloned().unwrap_or_default();
            let (glyph, label) = watch_button_info(&id);
            if quick_tile(ui, egui::vec2(slot_w, 118.0), slot + 1, glyph, label).clicked() {
                st.watch_button_cycle = Some(slot);
                st.sound_tab = true;
            }
        }
    });
    ui.spacing_mut().item_spacing.x = 10.0;

    group(ui, "Time zones · under the clock, up to two");
    card(ui, |ui| {
        let zones = st.watch_zone_ids.clone();
        for (slot, id) in zones.iter().enumerate() {
            if slot > 0 {
                divider(ui);
            }
            let now = st.watch_times.get(slot).map(|(_, t)| t.clone()).unwrap_or_default();
            let title = id.rsplit('/').next().unwrap_or(id).replace('_', " ");
            let sub = if now.is_empty() { id.clone() } else { format!("{id} · {now} now") };
            row(ui, &title, &sub, 170.0, |ui| {
                if small_icon(ui, icon::TRASH, "Remove this zone").clicked() {
                    st.watch_zone_remove = Some(slot);
                }
                ui.add_space(8.0);
                if small_icon(ui, icon::CARET_RIGHT, "Next zone").clicked() {
                    st.watch_zone_cycle = Some((slot, 1));
                }
                if small_icon(ui, icon::CARET_LEFT, "Previous zone").clicked() {
                    st.watch_zone_cycle = Some((slot, -1));
                }
            });
        }
        if zones.len() < 2 {
            if !zones.is_empty() {
                divider(ui);
            }
            row(ui, if zones.is_empty() { "No extra time zones" } else { "Another time zone" }, "", 160.0, |ui| {
                if action_button(ui, icon::PLUS, "Add zone").clicked() {
                    st.watch_zone_add = true;
                    st.sound_tab = true;
                }
            });
        }
    });
    if t {
        st.sound_tab = true;
    }
}

fn controllers(ui: &mut egui::Ui, st: &mut LibState) {
    group(ui, "When a controller switches off");
    card(ui, |ui| match st.hold_pose {
        Some(hold) => {
            row(ui, "In-game hands", "Freeze: they stay where they were · Let go: the game sees them off (avatar poses take over)", 250.0, |ui| {
                if let Some(i) = segmented(ui, &["Freeze", "Let go"], (!hold) as usize) {
                    let want = i == 0;
                    if want != hold {
                        st.hold_pose_request = Some(want);
                        st.sound_tab = true;
                    }
                }
            });
            note(ui, icon::INFO, "Back to Freeze whenever Monado restarts · also on a watch quick button, and over OSC (/monadeck/letgo)");
        }
        None => {
            note(ui, icon::INFO, "Needs Monadeck's Monado fork (libmonado 1.9) with the service running.");
        }
    });

    group(ui, "Freeze controllers");
    card(ui, |ui| {
        row(ui, "Countdown", "Time to settle into position before System › Monado › Freeze holds a game's hands", SLIDER_W, |ui| {
            modern_slider(ui, &mut st.freeze_delay_secs, 0.0..=10.0, SLIDER_W, |v| if v < 0.5 { "none".into() } else { format!("{v:.0} s") });
        });
    });

    group(ui, "Every gesture");
    card(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        for (i, (title, rows)) in CONTROLS.iter().enumerate() {
            ui.add_space(if i == 0 { 14.0 } else { 18.0 });
            ui.label(egui::RichText::new(*title).size(14.0).strong().color(theme::PRIMARY));
            ui.add_space(2.0);
            for (keys, what) in rows.iter() {
                gesture_row(ui, keys, what);
            }
        }
        ui.add_space(14.0);
    });
}

fn gaming(ui: &mut egui::Ui, st: &mut LibState) {
    use crate::desktop::DockMode;
    card(ui, |ui| {
        let mut on = st.game_mode;
        if switch_row(ui, "Gaming mode", "Controllers become an Xbox pad · the VR app behind gets no controller input", &mut on) {
            st.game_mode_request = Some(on);
        }
        if let Some(e) = st.game_pad_error.clone() {
            note(ui, icon::WARNING, &format!("No virtual pad: {e} · needs write access to /dev/uinput (the input group, or a udev rule). Remaps to keys and the mouse still work."));
        } else if st.game_mode {
            note(ui, icon::CHECK, "Pad plugged in as \"Microsoft X-Box 360 pad\" · the watch's Mouse button gives a laser back");
        }
    });

    group(ui, "Screens");
    card(ui, |ui| {
        row(ui, "Hang from", "World: pinned · Head: trail your view · Hands: held between the controllers", 320.0, |ui| {
            let modes = DockMode::ALL;
            let labels: Vec<&str> = modes.iter().map(|m| m.label()).collect();
            let cur = modes.iter().position(|m| *m == st.game_dock).unwrap_or(0);
            if let Some(i) = segmented(ui, &labels, cur) {
                if modes[i] != st.game_dock {
                    st.game_dock_request = Some(modes[i]);
                }
            }
        });
        divider(ui);
        row(ui, "Handheld size", "Screen width while held · grip + trigger + push/pull resizes it too", SLIDER_W, |ui| {
            modern_slider(ui, &mut st.game_handheld_width, 0.3..=1.2, SLIDER_W, |v| format!("{v:.2} m"));
        });
    });

    group(ui, "Pad");
    card(ui, |ui| {
        if switch_row(ui, "Rumble", "Game rumble becomes controller haptics", &mut st.game_rumble) {
            st.sound_tab = true;
        }
        divider(ui);
        if switch_row(ui, "Hide the pad from VR games", "A small Proton fix per game seen in VR, from its next launch", &mut st.game_hide_pad) {
            st.sound_tab = true;
        }
        if st.game_hide_pad && !st.game_protonfixes_ok {
            note(ui, icon::WARNING, "No GE-style Proton found: only GE-Proton reads the fix. With Valve's Proton, paste the launch options from Monadeck's desktop app.");
        }
    });

    group(ui, "Remap profile");
    card(ui, |ui| {
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            let names = st.game_profiles.clone();
            for (i, name) in names.iter().enumerate() {
                if choice_chip(ui, name, st.game_profile == *name).clicked() && st.game_profile != *name {
                    st.game_profile_select = Some(i);
                    st.sound_tab = true;
                }
            }
        });
        ui.add_space(4.0);
        divider(ui);
        row(ui, "Profiles folder", "~/.config/monadeck/gamepad_profiles · edit them in the desktop app", 150.0, |ui| {
            if action_button(ui, icon::ARROW_CLOCKWISE, "Reload").clicked() {
                st.game_profiles_reload = true;
                st.sound_tab = true;
            }
        });
    });
}

fn notifications(ui: &mut egui::Ui, st: &mut LibState) {
    let w = ui.available_width();
    let tile = egui::vec2((w - 12.0) / 2.0, TILE_H);
    let mut t = false;
    ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
    ui.horizontal(|ui| {
        let desk = if st.notif_dbus_ok { "What your desktop shows · listening" } else { "What your desktop shows · unavailable" };
        t |= toggle_tile(ui, tile, icon::BELL, "Desktop", desk, &mut st.notif_enabled, true);
        let xso = if st.notif_udp_ok { "VRCX and friends · udp/42069" } else { "Port busy: WayVR or XSOverlay running?" };
        t |= toggle_tile(ui, tile, icon::CHAT_CIRCLE_DOTS, "XSOverlay", xso, &mut st.notif_xso, true);
    });
    ui.spacing_mut().item_spacing = egui::vec2(10.0, 12.0);

    group(ui, "Sound");
    card(ui, |ui| {
        t |= switch_row(ui, "Ding", "A soft two-note chime with each notification", &mut st.notif_sound);
        divider(ui);
        let enabled = st.notif_sound;
        row(ui, "Volume", "On top of the UI volume", SLIDER_W, |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                modern_slider(ui, &mut st.notif_volume, 0.0..=1.0, SLIDER_W, |v| if v < 0.05 { "muted".into() } else { format!("{:.0}%", v * 100.0) });
            });
        });
        divider(ui);
        row(ui, "Try it", "Show a sample notification", 150.0, |ui| {
            if action_button(ui, icon::BELL_RINGING, "Test").clicked() {
                st.notif_test_request = true;
            }
        });
    });
    if t {
        st.sound_tab = true;
    }
}

fn osc(ui: &mut egui::Ui, st: &mut LibState) {
    card(ui, |ui| {
        let port = st.osc_port as u16;
        let status = if !st.osc_enabled {
            "VRChat avatar parameters, VRCOSC and other tools".to_string()
        } else if st.osc_ok {
            format!("Listening on udp/{port}")
        } else {
            format!("udp/{port} is busy · pick another port, or route through an OSC router")
        };
        if switch_row(ui, "Listen for OSC", &status, &mut st.osc_enabled) {
            st.sound_tab = true;
        }
        divider(ui);
        row(ui, "Port", "VRChat sends to 9001 · applies a second after the last change", 200.0, |ui| {
            stepper(ui, &mut st.osc_port, 1024.0, 65535.0, 1.0, |v| format!("{}", v as u16));
        });
    });

    group(ui, "Addresses · a bool sets it, no argument toggles");
    card(ui, |ui| {
        ui.add_space(8.0);
        for (addr, what) in [
            ("/monadeck/watch", "show / hide the wrist watch"),
            ("/monadeck/watch/mini", "the minimal, clock-only watch"),
            ("/monadeck/dashboard", "the dashboard"),
            ("/monadeck/screens", "every desktop screen"),
            ("/monadeck/keyboard", "the VR keyboard"),
            ("/monadeck/gaming", "gaming mode"),
            ("/monadeck/letgo", "let go of switched-off controllers (else they freeze)"),
            ("/monadeck/notify \"title\" \"body\"", "a toast"),
        ] {
            key_row(ui, addr, what);
        }
        ui.add_space(4.0);
        note(ui, icon::USER, "Avatar parameters work too: MonadeckWatch, MonadeckWatchMini, MonadeckDashboard, MonadeckScreens, MonadeckKeyboard, MonadeckGaming, MonadeckLetGo");
    });
}

// --- widget kit ----------------------------------------------------------------------

fn alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}

/// Blend `a` toward `b` by `t` (0 keeps `a`).
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Eased 0..1 hover amount for a response.
fn hover_t(ui: &egui::Ui, resp: &Response) -> f32 {
    ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.12)
}

/// One line of text, cut with an ellipsis past `max_w`.
fn fit_text(ui: &egui::Ui, text: &str, size: f32, color: Color32, max_w: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), FontId::proportional(size), color, max_w);
    job.wrap = egui::text::TextWrapping { max_width: max_w, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    ui.fonts(|f| f.layout_job(job))
}

/// A small uppercase label above a card.
fn group(ui: &mut egui::Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text.to_uppercase()).size(12.5).strong().color(theme::ON_SURFACE_VAR));
    ui.add_space(-4.0);
}

/// A rounded card holding rows.
fn card(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui)) {
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

fn divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, alpha(Color32::WHITE, 0.07)));
}

/// Title (+ a quieter line, wrapped) on the left, a control of `ctrl_w` on the
/// right, vertically centred. The text never runs under the control.
fn row(ui: &mut egui::Ui, title: &str, sub: &str, ctrl_w: f32, control: impl FnOnce(&mut egui::Ui)) {
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
fn note(ui: &mut egui::Ui, glyph: &str, text: &str) {
    ui.add_space(2.0);
    ui.horizontal_top(|ui| {
        ui.label(egui::RichText::new(glyph).size(14.0).color(theme::ON_SURFACE_VAR));
        ui.add(egui::Label::new(egui::RichText::new(text).size(13.0).color(theme::ON_SURFACE_VAR)).wrap());
    });
    ui.add_space(12.0);
}

/// An animated on/off switch centred at `c`. `t` is the eased 0..1 on-amount.
fn switch(painter: &egui::Painter, c: Pos2, t: f32, hover: f32) {
    let (w, h) = (52.0, 30.0);
    let track = Rect::from_center_size(c, egui::vec2(w, h));
    let off = mix(theme::SURFACE_CONTAINER_HIGH, Color32::from_rgb(56, 66, 78), hover);
    painter.rect_filled(track, CornerRadius::same((h / 2.0) as u8), mix(off, theme::PRIMARY, t));
    let x = track.left() + h / 2.0 + (w - h) * t;
    painter.circle_filled(Pos2::new(x, c.y), h / 2.0 - 4.0, mix(theme::ON_SURFACE_VAR, Color32::WHITE, t));
}

/// Title + wrapped sub on the left, a switch on the right; the whole row is
/// the target. Returns true if the value changed.
fn switch_row(ui: &mut egui::Ui, title: &str, sub: &str, value: &mut bool) -> bool {
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
fn toggle_tile(ui: &mut egui::Ui, size: egui::Vec2, glyph: &str, title: &str, sub: &str, value: &mut bool, enabled: bool) -> bool {
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
fn icon_chip(painter: &egui::Painter, rect: Rect, glyph: &str, lit: f32) {
    let fill = mix(alpha(Color32::WHITE, 0.06), alpha(theme::PRIMARY, 0.22), lit);
    painter.rect_filled(rect, CornerRadius::same((rect.height() * 0.3) as u8), fill);
    let fg = mix(theme::ON_SURFACE_VAR, theme::PRIMARY, lit);
    painter.text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(rect.height() * 0.52), fg);
}

/// A watch quick-button slot: its number, the action's glyph and name.
fn quick_tile(ui: &mut egui::Ui, size: egui::Vec2, n: usize, glyph: &str, label: &str) -> Response {
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
fn segmented(ui: &mut egui::Ui, options: &[&str], selected: usize) -> Option<usize> {
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
fn stepper(ui: &mut egui::Ui, value: &mut f32, min: f32, max: f32, step: f32, fmt: impl Fn(f32) -> String) -> bool {
    let (btn, mid) = (44.0, 96.0);
    let (rect, base) = ui.allocate_exact_size(egui::vec2(btn * 2.0 + mid, btn), Sense::hover());
    let before = *value;
    for (i, glyph, x) in [(0, icon::MINUS, rect.left()), (1, icon::PLUS, rect.right() - btn)] {
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
fn choice_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> Response {
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
fn small_icon(ui: &mut egui::Ui, glyph: &str, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), Sense::click());
    let h = hover_t(ui, &resp);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(12), mix(theme::SURFACE_CONTAINER_HIGH, Color32::from_rgb(56, 66, 78), h));
    p.text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(19.0), mix(theme::ON_SURFACE, Color32::WHITE, h));
    resp.on_hover_text(tip)
}

/// A gesture: the keys on the left, what they do wrapped beside them.
fn gesture_row(ui: &mut egui::Ui, keys: &str, what: &str) {
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
fn key_row(ui: &mut egui::Ui, keys: &str, what: &str) {
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
