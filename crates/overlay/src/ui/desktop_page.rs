// Desktop: the desktop viewer's screens, layouts, look, input and behaviour,
// as categories of one page built from the shared kit.
use egui_phosphor::regular as icon;

use super::kit::*;
use super::LibState;
use crate::gfx::theme;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DesktopTab {
    Screens,
    Layouts,
    Display,
    Input,
    Behaviour,
}

impl DesktopTab {
    pub const ALL: [DesktopTab; 5] = [Self::Screens, Self::Layouts, Self::Display, Self::Input, Self::Behaviour];

    fn tab(self) -> Tab {
        match self {
            Self::Screens => Tab { glyph: icon::MONITOR, label: "Screens", blurb: "Which monitors VR shows, in which order" },
            Self::Layouts => Tab { glyph: icon::SQUARES_FOUR, label: "Layouts", blurb: "Named arrangements of your screens and keyboard" },
            Self::Display => Tab { glyph: icon::SUN_DIM, label: "Display", blurb: "How new screens start out, and how bright they are" },
            Self::Input => Tab { glyph: icon::CURSOR_CLICK, label: "Mouse & keyboard", blurb: "Clicks, scrolling and the VR keyboard" },
            Self::Behaviour => Tab { glyph: icon::GAUGE, label: "Behaviour", blurb: "Hiding, restoring and capture performance" },
        }
    }
}

pub(super) fn desktop_page(ui: &mut egui::Ui, st: &mut LibState) {
    let tabs: Vec<Tab> = DesktopTab::ALL.iter().map(|t| t.tab()).collect();
    let current = DesktopTab::ALL.iter().position(|t| *t == st.desktop_tab).unwrap_or(0);
    let tab = st.desktop_tab;
    let picked = shell(ui, icon::MONITOR, "Desktop", &tabs, current, "desktop", |ui| match tab {
        DesktopTab::Screens => screens(ui, st),
        DesktopTab::Layouts => layouts(ui, st),
        DesktopTab::Display => display(ui, st),
        DesktopTab::Input => input(ui, st),
        DesktopTab::Behaviour => behaviour(ui, st),
    });
    if let Some(i) = picked {
        st.desktop_tab = DesktopTab::ALL[i];
        st.sound_tab = true;
    }
}

fn screens(ui: &mut egui::Ui, st: &mut LibState) {
    card(ui, |ui| {
        if !st.desktop_ready {
            let (glyph, label) = if st.desktop_pending { (icon::HOURGLASS, "Waiting…") } else { (icon::MONITOR, "Set up") };
            row(ui, "Set up screens", "Approve the monitors in the desktop's share dialog, once", 170.0, |ui| {
                if button_enabled(ui, glyph, label, Tone::Primary, 150.0, !st.desktop_pending).clicked() {
                    st.desktop_setup_request = true;
                    st.sound_tab = true;
                }
            });
            divider(ui);
        }
        if st.desktop_rows.is_empty() {
            empty_state(ui, icon::MONITOR, "No screens detected", &st.desktop_status.clone());
        }
        let approved = st.desktop_rows.iter().filter(|r| r.approved).count();
        let (mut mv, mut opacity) = (None, None);
        for (i, r) in st.desktop_rows.iter().enumerate() {
            if i > 0 {
                divider(ui);
            }
            let state = if !r.approved { "not approved" } else if r.shown { "shown" } else { "hidden" };
            let sub = match &r.hint {
                Some(h) => format!("{} · {h}", r.detail),
                None => format!("{} · {state}", r.detail),
            };
            let title = format!("{}  {}", if r.shown { icon::EYE } else { icon::EYE_CLOSED }, r.name);
            let mut op = r.opacity;
            row(ui, &title, &sub, 420.0, |ui| {
                if r.approved {
                    // Place in the bottom bar: ◀ / ▶ (left = earlier).
                    if icon_btn(ui, icon::CARET_RIGHT, "Later in the bottom bar", false, i + 1 < approved).clicked() {
                        mv = Some((i, 1));
                    }
                    if icon_btn(ui, icon::CARET_LEFT, "Earlier in the bottom bar", false, i > 0).clicked() {
                        mv = Some((i, -1));
                    }
                    ui.add_space(16.0);
                    let cs = st.desktop_color_scale;
                    ui.add_enabled_ui(cs, |ui| {
                        stepper(ui, &mut op, 0.2, 1.0, 0.1, |v| format!("{:.0}%", v * 100.0));
                    });
                    ui.label(egui::RichText::new("opacity").size(13.0).color(theme::ON_SURFACE_VAR));
                }
            });
            if r.approved && (op - r.opacity).abs() > 1e-3 {
                opacity = Some((i, op));
            }
        }
        if let Some(m) = mv {
            st.desktop_move_request = Some(m);
            st.sound_tab = true;
        }
        if opacity.is_some() {
            st.desktop_opacity_request = opacity;
        }
        if st.desktop_ready {
            divider(ui);
            row(ui, "Re-pick screens", "Choose again which monitors VR may show, in the desktop's share dialog", 200.0, |ui| {
                let armed = st.is_armed("repick");
                let (label, tone) = if armed { ("Tap again", Tone::DangerArmed) } else { ("Re-pick", Tone::Neutral) };
                if button(ui, icon::ARROW_COUNTER_CLOCKWISE, label, tone, 150.0).clicked() && st.confirm_tap("repick") {
                    st.desktop_reselect_request = true;
                    st.flash("Pick your screens in the desktop dialog");
                }
            });
        }
    });
    note(ui, icon::INFO, &format!("{} · show or hide screens from the bottom bar or the watch", st.desktop_status));
}

fn layouts(ui: &mut egui::Ui, st: &mut LibState) {
    card(ui, |ui| {
        row(ui, "Save what's up now", "Every screen's place, size and curve, plus the keyboard", 230.0, |ui| {
            if button(ui, icon::PLUS, "Save as new", Tone::Primary, 190.0).clicked() {
                st.naming = true;
                st.naming_layout = true;
                st.name_buf.clear();
                st.keyboard_open = true;
                st.sound_tab = true;
            }
        });
    });
    group(ui, "Saved");
    let now = ui.input(|i| i.time);
    if st.layout_delete_arm.is_some_and(|(_, t)| now - t > 3.0) {
        st.layout_delete_arm = None;
    }
    card(ui, |ui| {
        if st.layouts.is_empty() {
            empty_state(ui, icon::SQUARES_FOUR, "No layouts yet", "Arrange your screens, then save it here (e.g. Standing, Lying down)");
        }
        let n = st.layouts.len();
        let (mut apply, mut overwrite, mut delete, mut arm, mut rename, mut mv, mut follow_toggle) = (None, None, None, None, None, None, None);
        for (i, (name, shown)) in st.layouts.iter().enumerate() {
            if i > 0 {
                divider(ui);
            }
            let active = st.layout_active.as_deref() == Some(name.as_str());
            let follow = st.layout_follow.get(i).copied().unwrap_or(false);
            let sub = format!(
                "{shown} screen{} shown{}",
                if *shown == 1 { "" } else { "s" },
                if follow { " · comes back where you look" } else { "" }
            );
            row(ui, name, &sub, 590.0, |ui| {
                let armed = st.layout_delete_arm.is_some_and(|(j, _)| j == i);
                if icon_btn(ui, icon::TRASH, if armed { "Tap again to delete" } else { "Delete" }, armed, true).clicked() {
                    if armed {
                        delete = Some(i);
                    } else {
                        arm = Some(i);
                    }
                }
                if small_icon(ui, icon::FLOPPY_DISK, "Save what's up now over this layout").clicked() {
                    overwrite = Some(i);
                }
                if small_icon(ui, icon::PENCIL_SIMPLE, "Rename").clicked() {
                    rename = Some(i);
                }
                if icon_btn(ui, icon::CARET_DOWN, "Move down", false, i + 1 < n).clicked() {
                    mv = Some((i, 1));
                }
                if icon_btn(ui, icon::CARET_UP, "Move up", false, i > 0).clicked() {
                    mv = Some((i, -1));
                }
                ui.add_space(8.0);
                if choice_chip(ui, "Follows head", follow)
                    .on_hover_text("Double-B off / on brings it back centred on your head, like an unsaved arrangement")
                    .clicked()
                {
                    follow_toggle = Some((i, !follow));
                }
                ui.add_space(8.0);
                let (label, tone) = if active { ("Active", Tone::Active) } else { ("Apply", Tone::Neutral) };
                if button(ui, icon::PLAY, label, tone, 120.0).clicked() {
                    apply = Some(i);
                }
            });
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
        if follow_toggle.is_some() {
            st.layout_follow_toggle = follow_toggle;
            st.sound_tab = true;
        }
    });
    group(ui, "On start");
    card(ui, |ui| {
        let mut t = switch_row(ui, "Restore the last layout", "Bring the screens back where they were", &mut st.restore_layout);
        if st.restore_layout {
            divider(ui);
            t |= switch_row(ui, "Start hidden", "Loaded but out of sight until a double-B (left hand) brings it up", &mut st.restore_layout_hidden);
        }
        if t {
            st.sound_tab = true;
        }
    });
}

fn display(ui: &mut egui::Ui, st: &mut LibState) {
    let cs = st.desktop_color_scale;
    group(ui, "New screens");
    card(ui, |ui| {
        row(ui, "Width", "Screens you haven't resized follow it · grip + trigger, push/pull resizes one", 200.0, |ui| {
            stepper(ui, &mut st.screen_width_m, 0.6, 3.0, 0.1, |v| format!("{v:.1} m"));
        });
        divider(ui);
        let max_deg = crate::desktop::screen::MAX_CURVE_ANGLE.to_degrees();
        row(ui, "Curve", "Flat to wrapped · grip + trigger + stick left/right curves one screen", SLIDER_W, |ui| {
            slider(ui, &mut st.screen_curve, 0.0..=1.0, SLIDER_W, move |v| if v < 0.01 { "flat".into() } else { format!("{:.0}°", v * max_deg) });
        });
        divider(ui);
        row(ui, "Opacity", if cs { "Each screen has its own too (Screens)" } else { "This runtime can't fade layers" }, 200.0, |ui| {
            ui.add_enabled_ui(cs, |ui| {
                stepper(ui, &mut st.screen_opacity, 0.2, 1.0, 0.1, |v| format!("{:.0}%", v * 100.0));
            });
        });
        divider(ui);
        row(ui, "Distance", "How far in front of you a screen appears when you show it", SLIDER_W, |ui| {
            slider(ui, &mut st.screen_spawn_dist, 0.6..=3.0, SLIDER_W, |v| format!("{v:.2} m"));
        });
    });
    group(ui, "Brightness & tint · every screen");
    card(ui, |ui| {
        if !cs {
            note(ui, icon::INFO, "This runtime can't scale layer colours (XR_KHR_composition_layer_color_scale_bias)");
        }
        ui.add_enabled_ui(cs, |ui| {
            row(ui, "Brightness", "Handy lying down at night", SLIDER_W, |ui| {
                slider(ui, &mut st.screen_brightness, 0.2..=1.0, SLIDER_W, |v| format!("{:.0}%", v * 100.0));
            });
            divider(ui);
            row(ui, "Warm tint", "Less blue, like a night light", SLIDER_W, |ui| {
                slider(ui, &mut st.screen_warmth, 0.0..=1.0, SLIDER_W, |v| if v < 0.01 { "off".into() } else { format!("{:.0}%", v * 100.0) });
            });
            divider(ui);
            let presets = [("Normal", 1.0_f32, 0.0_f32), ("Dim", 0.6, 0.0), ("Night", 0.55, 0.7)];
            let cur = presets
                .iter()
                .position(|(_, b, w)| (st.screen_brightness - b).abs() < 0.01 && (st.screen_warmth - w).abs() < 0.01)
                .unwrap_or(usize::MAX);
            row(ui, "Presets", "", 320.0, |ui| {
                if let Some(i) = segmented(ui, &["Normal", "Dim", "Night"], cur) {
                    (st.screen_brightness, st.screen_warmth) = (presets[i].1, presets[i].2);
                    st.sound_tab = true;
                }
            });
        });
    });
}

fn input(ui: &mut egui::Ui, st: &mut LibState) {
    group(ui, "Mouse");
    card(ui, |ui| {
        row(ui, "B button", "Frozen click: a left click that never moves the cursor · Middle click: paste, open in a new tab · A stays the right click", 340.0, |ui| {
            if let Some(i) = segmented(ui, &["Frozen click", "Middle click"], st.mouse_b_middle as usize) {
                st.mouse_b_middle = i == 1;
                st.sound_tab = true;
            }
        });
        divider(ui);
        row(ui, "Scroll speed", "Thumbstick scrolling on a screen", SLIDER_W, |ui| {
            slider(ui, &mut st.scroll_speed, 0.25..=4.0, SLIDER_W, |v| format!("{v:.2}×"));
        });
        divider(ui);
        row(ui, "Drag threshold", "Trigger-held motion below this stays a click; past it, it's a drag", SLIDER_W, |ui| {
            slider(ui, &mut st.drag_threshold_px, 0.0..=60.0, SLIDER_W, |v| format!("{v:.0} px"));
        });
    });
    group(ui, "Keyboard");
    card(ui, |ui| {
        let mut t = switch_row(ui, "Follows screens and text fields", "Hides and returns with its docked screen; pops up under the last-used screen when a text field gets focus", &mut st.keyboard_auto);
        divider(ui);
        row(ui, "Size", "Also saved in layouts", 200.0, |ui| {
            stepper(ui, &mut st.keyboard_scale, 0.6, 1.6, 0.1, |v| format!("{:.0}%", v * 100.0));
        });
        divider(ui);
        t |= switch_row(ui, "Haptics", "A light tick on each key, a lighter one sliding between keys", &mut st.keyboard_haptics);
        divider(ui);
        let sub = format!("{} · follows the desktop (switch it from the keyboard's top bar)", st.keyboard_layout);
        row(ui, "Layout", &sub, 0.0, |_| {});
        if t {
            st.sound_tab = true;
        }
    });
}

fn behaviour(ui: &mut egui::Ui, st: &mut LibState) {
    group(ui, "Hide & restore");
    card(ui, |ui| {
        let mut t = switch_row(
            ui,
            "Double-B restore follows your head",
            "One screen or a docked group comes back centred in view; several loose screens keep their place around you",
            &mut st.recenter_on_toggle,
        );
        divider(ui);
        t |= switch_row(ui, "Tilt restored screens to your headset", "Off keeps them upright", &mut st.screen_restore_tilt);
        if t {
            st.sound_tab = true;
        }
    });
    group(ui, "Performance");
    card(ui, |ui| {
        if switch_row(ui, "Pause capture when not looking", "Frees GPU and CPU after ~2 s out of view; resumes the moment you look back", &mut st.gaze_pause) {
            st.sound_tab = true;
        }
        divider(ui);
        let mut fps = st.capture_max_fps as f32;
        row(ui, "Frame-rate cap", "Frames above the headset's rate are never seen; capping saves compositor work", 240.0, |ui| {
            stepper_with(ui, &mut fps, 0.0, 240.0, 30.0, (icon::MINUS, icon::PLUS), 136.0, |v| if v < 1.0 { "unlimited".into() } else { format!("{v:.0} fps") });
        });
        st.capture_max_fps = fps.round() as u32;
        divider(ui);
        let mut mh = st.capture_max_height as f32;
        row(ui, "Resolution cap", "Downscale mirrored screens in VR", 240.0, |ui| {
            stepper_with(ui, &mut mh, 0.0, 2160.0, 360.0, (icon::MINUS, icon::PLUS), 136.0, |v| if v < 1.0 { "native".into() } else { format!("{v:.0} px") });
        });
        st.capture_max_height = mh.round() as u32;
    });
    group(ui, "Status");
    card(ui, |ui| {
        let cap = if st.desktop_dmabuf { "GPU zero-copy (DMA-BUF)" } else { "CPU copy (SHM), slower" };
        row(ui, "Capture", cap, 0.0, |_| {});
        divider(ui);
        match &st.desktop_hid_error {
            None => row(ui, "Mouse and keyboard", "Virtual input device ready (uinput)", 0.0, |_| {}),
            Some(e) => {
                let sub = format!("{e} · add yourself to the input group and log in again");
                row(ui, "Mouse and keyboard unavailable", &sub, 0.0, |_| {});
            }
        }
        divider(ui);
        let shown = format!("{} screen{} in VR", st.desktop_shown, if st.desktop_shown == 1 { "" } else { "s" });
        row(ui, "Shown", &shown, 0.0, |_| {});
    });
}
