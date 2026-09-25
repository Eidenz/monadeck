// System: the timer, the playspace offset, and Monado's running apps, as
// categories of one page built from the shared kit.
use egui::{Align2, Color32, FontId, Sense};
use egui_phosphor::regular as icon;

use super::kit::*;
use super::{LibState, SystemTab};
use crate::gfx::theme;

const FAV_GOLD: Color32 = Color32::from_rgb(255, 200, 70);
const RUNNING_GREEN: Color32 = Color32::from_rgb(90, 220, 120);

const TABS: [(SystemTab, Tab); 3] = [
    (SystemTab::Timer, Tab { glyph: icon::TIMER, label: "Timer", blurb: "A countdown that chimes in the headset, even with the dashboard closed" }),
    (SystemTab::Playspace, Tab { glyph: icon::ARROWS_OUT_CARDINAL, label: "Playspace", blurb: "Nudge where your floor and play area sit" }),
    (SystemTab::Monado, Tab { glyph: icon::STACK, label: "Monado", blurb: "Running apps, which one the headset shows, and frozen hands" }),
];

pub(super) fn system_page(ui: &mut egui::Ui, st: &mut LibState) {
    let tabs: Vec<Tab> = TABS.iter().map(|(_, t)| Tab { glyph: t.glyph, label: t.label, blurb: t.blurb }).collect();
    let current = TABS.iter().position(|(t, _)| *t == st.system_tab).unwrap_or(0);
    let tab = st.system_tab;
    let picked = shell(ui, icon::WRENCH, "System", &tabs, current, "system", |ui| match tab {
        SystemTab::Timer => timer(ui, st),
        SystemTab::Playspace => playspace(ui, st),
        SystemTab::Monado => monado(ui, st),
    });
    if let Some(i) = picked {
        st.system_tab = TABS[i].0;
        st.sound_tab = true;
    }
}

// --- Timer ---------------------------------------------------------------------------

fn timer(ui: &mut egui::Ui, st: &mut LibState) {
    let active = st.timer_running || st.timer_paused;
    card(ui, |ui| {
        ui.add_space(22.0);
        ui.horizontal_top(|ui| {
            ui.add_space(6.0);
            let dim = 270.0;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(dim, dim), Sense::hover());
            let frac = if active { st.timer_remaining as f32 / st.timer_total.max(1) as f32 } else { 1.0 };
            let accent = if st.timer_running {
                theme::PRIMARY
            } else if st.timer_paused {
                FAV_GOLD
            } else {
                Color32::from_rgb(60, 78, 80) // armed, not running
            };
            timer_ring(ui.painter(), rect, frac.clamp(0.0, 1.0), accent);
            // The ring is fixed; the time shrinks (and gains an hours field) so
            // long durations still fit inside it.
            let secs = st.timer_remaining;
            let label = if secs >= 3600 {
                format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
            } else {
                format!("{:02}:{:02}", secs / 60, secs % 60)
            };
            let fs = match label.chars().count() {
                0..=5 => 62.0,
                6 => 52.0,
                7 => 44.0,
                _ => 36.0,
            };
            let p = ui.painter();
            p.text(rect.center() - egui::vec2(0.0, 8.0), Align2::CENTER_CENTER, label, FontId::proportional(fs), Color32::WHITE);
            let status = if st.timer_running {
                "running"
            } else if st.timer_paused {
                "paused"
            } else {
                "ready"
            };
            p.text(rect.center() + egui::vec2(0.0, 44.0), Align2::CENTER_CENTER, status, FontId::proportional(15.0), theme::ON_SURFACE_VAR);

            ui.add_space(40.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
                ui.add_space(20.0);
                if !active {
                    ui.label(egui::RichText::new("ADJUST").size(12.5).strong().color(theme::ON_SURFACE_VAR));
                    ui.horizontal(|ui| {
                        for (label, delta) in [("−1m", -60i64), ("−10s", -10), ("+10s", 10), ("+1m", 60)] {
                            if choice_chip(ui, label, false).clicked() {
                                st.timer_secs = (st.timer_secs as i64 + delta).clamp(0, 86_400) as u32;
                                st.sound_tab = true;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("PRESETS").size(12.5).strong().color(theme::ON_SURFACE_VAR));
                    ui.horizontal(|ui| {
                        for m in [1u32, 5, 10, 30, 60] {
                            let label = if m == 60 { "1h".to_string() } else { format!("{m}m") };
                            if choice_chip(ui, &label, st.timer_secs == m * 60).clicked() {
                                st.timer_secs = m * 60;
                                st.sound_tab = true;
                            }
                        }
                    });
                    ui.add_space(14.0);
                    if button_enabled(ui, icon::PLAY, "Start", Tone::Primary, 300.0, st.timer_secs > 0).clicked() {
                        st.timer_toggle_request = true;
                        st.sound_tab = true;
                    }
                } else {
                    ui.add_space(40.0);
                    ui.horizontal(|ui| {
                        let (glyph, label) = if st.timer_running { (icon::PAUSE, "Pause") } else { (icon::PLAY, "Resume") };
                        if button(ui, glyph, label, Tone::Primary, 170.0).clicked() {
                            st.timer_toggle_request = true;
                            st.sound_tab = true;
                        }
                        if button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Reset", Tone::Neutral, 150.0).clicked() {
                            st.timer_reset_request = true;
                            st.sound_tab = true;
                        }
                    });
                }
            });
        });
        ui.add_space(22.0);
    });
    note(ui, icon::INFO, "When it ends: a chime, a buzz on both hands and a card in front of you · the watch shows the time left");
}

/// A circular track with a progress arc sweeping clockwise from 12 o'clock,
/// rounded at both ends.
fn timer_ring(painter: &egui::Painter, rect: egui::Rect, frac: f32, accent: Color32) {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let center = rect.center();
    let radius = rect.width().min(rect.height()) * 0.5 - 14.0;
    let width = 15.0;
    painter.circle_stroke(center, radius, egui::Stroke::new(width, Color32::from_rgb(34, 40, 48)));
    if frac <= 0.0 {
        return;
    }
    let start = -FRAC_PI_2;
    let sweep = frac * TAU;
    let n = 96;
    let at = |a: f32| egui::pos2(center.x + radius * a.cos(), center.y + radius * a.sin());
    let pts: Vec<egui::Pos2> = (0..=n).map(|i| at(start + sweep * (i as f32 / n as f32))).collect();
    painter.add(egui::Shape::line(pts, egui::Stroke::new(width, accent)));
    painter.circle_filled(at(start), width * 0.5, accent);
    painter.circle_filled(at(start + sweep), width * 0.5, accent);
}

// --- Playspace ------------------------------------------------------------------------

fn playspace(ui: &mut egui::Ui, st: &mut LibState) {
    let editing_game = st.ps_target_game && st.ps_game_active;

    // Per-game vs global: only while a game runs (that's when an override applies).
    if st.ps_game_active {
        group(ui, "Adjusting");
        card(ui, |ui| {
            let game: String = if st.ps_game_name.chars().count() > 18 {
                format!("{}…", st.ps_game_name.chars().take(18).collect::<String>())
            } else {
                st.ps_game_name.clone()
            };
            row(ui, "Offset for", "The game's own offset replaces the global one while it runs", 330.0, |ui| {
                if let Some(i) = segmented(ui, &[game.as_str(), "Global"], if st.ps_target_game { 0 } else { 1 }) {
                    st.ps_target_game = i == 0;
                    st.sound_tab = true;
                }
            });
        });
    }

    group(ui, if editing_game { "Offset · this game" } else { "Offset" });
    card(ui, |ui| {
        row(ui, "Step", "How far each tap moves or turns", 560.0, |ui| {
            let turns = [5.0_f32, 15.0, 45.0];
            let cur = turns.iter().position(|d| (st.playspace_yaw_step - d).abs() < 0.1).unwrap_or(usize::MAX);
            if let Some(i) = segmented(ui, &["5°", "15°", "45°"], cur) {
                st.playspace_yaw_step = turns[i];
                st.sound_tab = true;
            }
            ui.add_space(10.0);
            let moves = [0.01_f32, 0.05, 0.10];
            let cur = moves.iter().position(|m| (st.playspace_step - m).abs() < 0.001).unwrap_or(usize::MAX);
            if let Some(i) = segmented(ui, &["1 cm", "5 cm", "10 cm"], cur) {
                st.playspace_step = moves[i];
                st.sound_tab = true;
            }
        });
        divider(ui);
        let (step, ystep) = (st.playspace_step, st.playspace_yaw_step);
        let mut bumped = false;
        {
            let (x, y, z, yaw) = if editing_game {
                (&mut st.ps_game_x, &mut st.ps_game_y, &mut st.ps_game_z, &mut st.ps_game_yaw)
            } else {
                (&mut st.playspace_x, &mut st.playspace_y, &mut st.playspace_z, &mut st.playspace_yaw)
            };
            let axes: [(&str, &str, &mut f32, f32); 3] = [
                ("Height", "Raise or lower your floor", y, 2.0),
                ("Forward / back", "Slide the play area toward or away from you", z, 3.0),
                ("Left / right", "Slide it sideways", x, 3.0),
            ];
            for (title, sub, v, lim) in axes {
                row(ui, title, sub, 220.0, |ui| {
                    bumped |= stepper_with(ui, v, -lim, lim, step, (icon::MINUS, icon::PLUS), 128.0, fmt_m);
                });
                divider(ui);
            }
            row(ui, "Rotate", "Turn the play area around you", 220.0, |ui| {
                // Unbounded here; wrapped into ±180° just below.
                if stepper_with(ui, yaw, -1.0e6, 1.0e6, ystep, (icon::ARROW_COUNTER_CLOCKWISE, icon::ARROW_CLOCKWISE), 128.0, fmt_deg) {
                    *yaw = wrap_deg(*yaw);
                    bumped = true;
                }
            });
        }
        if bumped {
            st.sound_tab = true;
            if editing_game {
                st.ps_game_override = true;
                st.ps_game_save_request = true;
            }
        }
        divider(ui);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if button(ui, icon::CROSSHAIR_SIMPLE, "Recenter", Tone::Primary, 170.0).on_hover_text("Recenter to your current head pose").clicked() {
                st.recenter_playspace_request = true;
                st.sound_tab = true;
            }
            let reset = if editing_game { "Use global" } else { "Reset offset" };
            if button(ui, icon::ARROW_COUNTER_CLOCKWISE, reset, Tone::Neutral, 170.0).clicked() {
                if editing_game {
                    (st.ps_game_x, st.ps_game_y, st.ps_game_z, st.ps_game_yaw) = (0.0, 0.0, 0.0, 0.0);
                    st.ps_game_override = false;
                    st.ps_game_clear_request = true;
                    st.flash(format!("{} uses the global offset", st.ps_game_name));
                } else {
                    (st.playspace_x, st.playspace_y, st.playspace_z, st.playspace_yaw) = (0.0, 0.0, 0.0, 0.0);
                    st.flash("Playspace offset reset");
                }
            }
        });
        ui.add_space(10.0);
        note(
            ui,
            icon::INFO,
            if editing_game {
                "Replaces your global offset only while this game runs · kept across restarts"
            } else {
                "Kept across restarts, and re-applied when the runtime comes back"
            },
        );
    });

    group(ui, "Drag");
    card(ui, |ui| {
        let mut t = false;
        let hint = match st.gloves {
            (true, true) => "Gloves on both hands: hold A + B",
            (true, false) => "Left glove: A + B · right: trackpad",
            (false, true) => "Right glove: A + B · left: trackpad",
            _ => "Hold the trackpad and move your hand · twice snaps back",
        };
        row(ui, "Hold to move", hint, 400.0, |ui| {
            let ids = ["off", "left", "right", "both"];
            let cur = ids.iter().position(|i| st.ps_drag_hands == *i).unwrap_or(usize::MAX);
            if let Some(i) = segmented(ui, &["Off", "Left", "Right", "Both"], cur) {
                st.ps_drag_hands = ids[i].into();
                t = true;
            }
        });
        divider(ui);
        row(ui, "Button", "Auto: the trackpad, or A + B on a glove", 330.0, |ui| {
            let ids = ["auto", "pad", "ab"];
            let cur = ids.iter().position(|i| st.ps_drag_button == *i).unwrap_or(usize::MAX);
            if let Some(i) = segmented(ui, &["Auto", "Trackpad", "A + B"], cur) {
                st.ps_drag_button = ids[i].into();
                t = true;
            }
        });
        divider(ui);
        t |= switch_row(ui, "Up and down too", "Off keeps it horizontal (no accidental floor changes)", &mut st.ps_drag_vertical);
        divider(ui);
        t |= switch_row(ui, "Overlays follow you", "Screens, keyboard, photos and the dashboard keep their place around you", &mut st.ps_drag_follow);
        divider(ui);
        let o = st.ps_drag_offset;
        let moved = o.iter().any(|v| v.abs() > 1e-4);
        let sub = if moved {
            format!("{:+.2} m · {:+.2} m · {:+.2} m on top of the offset · this session only", o[0], o[1], o[2])
        } else {
            "Nothing yet · drags last this session only".to_string()
        };
        row(ui, "Dragged so far", &sub, 170.0, |ui| {
            if button_enabled(ui, icon::ARROW_COUNTER_CLOCKWISE, "Snap back", Tone::Neutral, 150.0, moved).clicked() {
                st.ps_drag_reset_request = true;
                t = true;
            }
        });
        if t {
            st.sound_tab = true;
        }
    });
}

/// Wrap an angle (degrees) into (-180, 180].
fn wrap_deg(mut v: f32) -> f32 {
    while v > 180.0 {
        v -= 360.0;
    }
    while v <= -180.0 {
        v += 360.0;
    }
    v
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

// --- Monado -----------------------------------------------------------------------------

fn monado(ui: &mut egui::Ui, st: &mut LibState) {
    if let Some(hold) = st.hold_pose {
        group(ui, "When a controller switches off");
        card(ui, |ui| {
            row(ui, "In-game hands", "Freeze: they stay where they were · Let go: the game sees them off (avatar poses take over)", 250.0, |ui| {
                if let Some(i) = segmented(ui, &["Freeze", "Let go"], (!hold) as usize) {
                    st.hold_pose_request = Some(i == 0);
                    st.sound_tab = true;
                }
            });
            note(ui, icon::INFO, "Remembered across Monado restarts · also a watch button, and OSC /monadeck/letgo");
        });
    }

    group(ui, "Running apps");
    let clients = st.monado_clients.clone();
    if clients.is_empty() {
        card(ui, |ui| empty_state(ui, icon::STACK, "No apps running", "Start a game and it shows up here"));
        return;
    }
    card(ui, |ui| {
        for (i, c) in clients.iter().enumerate() {
            if i > 0 {
                divider(ui);
            }
            let sub = match (c.is_primary, c.is_app) {
                (true, _) => "What the headset shows",
                (false, true) => "Running",
                (false, false) => "Backgrounded",
            };
            // Right to left: Kill, then Freeze, then Set active.
            row(ui, &c.name, sub, 560.0, |ui| {
                let key = format!("kill:{}", c.id);
                let armed = st.is_armed(&key);
                let (label, tone) = if armed { ("Sure?", Tone::DangerArmed) } else { ("Kill", Tone::Danger) };
                if button(ui, icon::X, label, tone, 110.0).on_hover_text("Close this app (two taps)").clicked() && st.confirm_tap(&key) {
                    st.kill_request = Some(c.name.clone());
                    st.flash(format!("Closing {}…", c.name));
                }
                // Freeze only on the fork (stock Monado lacks it). It counts down
                // first (Settings › Controllers) so you can settle; tapping during
                // the countdown cancels.
                if st.monado_freeze_supported {
                    let pending = st.freeze_pending.and_then(|(pid, secs)| (pid == c.id).then_some(secs));
                    let (glyph, label, tone) = if c.frozen {
                        (icon::SNOWFLAKE, "Unfreeze hands".to_string(), Tone::Active)
                    } else if let Some(secs) = pending {
                        (icon::HOURGLASS, format!("Cancel ({}s)", secs.ceil() as u32), Tone::Active)
                    } else {
                        (icon::SNOWFLAKE, "Freeze hands".to_string(), Tone::Neutral)
                    };
                    if button(ui, glyph, &label, tone, 200.0).clicked() {
                        st.freeze_toggle_request = Some(c.id);
                        st.sound_tab = true;
                    }
                }
                if c.is_primary {
                    badge(ui, "Active", RUNNING_GREEN);
                } else if button(ui, icon::MONITOR, "Set active", Tone::Neutral, 150.0).clicked() {
                    st.set_active_request = Some(c.id);
                    st.sound_tab = true;
                }
            });
        }
    });
    note(ui, icon::INFO, &format!("Freeze holds that app's hands where they are while everything else keeps tracking · {:.0} s countdown (Settings › Controllers)", st.freeze_delay_secs));
}
