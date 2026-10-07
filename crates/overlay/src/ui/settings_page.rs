// Settings: a category list on the left (SteamVR-style), one focused page of
// cards on the right, built from the shared kit.
use egui_phosphor::regular as icon;

use super::kit::*;
use super::{watch_button_info, LibState, CONTROLS};
use crate::gfx::theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsTab {
    Dashboard,
    Watch,
    Controllers,
    Test,
    Models,
    Boundary,
    Gaming,
    Notifications,
    Osc,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 9] =
        [Self::Dashboard, Self::Watch, Self::Controllers, Self::Test, Self::Models, Self::Boundary, Self::Gaming, Self::Notifications, Self::Osc];

    fn glyph(self) -> &'static str {
        match self {
            Self::Dashboard => icon::LAYOUT,
            Self::Watch => icon::WATCH,
            Self::Controllers => icon::HAND_POINTING,
            Self::Test => icon::JOYSTICK,
            Self::Models => icon::CUBE,
            Self::Boundary => icon::POLYGON,
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
            Self::Test => "Controller test",
            Self::Models => "3D models",
            Self::Boundary => "Boundary",
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
            Self::Test => "Every button, stick and sensor, live",
            Self::Models => "Your controllers, trackers and base stations, as SteamVR draws them",
            Self::Boundary => "Walls that show as you near the edge of your play area",
            Self::Gaming => "Controllers as an Xbox pad for flat games",
            Self::Notifications => "Desktop and XSOverlay messages as toasts",
            Self::Osc => "Let games and tools drive the overlay",
        }
    }
}

pub(super) fn settings_page(ui: &mut egui::Ui, st: &mut LibState) {
    // No Boundary on WiVRn: the headset has its own.
    let shown: Vec<SettingsTab> = SettingsTab::ALL.into_iter().filter(|t| *t != SettingsTab::Boundary || st.boundary_possible).collect();
    let tabs: Vec<Tab> = shown.iter().map(|t| Tab { glyph: t.glyph(), label: t.label(), blurb: t.blurb() }).collect();
    let current = shown.iter().position(|t| *t == st.settings_tab).unwrap_or(0);
    let tab = st.settings_tab;
    let picked = shell(ui, icon::GEAR, "Settings", &tabs, current, "settings", |ui| match tab {
        SettingsTab::Dashboard => dashboard(ui, st),
        SettingsTab::Watch => watch(ui, st),
        SettingsTab::Controllers => controllers(ui, st),
        SettingsTab::Test => super::controller_test::controller_test(ui, st),
        SettingsTab::Models => models(ui, st),
        SettingsTab::Boundary => boundary(ui, st),
        SettingsTab::Gaming => gaming(ui, st),
        SettingsTab::Notifications => notifications(ui, st),
        SettingsTab::Osc => osc(ui, st),
    });
    if let Some(i) = picked {
        st.settings_tab = shown[i];
        st.sound_tab = true;
    }
}

// --- pages ---------------------------------------------------------------------------

fn dashboard(ui: &mut egui::Ui, st: &mut LibState) {
    group(ui, "Placement");
    card(ui, |ui| {
        row(ui, "Distance", "How far in front of you it opens", SLIDER_W, |ui| {
            slider(ui, &mut st.panel_dist, 0.8..=2.5, SLIDER_W, |v| format!("{v:.2} m"));
        });
        divider(ui);
        // A stepper, not a slider: a slider would sit on the panel it resizes,
        // and the grab point would jump as it grows under you.
        row(ui, "Size", "Every panel, together", 200.0, |ui| {
            stepper(ui, &mut st.panel_scale, 0.7, 1.4, 0.05, |v| format!("{:.0}%", v * 100.0));
        });
        divider(ui);
        row(ui, "Curve", "1 wraps around you · higher is flatter", SLIDER_W, |ui| {
            slider(ui, &mut st.panel_curve, 1.0..=3.0, SLIDER_W, |v| format!("{v:.2}"));
        });
        divider(ui);
        if switch_row(ui, "Tilt on summon", "Match your headset's pitch instead of standing upright", &mut st.summon_tilt) {
            st.sound_tab = true;
        }
        divider(ui);
        let sub = if st.summon_tilt {
            "Carried, or summoned with your head straight, within a few degrees of level: it straightens out"
        } else {
            "Carried within a few degrees of level, it straightens out"
        };
        if switch_row(ui, "Snap level", sub, &mut st.panel_level) {
            st.sound_tab = true;
        }
        divider(ui);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if button(ui, icon::CROSSHAIR_SIMPLE, "Recenter panel", Tone::Neutral, 150.0).clicked() {
                st.recenter_request = true;
                st.sound_tab = true;
            }
            if button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Reset placement", Tone::Neutral, 150.0).clicked() {
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
            if button(ui, icon::ARROW_CLOCKWISE, "Reload", Tone::Neutral, 150.0).clicked() {
                st.skybox_reload_request = true;
                st.sound_tab = true;
            }
        });
        divider(ui);
        if switch_row(ui, "Dim the game", "While the dashboard is open over it", &mut st.dim_game) {
            st.sound_tab = true;
        }
        divider(ui);
        let enabled = st.dim_game;
        row(ui, "Dimming", "", SLIDER_W, |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                slider(ui, &mut st.dim_strength, 0.1..=0.9, SLIDER_W, |v| format!("{:.0}%", v * 100.0));
            });
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
                slider(ui, &mut st.audio_volume, 0.0..=1.0, SLIDER_W, |v| format!("{:.0}%", v * 100.0));
            });
        });
    });

    group(ui, "Library");
    card(ui, |ui| {
        let sub = format!("{} games · re-scan for new games and artwork", st.games.len());
        row(ui, "Refresh library", &sub, 150.0, |ui| {
            if button(ui, icon::ARROW_CLOCKWISE, "Refresh", Tone::Neutral, 150.0).clicked() {
                st.refresh_request = true;
                st.sound_tab = true;
            }
        });
        if st.uevr_available {
            divider(ui);
            row(ui, "VR Mod injection delay", "Wait after launch before UEVR injects · raise it for slow games", SLIDER_W, |ui| {
                let mut d = st.uevr_delay as f32;
                if slider(ui, &mut d, 0.0..=120.0, SLIDER_W, |v| format!("{v:.0} s")) {
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
            if button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Reset", Tone::Neutral, 150.0).clicked() {
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
                if button(ui, icon::PLUS, "Add zone", Tone::Neutral, 150.0).clicked() {
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
                    st.hold_pose_request = Some(i == 0);
                    st.sound_tab = true;
                }
            });
            note(ui, icon::INFO, "Remembered across Monado restarts · also a watch quick button, and OSC /monadeck/letgo");
        }
        None => {
            note(ui, icon::INFO, "Needs Monadeck's Monado fork (libmonado 1.9) with the service running.");
        }
    });

    group(ui, "Freeze controllers");
    card(ui, |ui| {
        row(ui, "Countdown", "Time to settle into position before System › Monado › Freeze holds a game's hands", SLIDER_W, |ui| {
            slider(ui, &mut st.freeze_delay_secs, 0.0..=10.0, SLIDER_W, |v| if v < 0.5 { "none".into() } else { format!("{v:.0} s") });
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

fn models(ui: &mut egui::Ui, st: &mut LibState) {
    let w = ui.available_width();
    let tile = egui::vec2((w - 12.0) / 2.0, TILE_H);
    let mut t = false;
    ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
    ui.horizontal(|ui| {
        t |= toggle_tile(ui, tile, icon::GAME_CONTROLLER, "Controllers", "In your hands, buttons and all", &mut st.models_controllers, true);
        t |= toggle_tile(ui, tile, icon::HAND, "Hands", "From hand tracking or UdCap gloves", &mut st.models_hands, true);
    });
    ui.horizontal(|ui| {
        t |= toggle_tile(ui, tile, icon::TARGET, "Trackers", "Full-body trackers where they're strapped", &mut st.models_trackers, true);
        let sub = if st.models_stations_possible { "Where they hang, from the room setup" } else { "With SteamVR's Lighthouse driver only" };
        t |= toggle_tile(ui, tile, icon::LIGHTHOUSE, "Base stations", sub, &mut st.models_base_stations, st.models_stations_possible);
    });
    ui.horizontal(|ui| {
        t |= toggle_tile(ui, tile, icon::GRID_FOUR, "Floor grid", "Under the 360° background, while no game runs", &mut st.models_grid, true);
    });
    ui.spacing_mut().item_spacing = egui::vec2(10.0, 12.0);
    ui.add_space(4.0);
    note(ui, icon::INFO, "Shown while the dashboard is open, and the whole time no game runs.");
    if !st.models_available {
        note(ui, icon::WARNING, "SteamVR isn't installed here: the models come from its install. The floor grid and hands still show.");
    }
    if t {
        st.sound_tab = true;
    }
}

fn boundary(ui: &mut egui::Ui, st: &mut LibState) {
    use crate::boundary::Cmd;
    let ask = |st: &mut LibState, cmd: Cmd| {
        st.boundary_request = Some(cmd);
        st.sound_tab = true;
    };
    if let Some((o, problem)) = st.boundary_setup {
        group(ui, "Drawing your boundary");
        card(ui, |ui| {
            ui.add_space(10.0);
            gesture_row(ui, "Trigger click", "Drop a corner under your controller");
            ui.add_space(6.0);
            gesture_row(ui, "Hold the trigger and walk", "Trace a wall, or around furniture");
            ui.add_space(6.0);
            gesture_row(ui, "Back at the first corner", "Close the outline and save it");
            ui.add_space(10.0);
        });
        card(ui, |ui| {
            let (title, sub) = match o.corners {
                0 => ("Nothing yet", "Walk to the edge of your play area".to_string()),
                n => ("Your outline", format!("{n} corner{} · {:.1} m", if n == 1 { "" } else { "s" }, o.length)),
            };
            row(ui, title, &sub, 0.0, |_| {});
            ui.horizontal(|ui| {
                let some = o.corners > 0;
                if button_enabled(ui, icon::ARROW_U_UP_LEFT, "Undo", Tone::Neutral, 110.0, some).clicked() {
                    ask(st, Cmd::Undo);
                }
                if button_enabled(ui, icon::ARROW_COUNTER_CLOCKWISE, "Start over", Tone::Neutral, 130.0, some).clicked() {
                    ask(st, Cmd::Restart);
                }
                if button_enabled(ui, icon::CHECK, "Save", Tone::Primary, 110.0, problem.is_none()).clicked() {
                    ask(st, Cmd::Save);
                }
                if button(ui, icon::X, "Cancel", Tone::Neutral, 110.0).clicked() {
                    ask(st, Cmd::Cancel);
                }
            });
            ui.add_space(10.0);
            if let Some(p) = problem.filter(|_| o.corners >= 3) {
                note(ui, icon::WARNING, p);
            }
        });
        note(ui, icon::INFO, "Aiming at the dashboard doesn't drop corners · grip it to move it out of the way");
        return;
    }
    if !st.boundary_ready {
        empty_state(ui, icon::POLYGON, "No room setup yet", "The boundary is drawn on your floor: set it up first in the desktop app's Lighthouse settings");
        return;
    }
    card(ui, |ui| match st.boundary_info {
        Some(o) => {
            row(ui, "Your boundary", &format!("{} corners · {:.1} m around · {:.1} m²", o.corners, o.length, o.area), 250.0, |ui| {
                let armed = st.is_armed("boundary-clear");
                let (label, tone) = if armed { ("Tap again", Tone::DangerArmed) } else { ("Clear", Tone::Danger) };
                if button(ui, icon::TRASH, label, tone, 110.0).clicked() && st.confirm_tap("boundary-clear") {
                    ask(st, Cmd::Clear);
                }
                if button(ui, icon::PENCIL_SIMPLE, "Redraw", Tone::Neutral, 120.0).clicked() {
                    ask(st, Cmd::Start);
                }
            });
            // Games' play area (SteamVR's files): centred on the room setup's centre.
            if st.models_stations_possible {
                divider(ui);
                play_area_row(ui, st, &ask);
            }
        }
        None => row(ui, "No boundary yet", "Walk the edge of your play area with a controller", 150.0, |ui| {
            if button(ui, icon::PENCIL_SIMPLE, "Draw it", Tone::Primary, 130.0).clicked() {
                ask(st, Cmd::Start);
            }
        }),
    });

    group(ui, "Walls");
    card(ui, |ui| {
        let mut t = switch_row(ui, "Show the walls", "They fade in as you near the edge, and all show once you step out", &mut st.boundary_walls);
        divider(ui);
        row(ui, "Distance", "How close before a wall shows", SLIDER_W, |ui| {
            slider(ui, &mut st.boundary_reach, 0.2..=1.0, SLIDER_W, |v| format!("{v:.2} m"));
        });
        divider(ui);
        t |= switch_row(ui, "Trackers bring them up", "Feet and other trackers count, not only your head and hands", &mut st.boundary_trackers);
        divider(ui);
        t |= switch_row(ui, "Outline on the floor in games", "It's always there while the dashboard is open", &mut st.boundary_floor);
        if t {
            st.sound_tab = true;
        }
    });
    if st.models_stations_possible {
        note(ui, icon::INFO, "Kept with SteamVR's room setup: SteamVR shows this boundary too, and one drawn there shows here");
    }
    note(ui, icon::WARNING, "Monadeck's overlay draws the walls: when it isn't running, there are none");
}

/// The play area games are told about, and the way to make it bigger: move
/// the room's centre into the roomiest spot of the boundary.
fn play_area_row(ui: &mut egui::Ui, st: &mut LibState, ask: &dyn Fn(&mut LibState, crate::boundary::Cmd)) {
    let size = |a: [f32; 2]| format!("{:.1} × {:.1} m", a[0], a[1]);
    let area = |a: Option<[f32; 2]>| a.map_or(0.0, |a| a[0] * a[1]);
    // Worth moving the centre for: a fifth more room, and half a square metre.
    let roomier = st.boundary_roomiest.filter(|r| area(Some(*r)) > area(st.boundary_play) * 1.2 && area(Some(*r)) > area(st.boundary_play) + 0.5);
    let sub = match (st.boundary_play, roomier) {
        (Some(p), Some(r)) => format!("{} around your room's centre · {} with the centre moved into the boundary", size(p), size(r)),
        (Some(p), None) => format!("{} around your room's centre", size(p)),
        (None, Some(r)) => format!("None: your room's centre is outside the boundary · {} with it moved in", size(r)),
        (None, None) => "None fits around your room's centre".to_string(),
    };
    row(ui, "Play area for games", &sub, 160.0, |ui| {
        if roomier.is_some() {
            let armed = st.is_armed("boundary-centre");
            let (label, tone) = if armed { ("Tap again", Tone::DangerArmed) } else { ("Centre it", Tone::Neutral) };
            if button(ui, icon::CROSSHAIR_SIMPLE, label, tone, 140.0).on_hover_text("Moves your room's centre: games start you there").clicked()
                && st.confirm_tap("boundary-centre")
            {
                ask(st, crate::boundary::Cmd::Centre);
            }
        }
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
            slider(ui, &mut st.game_handheld_width, 0.3..=1.2, SLIDER_W, |v| format!("{v:.2} m"));
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
            if button(ui, icon::ARROW_CLOCKWISE, "Reload", Tone::Neutral, 150.0).clicked() {
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
                slider(ui, &mut st.notif_volume, 0.0..=1.0, SLIDER_W, |v| if v < 0.05 { "muted".into() } else { format!("{:.0}%", v * 100.0) });
            });
        });
        divider(ui);
        row(ui, "Try it", "Show a sample notification", 150.0, |ui| {
            if button(ui, icon::BELL_RINGING, "Test", Tone::Neutral, 150.0).clicked() {
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
