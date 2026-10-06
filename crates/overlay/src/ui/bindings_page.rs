// Controller bindings: SteamVR's binding editor in the dashboard's kit. The
// list holds Monadeck's own controls and every game that ships SteamVR Input
// bindings; one opens on the controller in hand with its action sets as tabs,
// each hand's inputs in a column beside the controller drawings (hovering an
// input or its part of a drawing links the two), and what every input does in
// the game's own words. A game's binding saves as xrizer's personal binding,
// never the game's file; Monadeck's own apply at once.
use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, UiBuilder};
use egui_phosphor::regular as icon;
use monadeck_core::bindings::{self as core, ActionKind, BindableGame, Hand, InputDef, Mode, Source};

use super::{draw_art, kit, LibState, RUNNING_GREEN};
use crate::bindings::{controller_for, Editor, Leave, Popup, Subject, Target};
use crate::games::{ArtState, LibGame};
use crate::gfx::{glyph, theme};

const AMBER: Color32 = Color32::from_rgb(240, 180, 70);
const RED: Color32 = Color32::from_rgb(232, 96, 96);
/// The controller drawings' box, each hand.
const ART_W: f32 = 214.0;
const ART_H: f32 = 300.0;

pub(super) fn bindings_page(ui: &mut egui::Ui, st: &mut LibState) {
    if let Some(msg) = st.binds.notice.take() {
        st.flash(msg);
    }
    if !st.binds.scanned && !st.binds.scanning {
        st.binds.scan();
    }
    if st.binds.editor.is_some() {
        st.visible_now.clear();
        st.hovered_index = None;
        editor_view(ui, st);
    } else {
        list_view(ui, st);
    }
}

/// The library game a bindable game is (for its cover art and running state).
fn lib_index(games: &[LibGame], bg: &BindableGame) -> Option<usize> {
    games.iter().position(|g| (g.app_id.is_some() && g.app_id == bg.app_id) || (g.shortcut_id.is_some() && g.shortcut_id == bg.shortcut_id))
}

// --- the game list ----------------------------------------------------------------------------

fn list_view(ui: &mut egui::Ui, st: &mut LibState) {
    let customized = st.binds.games.iter().filter(|g| !g.personal.is_empty()).count();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(icon::GAME_CONTROLLER).size(28.0).color(theme::PRIMARY));
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Controller bindings").size(28.0).strong().color(Color32::WHITE));
        ui.add_space(14.0);
        ui.label(egui::RichText::new("What each button does in your SteamVR games").size(15.0).color(theme::ON_SURFACE_VAR));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if kit::icon_btn(ui, icon::ARROWS_CLOCKWISE, "Look for games again", false, !st.binds.scanning).clicked() {
                st.binds.scan();
            }
            ui.add_space(12.0);
            let custom = format!("Customized · {customized}");
            if let Some(i) = kit::segmented(ui, &["All games", custom.as_str()], st.binds.only_personal as usize) {
                st.binds.only_personal = i == 1;
                st.sound_tab = true;
            }
        });
    });
    ui.add_space(16.0);
    if own_card(ui, st).clicked() {
        st.binds.open(Target::Monadeck, None);
        st.sound_select = true;
    }
    ui.add_space(18.0);
    ui.label(egui::RichText::new("GAMES").size(12.5).strong().color(theme::ON_SURFACE_VAR));
    ui.add_space(8.0);
    if let Some(err) = st.binds.error.clone() {
        ui.label(egui::RichText::new(format!("{}  {err}", icon::WARNING)).size(14.0).color(RED));
        ui.add_space(8.0);
    }
    if st.binds.games.is_empty() {
        if st.binds.scanning || !st.binds.scanned {
            ui.add_space(80.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(28.0));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Looking for games with controller bindings…").color(theme::ON_SURFACE_VAR));
            });
        } else {
            kit::empty_state(ui, icon::GAME_CONTROLLER, "No games with controller bindings yet", "Games made for SteamVR Input show up here once they're installed");
        }
        return;
    }
    let q = st.search.trim().to_lowercase();
    let shown: Vec<usize> = (0..st.binds.games.len())
        .filter(|&i| {
            let g = &st.binds.games[i];
            (!st.binds.only_personal || !g.personal.is_empty()) && (q.is_empty() || g.name.to_lowercase().contains(&q))
        })
        .collect();
    if shown.is_empty() {
        if st.binds.only_personal && q.is_empty() {
            kit::empty_state(ui, icon::SPARKLE, "Nothing customized yet", "Open a game and save a change: it shows up here");
        } else {
            kit::empty_state(ui, icon::MAGNIFYING_GLASS, "No game matches", "Try another name");
        }
        return;
    }
    let opening = st.binds.opening.map(|(t, _)| t);
    let (mut visible, mut picked, mut hovered) = (Vec::new(), None, None);
    let missing = ArtState::Missing;
    egui::ScrollArea::vertical().id_salt("bind-games").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(16.0, 18.0);
            for &i in &shown {
                let bg = &st.binds.games[i];
                let lib = lib_index(&st.games, bg);
                let art = lib.map_or(&missing, |l| &st.games[l].cover);
                let running = lib.is_some() && lib == st.running_index;
                let r = bind_tile(ui, bg, art, running, opening == Some(Target::Game(i)));
                if let Some(l) = lib {
                    if ui.is_rect_visible(r.rect) {
                        visible.push(l);
                    }
                    if r.hovered() {
                        hovered = Some(l);
                    }
                }
                if r.clicked() {
                    picked = Some(i);
                }
            }
        });
        ui.add_space(22.0);
        missing_note(ui);
        ui.add_space(12.0);
    });
    st.visible_now = visible;
    st.hovered_index = hovered;
    if let Some(i) = picked {
        st.binds.open(Target::Game(i), None);
        st.sound_select = true;
    }
}

/// Monadeck's own controls, above the games: a wide card.
fn own_card(ui: &mut egui::Ui, st: &LibState) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 84.0), Sense::click());
    let h = kit::hover_t(ui, &resp);
    let opening = st.binds.opening.is_some_and(|(t, _)| t == Target::Monadeck);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(18), kit::mix(theme::SURFACE_CONTAINER, Color32::from_rgb(40, 52, 60), h * 0.6));
    p.rect_stroke(rect, CornerRadius::same(18), Stroke::new(1.0, kit::mix(kit::alpha(Color32::WHITE, 0.05), kit::alpha(theme::PRIMARY, 0.5), h)), StrokeKind::Inside);
    let chip = Rect::from_center_size(Pos2::new(rect.left() + 46.0, rect.center().y), egui::vec2(52.0, 52.0));
    kit::icon_chip(p, chip, icon::ARROWS_OUT_CARDINAL, 1.0);
    p.text(Pos2::new(rect.left() + 88.0, rect.center().y - 12.0), Align2::LEFT_CENTER, "Monadeck", FontId::proportional(19.0), Color32::WHITE);
    p.text(Pos2::new(rect.left() + 88.0, rect.center().y + 14.0), Align2::LEFT_CENTER, "Its own controls: the dashboard, your screens, the mouse on them, the playspace drag", FontId::proportional(14.0), theme::ON_SURFACE_VAR);
    let mut x = rect.right() - 24.0;
    p.text(Pos2::new(x, rect.center().y), Align2::RIGHT_CENTER, icon::CARET_RIGHT, FontId::proportional(18.0), kit::mix(theme::ON_SURFACE_VAR, Color32::WHITE, h));
    x -= 34.0;
    if opening {
        ui.put(Rect::from_center_size(Pos2::new(x - 12.0, rect.center().y), egui::vec2(24.0, 24.0)), egui::Spinner::new().size(22.0));
    } else if !st.binds.own_personal.is_empty() {
        let g = p.layout_no_wrap(format!("{}  Customized", icon::SPARKLE), FontId::proportional(12.5), theme::PRIMARY);
        let r = Rect::from_min_size(Pos2::new(x - g.size().x - 20.0, rect.center().y - 13.0), egui::vec2(g.size().x + 20.0, 26.0));
        p.rect_filled(r, CornerRadius::same(13), kit::alpha(theme::PRIMARY, 0.14));
        p.galley(Pos2::new(r.left() + 10.0, r.center().y - g.size().y / 2.0), g, theme::PRIMARY);
    }
    resp
}

/// Why a VR game might not be in the list.
fn missing_note(ui: &mut egui::Ui) {
    let text = format!(
        "{}  Not seeing a game? Only games made for SteamVR Input have bindings to change. Games that run on OpenXR directly \
         (Phasmophobia, Bonelab…) and older SteamVR games come with fixed controls.",
        icon::INFO
    );
    ui.add(egui::Label::new(egui::RichText::new(text).size(13.5).color(theme::ON_SURFACE_VAR)).wrap());
}

const TILE_W: f32 = 168.0;
const TILE_H: f32 = 252.0;

fn bind_tile(ui: &mut egui::Ui, g: &BindableGame, art: &ArtState, running: bool, opening: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(TILE_W, TILE_H + 32.0), Sense::click());
    let h = kit::hover_t(ui, &resp);
    let cover = Rect::from_min_size(rect.min, egui::vec2(TILE_W, TILE_H)).expand(h * 4.0);
    if h > 0.001 {
        ui.painter().rect_filled(cover.translate(egui::vec2(0.0, 6.0)).expand(2.0), CornerRadius::same(18), Color32::from_black_alpha((h * 110.0) as u8));
    }
    draw_art(ui, cover, art, &g.name, 16.0);
    let p = ui.painter();
    p.rect_stroke(cover, CornerRadius::same(16), Stroke::new(1.0, kit::mix(Color32::from_white_alpha(14), Color32::from_white_alpha(70), h)), StrokeKind::Inside);
    let pill = |y: f32, text: &str, glyph: &str, fg: Color32| {
        let g = p.layout_no_wrap(format!("{glyph}  {text}"), FontId::proportional(12.5), fg);
        let r = Rect::from_min_size(Pos2::new(cover.left() + 10.0, y), egui::vec2(g.size().x + 20.0, 26.0));
        p.rect_filled(r, CornerRadius::same(13), Color32::from_black_alpha(175));
        p.rect_stroke(r, CornerRadius::same(13), Stroke::new(1.0, kit::alpha(fg, 0.4)), StrokeKind::Inside);
        p.galley(Pos2::new(r.left() + 10.0, r.center().y - g.size().y / 2.0), g, fg);
    };
    if running {
        pill(cover.top() + 10.0, "Playing", icon::PLAY, RUNNING_GREEN);
    }
    if !g.personal.is_empty() {
        pill(cover.bottom() - 36.0, "Customized", icon::SPARKLE, theme::PRIMARY);
    }
    if opening {
        p.rect_filled(cover, CornerRadius::same(16), Color32::from_black_alpha(140));
        let s = Rect::from_center_size(cover.center(), egui::vec2(30.0, 30.0));
        ui.put(s, egui::Spinner::new().size(30.0).color(Color32::WHITE));
    }
    let fg = kit::mix(theme::ON_SURFACE_VAR, Color32::WHITE, h);
    let cap = kit::fit_text(ui, &g.name, 14.0, fg, TILE_W - 4.0);
    ui.painter().galley(Pos2::new(rect.left() + 2.0, rect.top() + TILE_H + 9.0), cap, fg);
    resp.on_hover_text(&g.name)
}

// --- the editor -------------------------------------------------------------------------------

/// What the editor asked for this frame, done once it's back in `st.binds`.
enum Cmd {
    Close,
    Save,
    SaveThen(Leave),
    Reset,
    Open(&'static str),
}

/// Per-frame layout the columns share with the drawings.
#[derive(Default)]
struct Frame {
    /// Each input's list entry: where its line starts, and whether it's in view.
    anchors: Vec<((Hand, String), Pos2, bool)>,
    /// Each input's spot on the drawings.
    spots: Vec<((Hand, String), Pos2)>,
    /// The input under the pointer this frame.
    hover: Option<(Hand, String)>,
}

fn editor_view(ui: &mut egui::Ui, st: &mut LibState) {
    let Some(mut e) = st.binds.editor.take() else { return };
    let page = ui.available_rect_before_wrap();
    ui.allocate_rect(page, Sense::hover());
    let mut cmd = None;
    let mut fr = Frame::default();

    let header = Rect::from_min_size(page.min, egui::vec2(page.width(), 56.0));
    let mut y = header.bottom() + 12.0;
    // Only xrizer chokes on those; Monadeck skips what it can't read.
    let unreadable = if e.own() { Vec::new() } else { e.doc.unreadable_modes() };
    let banner = (!unreadable.is_empty()).then(|| {
        let r = Rect::from_min_size(Pos2::new(page.left(), y), egui::vec2(page.width(), 64.0));
        y = r.bottom() + 12.0;
        r
    });
    let tabs = Rect::from_min_size(Pos2::new(page.left(), y), egui::vec2(page.width(), 52.0));
    let footer = Rect::from_min_max(Pos2::new(page.left(), page.bottom() - 50.0), page.max);
    let body = Rect::from_min_max(Pos2::new(page.left(), tabs.bottom() + 16.0), Pos2::new(page.right(), footer.top() - 14.0));

    let subj = st.binds.subject(e.target);
    let game_name = subj.as_ref().map_or(String::new(), |s| s.name.clone());
    let running = match e.target {
        Target::Game(i) => st.binds.games.get(i).and_then(|g| lib_index(&st.games, g)).is_some_and(|l| st.running_index == Some(l)),
        Target::Monadeck => false,
    };
    header_ui(&mut ui.new_child(UiBuilder::new().max_rect(header)), &mut e, subj.as_ref(), &mut cmd);
    if let Some(r) = banner {
        unreadable_banner(&mut ui.new_child(UiBuilder::new().max_rect(r)), &mut e, &unreadable);
    }
    set_tabs(&mut ui.new_child(UiBuilder::new().max_rect(tabs)), &mut e, st);

    let center_w = ART_W * 2.0 + 16.0;
    let gap = 22.0;
    let side_w = ((body.width() - center_w) / 2.0 - gap).max(300.0);
    let left = Rect::from_min_size(body.min, egui::vec2(side_w, body.height()));
    let right = Rect::from_min_size(Pos2::new(body.right() - side_w, body.top()), egui::vec2(side_w, body.height()));
    let center = Rect::from_min_max(Pos2::new(left.right() + gap, body.top()), Pos2::new(right.left() - gap, body.bottom()));
    let popup_open = e.popup.is_some();
    hand_column(&mut ui.new_child(UiBuilder::new().max_rect(left)), &mut e, Hand::Left, &mut fr);
    hand_column(&mut ui.new_child(UiBuilder::new().max_rect(right)), &mut e, Hand::Right, &mut fr);
    // The lists have scrolled to it; a tap on the drawing asks again below.
    e.scroll_to = None;
    center_column(&mut ui.new_child(UiBuilder::new().max_rect(center)), &mut e, st, &mut fr);
    if !popup_open {
        leader_line(ui, &fr, body);
        e.hover = fr.hover.clone();
    }
    footer_ui(&mut ui.new_child(UiBuilder::new().max_rect(footer)), &mut e, st, &game_name, running, &mut cmd);
    popups(ui.ctx(), &mut e, st, subj.as_ref(), &mut cmd);

    st.binds.editor = Some(e);
    match cmd {
        Some(Cmd::Close) => st.binds.close(),
        Some(Cmd::Save) => st.binds.save(),
        Some(Cmd::SaveThen(leave)) => {
            st.binds.after_save = Some(leave);
            st.binds.save();
        }
        Some(Cmd::Reset) => st.binds.reset(),
        Some(Cmd::Open(ty)) => {
            if let Some(target) = st.binds.editor.as_ref().map(|e| e.target) {
                st.binds.open(target, Some(ty));
            }
        }
        None => {}
    }
}

pub(super) fn controller_glyph(ty: &str, hand: Hand) -> &'static str {
    match (ty, hand) {
        ("knuckles" | "udcap_gloves", Hand::Left) => glyph::INDEX_LEFT,
        ("knuckles" | "udcap_gloves", Hand::Right) => glyph::INDEX_RIGHT,
        ("oculus_touch", Hand::Left) => glyph::QUEST_LEFT,
        ("oculus_touch", Hand::Right) => glyph::QUEST_RIGHT,
        _ => icon::GAME_CONTROLLER,
    }
}

fn header_ui(ui: &mut egui::Ui, e: &mut Editor, subj: Option<&Subject>, cmd: &mut Option<Cmd>) {
    let rect = ui.max_rect();
    ui.horizontal_centered(|ui| {
        if kit::button(ui, icon::ARROW_LEFT, "Back", kit::Tone::Neutral, 0.0).clicked() {
            if e.dirty() {
                e.popup = Some(Popup::Leave(Leave::ToList));
            } else {
                *cmd = Some(Cmd::Close);
            }
        }
    });
    let name = subj.map_or("", |s| s.name.as_str());
    let text_left = rect.left() + 140.0;
    let title = kit::fit_text(ui, name, 25.0, Color32::WHITE, rect.width() - 150.0 - 330.0);
    let p = ui.painter();
    p.galley(Pos2::new(text_left, rect.top() + 2.0), title, Color32::WHITE);
    let (whose, fg) = match (e.personal, e.own()) {
        (true, _) => ("Your personal binding", theme::PRIMARY),
        (false, true) => ("Monadeck's default controls", theme::ON_SURFACE_VAR),
        (false, false) => ("The game's own binding", theme::ON_SURFACE_VAR),
    };
    let sub = e.doc.name().filter(|_| !e.personal).map_or(whose.to_string(), |n| format!("{whose} · {n}"));
    p.galley(Pos2::new(text_left, rect.top() + 34.0), kit::fit_text(ui, &sub, 14.0, fg, rect.width() - 150.0 - 330.0), fg);

    // The controller being edited; a menu when the game ships more than one.
    let many = subj.is_some_and(|s| s.controllers.len() > 1);
    let label = format!("{}   {}", controller_glyph(e.ctrl.ty, Hand::Right), e.ctrl.name);
    let g = ui.fonts(|f| f.layout_no_wrap(label.clone(), FontId::proportional(16.0), Color32::WHITE));
    let w = g.size().x + if many { 64.0 } else { 36.0 };
    let r = Rect::from_min_size(Pos2::new(rect.right() - w, rect.center().y - 23.0), egui::vec2(w, 46.0));
    let resp = ui.interact(r, ui.id().with("ctrl-menu"), if many { Sense::click() } else { Sense::hover() });
    let h = if many { kit::hover_t(ui, &resp) } else { 0.0 };
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(13), kit::mix(theme::SURFACE_CONTAINER_HIGH, Color32::from_rgb(56, 66, 78), h));
    p.galley(Pos2::new(r.left() + 18.0, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
    if many {
        p.text(Pos2::new(r.right() - 22.0, r.center().y), Align2::CENTER_CENTER, icon::CARET_DOWN, FontId::proportional(15.0), theme::ON_SURFACE_VAR);
        if resp.on_hover_text("Edit another controller's binding").clicked() {
            e.popup = Some(Popup::Controller);
        }
    }
}

fn unreadable_banner(ui: &mut egui::Ui, e: &mut Editor, bad: &[(String, usize, String)]) {
    let rect = ui.max_rect();
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(16), Color32::from_rgb(52, 30, 32));
    p.rect_stroke(rect, CornerRadius::same(16), Stroke::new(1.0, kit::alpha(RED, 0.5)), StrokeKind::Inside);
    p.text(Pos2::new(rect.left() + 30.0, rect.center().y), Align2::CENTER_CENTER, icon::WARNING, FontId::proportional(24.0), RED);
    let mut modes: Vec<&str> = bad.iter().map(|b| b.2.as_str()).collect();
    modes.sort_unstable();
    modes.dedup();
    let n = bad.len();
    p.text(Pos2::new(rect.left() + 58.0, rect.top() + 20.0), Align2::LEFT_CENTER, "xrizer can't read this binding", FontId::proportional(16.5), Color32::WHITE);
    let why = format!(
        "{n} input{} use{} a mode it doesn't know ({}), so it ignores the whole file and the game gets no controls from it.",
        if n == 1 { "" } else { "s" },
        if n == 1 { "s" } else { "" },
        modes.join(", ")
    );
    p.galley(Pos2::new(rect.left() + 58.0, rect.top() + 34.0), kit::fit_text(ui, &why, 13.5, theme::ON_SURFACE_VAR, rect.width() - 58.0 - 200.0), Color32::WHITE);
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(egui::vec2(16.0, 9.0))).layout(Layout::right_to_left(Align::Center)));
    if kit::button(&mut child, icon::WRENCH, "Remove them", kit::Tone::Primary, 0.0).on_hover_text("Drop those inputs, then save to make it readable").clicked() {
        e.doc.drop_unreadable_modes();
    }
}

/// The action sets as tabs, each with how many inputs it uses; one with a
/// required action nothing drives gets an amber mark.
fn set_tabs(ui: &mut egui::Ui, e: &mut Editor, st: &mut LibState) {
    let rect = ui.max_rect();
    let tabs: Vec<(String, String, usize, Vec<String>)> = e
        .sets()
        .iter()
        .map(|s| (s.key.clone(), s.name.clone(), e.doc.bound_count(&s.key), e.unbound_required(&s.key).iter().map(|a| a.name.clone()).collect()))
        .collect();
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(15), Color32::from_rgb(22, 26, 32));
    if tabs.is_empty() {
        p.text(rect.center(), Align2::CENTER_CENTER, "This game lists no actions", FontId::proportional(15.0), theme::ON_SURFACE_VAR);
        return;
    }
    let (pad, gap) = (5.0, 5.0);
    let n = tabs.len() as f32;
    let w = (rect.width() - pad * 2.0 - gap * (n - 1.0)) / n;
    for (i, (key, name, count, missing)) in tabs.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(rect.left() + pad + i as f32 * (w + gap), rect.top() + pad), egui::vec2(w, rect.height() - pad * 2.0));
        let resp = ui.interact(r, ui.id().with(("set-tab", i)), Sense::click());
        let h = kit::hover_t(ui, &resp);
        let on = ui.ctx().animate_bool_with_time(resp.id.with("on"), *key == e.set, 0.14);
        let p = ui.painter();
        p.rect_filled(r, CornerRadius::same(11), kit::mix(kit::alpha(Color32::WHITE, 0.06 * h), theme::PRIMARY, on));
        let fg = kit::mix(kit::mix(theme::ON_SURFACE, Color32::WHITE, h), Color32::BLACK, on);
        // The count on the right, the name centred in what's left.
        let badge = count.to_string();
        let bg = p.layout_no_wrap(badge, FontId::proportional(13.0), fg);
        let bw = bg.size().x.max(10.0) + 14.0;
        let br = Rect::from_min_size(Pos2::new(r.right() - bw - 10.0, r.center().y - 11.0), egui::vec2(bw, 22.0));
        p.rect_filled(br, CornerRadius::same(11), kit::mix(kit::alpha(Color32::WHITE, 0.08), Color32::from_black_alpha(40), on));
        p.galley(Pos2::new(br.center().x - bg.size().x / 2.0, br.center().y - bg.size().y / 2.0), bg, fg);
        let text_w = r.width() - bw - 30.0 - if missing.is_empty() { 0.0 } else { 18.0 };
        let ng = kit::fit_text(ui, name, 16.0, fg, text_w);
        let nx = (r.left() + 12.0).max(r.left() + (r.width() - bw - 10.0 - ng.size().x) / 2.0);
        ui.painter().galley(Pos2::new(nx, r.center().y - ng.size().y / 2.0), ng, fg);
        if !missing.is_empty() {
            ui.painter().circle_filled(Pos2::new(br.left() - 12.0, r.center().y), 4.5, AMBER);
        }
        let tip = if missing.is_empty() {
            format!("{name}: {count} input{} in use", if *count == 1 { "" } else { "s" })
        } else {
            format!("{name}: not on any input yet — {}", missing.join(", "))
        };
        if resp.on_hover_text(tip).clicked() && *key != e.set {
            e.set = key.clone();
            st.sound_tab = true;
        }
    }
}

/// "Left trigger", "Left A button".
fn input_label(hand: Hand, label: &str) -> String {
    let first_word = label.split_whitespace().next().unwrap_or("");
    let label = if first_word.chars().count() > 1 {
        let mut cs = label.chars();
        cs.next().map(|c| c.to_lowercase().chain(cs).collect()).unwrap_or_default()
    } else {
        label.to_string()
    };
    format!("{} {label}", if hand == Hand::Left { "Left" } else { "Right" })
}

/// One hand's inputs, each with its "use as" cards; inputs the controller
/// doesn't list (a pinch, a finger) close the list, flagged.
fn hand_column(ui: &mut egui::Ui, e: &mut Editor, hand: Hand, fr: &mut Frame) {
    let rect = ui.max_rect();
    let title = format!("{}   {} controller", controller_glyph(e.ctrl.ty, hand), if hand == Hand::Left { "Left" } else { "Right" });
    ui.painter().text(Pos2::new(if hand == Hand::Left { rect.left() + 2.0 } else { rect.right() - 2.0 }, rect.top() + 12.0), if hand == Hand::Left { Align2::LEFT_CENTER } else { Align2::RIGHT_CENTER }, title, FontId::proportional(16.0), theme::ON_SURFACE_VAR);
    let list = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + 32.0), rect.max);
    let mut child = ui.new_child(UiBuilder::new().max_rect(list));
    let sources: Vec<Source> = e.doc.sources(&e.set).into_iter().filter(|s| s.hand == Some(hand)).collect();
    let chords = e.doc.chords(&e.set);
    // Each binding (and set) keeps its own scroll.
    let salt = ("bind-col", hand.id(), format!("{:?}", e.target), e.ctrl.ty, e.set.clone());
    egui::ScrollArea::vertical().id_salt(salt).auto_shrink([false, false]).show(&mut child, |ui| {
        ui.set_width(list.width() - 14.0);
        ui.spacing_mut().item_spacing.y = 8.0;
        for def in e.ctrl.inputs.iter().filter(|d| d.on(hand)) {
            let cards: Vec<&Source> = sources.iter().filter(|s| s.input == def.id).collect();
            input_section(ui, e, hand, def.id, def.label, Some(def), &cards, &chords, fr);
        }
        let mut others: Vec<&str> = sources.iter().filter(|s| e.ctrl.input(&s.input).is_none()).map(|s| s.input.as_str()).collect();
        others.dedup();
        for id in others {
            let cards: Vec<&Source> = sources.iter().filter(|s| s.input == id).collect();
            let label = core::pretty_name(id);
            input_section(ui, e, hand, id, &label, None, &cards, &chords, fr);
        }
        ui.add_space(12.0);
    });
}

#[allow(clippy::too_many_arguments)]
fn input_section(ui: &mut egui::Ui, e: &mut Editor, hand: Hand, id: &str, label: &str, def: Option<&InputDef>, cards: &[&Source], chords: &[core::Chord], fr: &mut Frame) {
    let top = ui.cursor().top();
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 44.0), Sense::hover());
    let key = (hand, id.to_string());
    let lit = ui.ctx().animate_bool_with_time(ui.id().with(("in-hover", hand.id(), id)), e.hover.as_ref() == Some(&key), 0.12);
    let bindable = def.is_some_and(|d| d.bindable());
    let p = ui.painter();
    let fg = if bindable { kit::mix(Color32::WHITE, theme::PRIMARY, lit) } else { theme::ON_SURFACE_VAR };
    p.text(Pos2::new(rect.left() + 2.0, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(19.0), fg);
    p.hline(rect.x_range(), rect.bottom() - 1.0, Stroke::new(1.0 + lit, kit::mix(kit::alpha(Color32::WHITE, 0.12), theme::PRIMARY, lit)));
    if bindable {
        let plus = Rect::from_min_size(Pos2::new(rect.right() - 38.0, rect.center().y - 19.0), egui::vec2(38.0, 38.0));
        let resp = ui.interact(plus, ui.id().with(("add", hand.id(), id)), Sense::click());
        let h = kit::hover_t(ui, &resp);
        let p = ui.painter();
        p.rect_filled(plus, CornerRadius::same(11), kit::mix(theme::SURFACE_CONTAINER_HIGH, theme::PRIMARY, h));
        p.text(plus.center(), Align2::CENTER_CENTER, icon::PLUS, FontId::proportional(18.0), kit::mix(theme::ON_SURFACE, Color32::BLACK, h));
        if resp.on_hover_text(format!("Use the {} for something", input_label(hand, label).to_lowercase())).clicked() {
            e.popup = Some(Popup::AddMode { hand, input: id.to_string() });
        }
    } else {
        let note = def.map_or(if e.own() { "Monadeck can't read this input" } else { "xrizer ignores this input" }, |d| d.note);
        let g = p.layout_no_wrap(note.to_string(), FontId::proportional(12.5), theme::ON_SURFACE_VAR);
        let r = Rect::from_min_size(Pos2::new(rect.right() - g.size().x - 20.0, rect.center().y - 12.0), egui::vec2(g.size().x + 20.0, 24.0));
        p.rect_filled(r, CornerRadius::same(12), kit::alpha(Color32::WHITE, 0.06));
        p.galley(Pos2::new(r.left() + 10.0, r.center().y - g.size().y / 2.0), g, theme::ON_SURFACE_VAR);
    }
    if e.scroll_to.as_ref() == Some(&key) {
        ui.scroll_to_rect(rect, Some(Align::Center));
    }
    for s in cards {
        source_card(ui, e, def, s);
    }
    // Chords this input is part of: what they do, and with what.
    let mut chorded = false;
    for c in chords.iter().filter(|c| c.inputs.iter().any(|i| i.hand == Some(hand) && i.input == id)) {
        let others: Vec<String> = c
            .inputs
            .iter()
            .filter(|i| !(i.hand == Some(hand) && i.input == id))
            .map(|i| {
                let l = e.ctrl.input(&i.input).map_or_else(|| core::pretty_name(&i.input), |d| d.label.to_string());
                if i.hand == Some(hand) { l } else { i.hand.map_or(l.clone(), |h| input_label(h, &l)) }
            })
            .collect();
        let text = if others.is_empty() { e.manifest.action_name(&c.action) } else { format!("{}  ·  with {}", e.manifest.action_name(&c.action), others.join(" + ")) };
        // A game's chords are only shown: xrizer doesn't read them.
        if chord_row(ui, &text, !e.own()).clicked() && e.own() {
            e.popup = Some(Popup::Chords);
        }
        chorded = true;
    }
    if !cards.is_empty() || chorded {
        ui.add_space(4.0);
    }
    let section = Rect::from_min_max(Pos2::new(rect.left(), top), Pos2::new(rect.right(), ui.cursor().top()));
    if ui.rect_contains_pointer(section) {
        fr.hover = Some(key.clone());
    }
    let anchor = if hand == Hand::Left { Pos2::new(rect.right() + 8.0, rect.center().y) } else { Pos2::new(rect.left() - 8.0, rect.center().y) };
    fr.anchors.push((key, anchor, ui.is_rect_visible(rect.shrink(8.0))));
}

/// A chord an input is in, under its entry: tapping opens the chords. An
/// `ignored` one (a game's: xrizer doesn't read chords) is only shown, flagged.
fn chord_row(ui: &mut egui::Ui, text: &str, ignored: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), if ignored { Sense::hover() } else { Sense::click() });
    let h = if ignored { 0.0 } else { kit::hover_t(ui, &resp) };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(14), kit::mix(theme::SURFACE_CONTAINER, Color32::from_rgb(40, 52, 60), h * 0.6));
    p.text(Pos2::new(rect.left() + 16.0, rect.center().y), Align2::LEFT_CENTER, "CHORD", FontId::proportional(12.5), theme::ON_SURFACE_VAR);
    let fg = if ignored { theme::ON_SURFACE_VAR } else { Color32::WHITE };
    let mut right = rect.right() - 16.0;
    if ignored {
        let g = p.layout_no_wrap("xrizer ignores this".to_string(), FontId::proportional(11.5), AMBER);
        let r = Rect::from_min_size(Pos2::new(right - g.size().x - 16.0, rect.center().y - 10.0), egui::vec2(g.size().x + 16.0, 20.0));
        p.rect_filled(r, CornerRadius::same(10), kit::alpha(AMBER, 0.16));
        p.galley(Pos2::new(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, AMBER);
        right = r.left() - 10.0;
    } else {
        p.text(Pos2::new(rect.left() + 76.0, rect.center().y), Align2::CENTER_CENTER, icon::LINK_SIMPLE, FontId::proportional(15.0), theme::PRIMARY);
        p.text(Pos2::new(right, rect.center().y), Align2::CENTER_CENTER, icon::CARET_RIGHT, FontId::proportional(13.0), kit::alpha(theme::ON_SURFACE_VAR, 0.4 + 0.6 * h));
        right -= 16.0;
    }
    let left = rect.left() + if ignored { 80.0 } else { 96.0 };
    let g = kit::fit_text(ui, text, 15.0, fg, right - left);
    ui.painter().galley(Pos2::new(left, rect.center().y - g.size().y / 2.0), g, fg);
    resp.on_hover_text(if ignored { "Buttons held together: xrizer doesn't read chords, so this does nothing" } else { "Buttons held together" })
}

fn mode_title(mode: &str) -> String {
    match Mode::from_id(mode) {
        Some(Mode::None) => "DOES NOTHING".into(),
        Some(m) => format!("USE AS {}", m.label().to_uppercase()),
        None => format!("USE AS {}", mode.replace('_', " ").to_uppercase()),
    }
}

fn source_card(ui: &mut egui::Ui, e: &mut Editor, def: Option<&InputDef>, s: &Source) {
    let mode = Mode::from_id(&s.mode);
    let editable = mode.is_some() && def.is_some_and(|d| d.bindable());
    // The mode's slots this input has, then anything else the file binds.
    let mut rows: Vec<(String, String, Option<&'static str>)> = Vec::new();
    if let Some(m) = mode {
        for sl in m.slots().iter().filter(|sl| def.is_none_or(|d| d.has_slot(sl))) {
            rows.push((sl.key.to_string(), sl.label.to_string(), editable.then_some(sl.key)));
        }
    }
    for (k, _) in &s.outputs {
        if !rows.iter().any(|(rk, _, _)| rk == k) {
            rows.push((k.clone(), core::pretty_name(k), None));
        }
    }
    egui::Frame::default()
        .fill(theme::SURFACE_CONTAINER)
        .stroke(Stroke::new(1.0, kit::alpha(Color32::WHITE, 0.05)))
        .corner_radius(14)
        .inner_margin(egui::Margin { left: 14, right: 8, top: 8, bottom: 8 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let (head, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), Sense::hover());
            let p = ui.painter();
            let title_g = p.layout_no_wrap(mode_title(&s.mode), FontId::proportional(12.5), theme::ON_SURFACE_VAR);
            let title_w = title_g.size().x;
            p.galley(Pos2::new(head.left(), head.center().y - title_g.size().y / 2.0), title_g, theme::ON_SURFACE_VAR);
            // Why it doesn't work, if it doesn't.
            let flag = if mode.is_none() {
                Some((if e.own() { "Monadeck ignores this" } else { "xrizer can't read this" }, RED))
            } else if def.is_none() {
                Some((if e.own() { "Monadeck ignores this" } else { "xrizer ignores this" }, AMBER))
            } else {
                None
            };
            if let Some((text, c)) = flag {
                let g = p.layout_no_wrap(text.to_string(), FontId::proportional(11.5), c);
                let r = Rect::from_min_size(Pos2::new(head.left() + title_w + 10.0, head.center().y - 10.0), egui::vec2(g.size().x + 16.0, 20.0));
                p.rect_filled(r, CornerRadius::same(10), kit::alpha(c, 0.16));
                p.galley(Pos2::new(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, c);
            }
            let mut x = head.right();
            let mut small = |ui: &mut egui::Ui, glyph: &str, tip: &str, danger: bool| {
                let r = Rect::from_min_size(Pos2::new(x - 32.0, head.center().y - 16.0), egui::vec2(32.0, 32.0));
                x -= 36.0;
                let resp = ui.interact(r, ui.id().with((s.index, glyph)), Sense::click());
                let h = kit::hover_t(ui, &resp);
                let accent = if danger { RED } else { Color32::WHITE };
                ui.painter().rect_filled(r, CornerRadius::same(9), kit::alpha(accent, 0.12 * h));
                ui.painter().text(r.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(16.0), kit::mix(kit::alpha(theme::ON_SURFACE_VAR, 0.8), accent, h));
                resp.on_hover_text(tip).clicked()
            };
            if small(ui, icon::TRASH, "Remove", true) {
                e.remove(s.index);
                return;
            }
            if editable && def.is_some_and(|d| d.modes.len() > 1) && small(ui, icon::PENCIL_SIMPLE, "Use it another way", false) {
                e.popup = Some(Popup::ChangeMode { index: s.index });
            }
            for (key, label, slot) in &rows {
                let action = s.output(key);
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), if slot.is_some() { Sense::click() } else { Sense::hover() });
                let h = if slot.is_some() { kit::hover_t(ui, &resp) } else { 0.0 };
                let p = ui.painter();
                if h > 0.001 {
                    p.rect_filled(rect.expand2(egui::vec2(6.0, 0.0)), CornerRadius::same(9), kit::alpha(Color32::WHITE, 0.06 * h));
                }
                let label_w = 118.0;
                p.galley(Pos2::new(rect.left() + 4.0, rect.center().y - 9.0), kit::fit_text(ui, label, 15.0, theme::ON_SURFACE_VAR, label_w - 8.0), Color32::WHITE);
                let (text, fg) = match action {
                    Some(a) => (e.manifest.action_name(a), if slot.is_some() { Color32::WHITE } else { theme::ON_SURFACE_VAR }),
                    None => ("None".to_string(), kit::alpha(theme::ON_SURFACE_VAR, 0.55)),
                };
                let caret = if slot.is_some() { 22.0 } else { 4.0 };
                let g = kit::fit_text(ui, &text, 15.0, kit::mix(fg, Color32::WHITE, h), rect.width() - label_w - caret);
                ui.painter().galley(Pos2::new(rect.left() + label_w, rect.center().y - g.size().y / 2.0), g, Color32::WHITE);
                if slot.is_some() {
                    ui.painter().text(Pos2::new(rect.right() - 8.0, rect.center().y), Align2::CENTER_CENTER, icon::CARET_RIGHT, FontId::proportional(13.0), kit::alpha(theme::ON_SURFACE_VAR, 0.4 + 0.6 * h));
                }
                if let (Some(slot), true) = (slot, resp.clicked()) {
                    e.popup = Some(Popup::Action { index: s.index, slot });
                }
            }
        });
}

// --- the drawings -----------------------------------------------------------------------------

pub(super) fn art_texture(ctx: &egui::Context, cache: &mut std::collections::HashMap<&'static str, egui::TextureHandle>, ty: &'static str) -> Option<egui::TextureHandle> {
    if let Some(t) = cache.get(ty) {
        return Some(t.clone());
    }
    let bytes: &[u8] = match core::art(ty)? {
        "index" => include_bytes!("../../assets/bindings/index.png"),
        "touch" => include_bytes!("../../assets/bindings/touch.png"),
        "vive" => include_bytes!("../../assets/bindings/vive.png"),
        _ => return None,
    };
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let tex = ctx.load_texture(format!("bind-art-{ty}"), egui::ColorImage::from_rgba_unmultiplied(size, &img.into_raw()), egui::TextureOptions::LINEAR);
    cache.insert(ty, tex.clone());
    Some(tex)
}

fn center_column(ui: &mut egui::Ui, e: &mut Editor, st: &mut LibState, fr: &mut Frame) {
    let rect = ui.max_rect();
    let ty = e.ctrl.ty;
    let tex = art_texture(ui.ctx(), &mut st.binds.art, ty);
    let sources = e.doc.sources(&e.set);
    let now = ui.input(|i| i.time);
    let mut dwelling = None;
    for (i, hand) in [Hand::Left, Hand::Right].into_iter().enumerate() {
        let bx = Rect::from_min_size(Pos2::new(rect.center().x - ART_W - 8.0 + i as f32 * (ART_W + 16.0), rect.top()), egui::vec2(ART_W, ART_H));
        let Some(tex) = &tex else { continue };
        let [tw, th] = tex.size();
        let s = (bx.width() / tw as f32).min(bx.height() / th as f32);
        let art = Rect::from_center_size(bx.center(), egui::vec2(tw as f32 * s, th as f32 * s));
        let uv = if hand == Hand::Left { Rect::from_min_max(Pos2::new(1.0, 0.0), Pos2::new(0.0, 1.0)) } else { Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)) };
        ui.painter().image(tex.id(), art, uv, kit::alpha(theme::ON_SURFACE, 0.92));
        for &(id, fx, fy, fr_r) in core::spots(ty) {
            let Some(def) = e.ctrl.input(id).filter(|d| d.on(hand)) else { continue };
            let x = if hand == Hand::Left { 1.0 - fx } else { fx };
            let c = Pos2::new(art.left() + x * art.width(), art.top() + fy * art.height());
            let r = (fr_r * art.width()).max(7.0);
            let key = (hand, id.to_string());
            fr.spots.push((key.clone(), c));
            let resp = ui.interact(Rect::from_center_size(c, egui::vec2(r * 2.0 + 10.0, r * 2.0 + 10.0)), ui.id().with(("spot", hand.id(), id)), Sense::click());
            if resp.hovered() {
                fr.hover = Some(key.clone());
                // Held there a moment with its entry out of view: bring it in, so
                // the link line can reach it.
                let since = match &e.spot_dwell {
                    Some((k, t)) if *k == key => *t,
                    _ => now,
                };
                dwelling = Some((key.clone(), since));
                let in_view = fr.anchors.iter().any(|(k, _, visible)| *k == key && *visible);
                if now - since > 0.3 && !in_view {
                    e.scroll_to = Some(key.clone());
                }
            }
            // Nothing marks the drawing until an input is hovered, here or in
            // the lists: then its part lights up and a line links the two.
            let lit = ui.ctx().animate_bool_with_time(resp.id.with("lit"), e.hover.as_ref() == Some(&key), 0.12);
            let p = ui.painter();
            if lit > 0.01 {
                p.circle_filled(c, r * 1.5, kit::alpha(theme::PRIMARY, 0.16 * lit));
                p.circle_stroke(c, r, Stroke::new(2.0, kit::alpha(theme::PRIMARY, lit)));
            }
            if ty == "oculus_touch" && matches!(id, "a" | "b" | "x" | "y") {
                // Touch buttons carry their letter (the drawing has none, as it's mirrored).
                p.text(c, Align2::CENTER_CENTER, id.to_uppercase(), FontId::proportional(12.0), kit::mix(theme::ON_SURFACE_VAR, theme::PRIMARY, lit));
            }
            if resp.on_hover_text(input_label(hand, def.label)).clicked() {
                e.scroll_to = Some(key);
                let used = sources.iter().any(|s| s.hand == Some(hand) && s.input == id);
                if def.bindable() && !used {
                    e.popup = Some(Popup::AddMode { hand, input: id.to_string() });
                }
            }
        }
    }

    e.spot_dwell = dwelling;

    // Under the drawings: mirror mode, haptics / poses (a game's) or chords
    // (Monadeck's own), and what's left unbound.
    let mut y = rect.top() + ART_H + 18.0;
    let row = Rect::from_min_size(Pos2::new(rect.center().x - 170.0, y), egui::vec2(340.0, 50.0));
    let resp = ui.interact(row, ui.id().with("mirror"), Sense::click());
    let h = kit::hover_t(ui, &resp);
    let on = ui.ctx().animate_bool_with_time(resp.id.with("on"), e.mirror, 0.16);
    let p = ui.painter();
    p.rect_filled(row, CornerRadius::same(14), kit::mix(theme::SURFACE_CONTAINER, Color32::from_rgb(40, 52, 60), h * 0.6));
    p.text(Pos2::new(row.left() + 26.0, row.center().y), Align2::CENTER_CENTER, icon::FLIP_HORIZONTAL, FontId::proportional(19.0), kit::mix(theme::ON_SURFACE_VAR, theme::PRIMARY, on));
    p.text(Pos2::new(row.left() + 48.0, row.center().y - 9.0), Align2::LEFT_CENTER, "Mirror mode", FontId::proportional(16.0), Color32::WHITE);
    p.text(Pos2::new(row.left() + 48.0, row.center().y + 11.0), Align2::LEFT_CENTER, "Changes copy to the other hand", FontId::proportional(12.5), theme::ON_SURFACE_VAR);
    kit::switch(p, Pos2::new(row.right() - 36.0, row.center().y), on, h);
    if resp.clicked() {
        e.mirror = !e.mirror;
    }
    y = row.bottom() + 12.0;
    let buttons = Rect::from_min_size(Pos2::new(rect.left(), y), egui::vec2(rect.width(), 46.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(buttons).layout(Layout::left_to_right(Align::Center)));
    let bw = 160.0;
    if e.own() {
        child.add_space(((rect.width() - bw) / 2.0).max(0.0));
        let n = e.doc.chords(&e.set).len();
        let label = if n == 0 { "Chords".to_string() } else { format!("Chords · {n}") };
        if kit::button(&mut child, icon::LINK_SIMPLE, &label, kit::Tone::Neutral, bw).on_hover_text("Buttons held together").clicked() {
            e.popup = Some(Popup::Chords);
        }
    } else {
        child.add_space(((rect.width() - bw * 2.0 - 12.0) / 2.0).max(0.0));
        if kit::button(&mut child, icon::VIBRATE, "Haptics", kit::Tone::Neutral, bw).on_hover_text("Which vibration each hand plays").clicked() {
            e.popup = Some(Popup::Haptics);
        }
        child.add_space(12.0);
        if kit::button(&mut child, icon::HAND, "Poses", kit::Tone::Neutral, bw).on_hover_text("Where the game holds things in your hands").clicked() {
            e.popup = Some(Popup::Poses);
        }
    }
    y = buttons.bottom() + 16.0;
    let hint = match e.set.as_str() {
        _ if !e.own() => None,
        monadeck_core::bindings::own::PLAYSPACE => Some("Hold to move the playspace, double press to snap it back"),
        monadeck_core::bindings::own::MOUSE_SET => Some("While you point at a screen, and there first: a button used here does nothing else on that hand"),
        _ => None,
    };
    if let Some(hint) = hint {
        let text = format!("{}  {hint}", icon::INFO);
        let g = ui.fonts(|f| f.layout(text, FontId::proportional(13.5), theme::ON_SURFACE_VAR, rect.width() - 20.0));
        let h = g.size().y;
        ui.painter().galley(Pos2::new(rect.center().x - g.size().x / 2.0, y), g, theme::ON_SURFACE_VAR);
        y += h + 12.0;
    }
    let missing = e.unbound_required(&e.set);
    if !missing.is_empty() {
        let names: Vec<&str> = missing.iter().map(|a| a.name.as_str()).collect();
        let text = format!("{}  Not on any input yet: {}", icon::WARNING, names.join(", "));
        let g = ui.fonts(|f| f.layout(text, FontId::proportional(13.5), AMBER, rect.width() - 20.0));
        ui.painter().galley(Pos2::new(rect.center().x - g.size().x / 2.0, y), g, AMBER);
    }
}

/// SteamVR's line from the input under the pointer to its spot on the drawing.
fn leader_line(ui: &egui::Ui, fr: &Frame, body: Rect) {
    let Some(key) = &fr.hover else { return };
    let Some((_, anchor, true)) = fr.anchors.iter().find(|(k, _, _)| k == key) else { return };
    let Some((_, spot)) = fr.spots.iter().find(|(k, _)| k == key) else { return };
    let bend = Pos2::new(anchor.x + if key.0 == Hand::Left { 22.0 } else { -22.0 }, anchor.y);
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("bind-leader"))).with_clip_rect(body);
    let stroke = Stroke::new(1.6, kit::alpha(theme::PRIMARY, 0.85));
    painter.line_segment([*anchor, bend], stroke);
    painter.line_segment([bend, *spot], stroke);
    painter.circle_filled(*anchor, 3.0, theme::PRIMARY);
}

fn footer_ui(ui: &mut egui::Ui, e: &mut Editor, st: &mut LibState, game: &str, running: bool, cmd: &mut Option<Cmd>) {
    let rect = ui.max_rect();
    let p = ui.painter();
    p.hline(rect.x_range(), rect.top() - 7.0, Stroke::new(1.0, kit::alpha(Color32::WHITE, 0.06)));
    let dirty = e.dirty();
    let blocker = e.save_blocker();
    let (glyph, text, fg) = if let Some(err) = &st.binds.error {
        (icon::WARNING, err.clone(), RED)
    } else if let Some(why) = &blocker {
        (icon::WARNING, why.clone(), AMBER)
    } else if dirty {
        (icon::CIRCLE, "Unsaved changes".to_string(), AMBER)
    } else if e.personal && e.own() {
        (icon::CHECK, "Saved · in use now".to_string(), theme::PRIMARY)
    } else if e.own() {
        (icon::INFO, "Monadeck's default controls · save a change to make them yours".to_string(), theme::ON_SURFACE_VAR)
    } else if e.personal && running && st.binds.live {
        (icon::CHECK, format!("Saved · {game} is using it"), theme::PRIMARY)
    } else if e.personal && running {
        (icon::CHECK, format!("Saved · restart {game} to use it"), theme::PRIMARY)
    } else if e.personal {
        (icon::CHECK, format!("Saved · {game} uses it from its next start"), theme::PRIMARY)
    } else {
        (icon::INFO, "The game's own binding · save a change to make it yours".to_string(), theme::ON_SURFACE_VAR)
    };
    let g = kit::fit_text(ui, &format!("{glyph}  {text}"), 15.0, fg, rect.width() - 560.0);
    ui.painter().galley(Pos2::new(rect.left() + 4.0, rect.center().y - g.size().y / 2.0), g, fg);

    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::right_to_left(Align::Center)));
    let ui = &mut child;
    let label = if e.busy { "Saving…" } else { "Save" };
    let tip = if e.own() { "Save your binding: it's used right away" } else { "Save as your personal binding (the game's own file stays as it is)" };
    let tip = blocker.as_deref().unwrap_or(tip);
    if kit::button_enabled(ui, icon::FLOPPY_DISK, label, kit::Tone::Primary, 130.0, dirty && !e.busy && blocker.is_none()).on_hover_text(tip).clicked() {
        *cmd = Some(Cmd::Save);
    }
    if dirty {
        ui.add_space(10.0);
        if kit::button(ui, icon::ARROW_COUNTER_CLOCKWISE, "Undo changes", kit::Tone::Neutral, 0.0).clicked() {
            e.doc = e.saved.clone();
        }
    }
    if e.personal && !e.busy {
        ui.add_space(10.0);
        let armed = st.is_armed("bind-reset");
        let (tone, label) = if armed { (kit::Tone::DangerArmed, "Tap again to reset") } else { (kit::Tone::Danger, "Reset to default") };
        let tip = if e.own() { "Go back to Monadeck's default controls (yours is kept as a .bak file)" } else { "Go back to the game's own binding (yours is kept as a .bak file)" };
        if kit::button(ui, icon::ARROW_COUNTER_CLOCKWISE, label, tone, 0.0).on_hover_text(tip).clicked() && st.confirm_tap("bind-reset") {
            *cmd = Some(Cmd::Reset);
        }
    }
}

// --- pickers ------------------------------------------------------------------------------------

/// A centred card over a dimmed editor. Returns true when the dim was tapped
/// (or the card's close button): the caller closes it.
fn modal(ctx: &egui::Context, width: f32, title: &str, sub: &str, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let mut close = false;
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("bind-dim")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.painter().rect_filled(r, CornerRadius::ZERO, Color32::from_black_alpha(150));
        if resp.clicked() {
            close = true;
        }
    });
    // A fixed top rather than centred: an Area sizes from its last frame, so a
    // centred one would start short and clip its list.
    let top = screen.top() + 70.0;
    let pos = Pos2::new(screen.center().x - width / 2.0 - 22.0, top);
    egui::Area::new(egui::Id::new("bind-modal")).order(egui::Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
        egui::Frame::default()
            .fill(Color32::from_rgb(28, 33, 40))
            .stroke(Stroke::new(1.0, kit::alpha(Color32::WHITE, 0.08)))
            .corner_radius(22)
            .inner_margin(egui::Margin::same(22))
            .shadow(egui::epaint::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(140) })
            .show(ui, |ui| {
                ui.set_width(width);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(title).size(21.0).strong().color(Color32::WHITE));
                        if !sub.is_empty() {
                            ui.label(egui::RichText::new(sub).size(14.0).color(theme::ON_SURFACE_VAR));
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        if kit::small_icon(ui, icon::X, "Close").clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(14.0);
                // An Area lays out inside last frame's size, so a plain scroll area
                // could never grow: `min_scrolled_height` makes it take the full
                // height whenever the list needs it.
                let list_h = screen.bottom() - top - 190.0;
                egui::ScrollArea::vertical().max_height(list_h).min_scrolled_height(list_h).auto_shrink([false, true]).show(ui, |ui| {
                    ui.set_width(width - 8.0);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    body(ui);
                });
            });
    });
    close
}

/// A pickable row: glyph, title, a quieter line, a badge; lit when current.
fn pick_row(ui: &mut egui::Ui, glyph: &str, title: &str, sub: &str, badge: Option<(&str, Color32)>, current: bool) -> egui::Response {
    let h_row = if sub.is_empty() { 48.0 } else { 60.0 };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), h_row), Sense::click());
    let h = kit::hover_t(ui, &resp);
    let p = ui.painter();
    let fill = if current { Color32::from_rgb(20, 72, 70) } else { kit::alpha(Color32::WHITE, 0.03 + 0.06 * h) };
    p.rect_filled(rect, CornerRadius::same(13), fill);
    if current {
        p.rect_stroke(rect, CornerRadius::same(13), Stroke::new(1.0, kit::alpha(theme::PRIMARY, 0.6)), StrokeKind::Inside);
    }
    let chip = Rect::from_center_size(Pos2::new(rect.left() + 30.0, rect.center().y), egui::vec2(36.0, 36.0));
    kit::icon_chip(p, chip, glyph, if current { 1.0 } else { h * 0.5 });
    let mut right = rect.right() - 16.0;
    if current {
        p.text(Pos2::new(right - 8.0, rect.center().y), Align2::CENTER_CENTER, icon::CHECK, FontId::proportional(18.0), theme::PRIMARY);
        right -= 30.0;
    }
    if let Some((text, c)) = badge {
        let g = p.layout_no_wrap(text.to_string(), FontId::proportional(12.0), c);
        let r = Rect::from_min_size(Pos2::new(right - g.size().x - 18.0, rect.center().y - 11.0), egui::vec2(g.size().x + 18.0, 22.0));
        p.rect_filled(r, CornerRadius::same(11), kit::alpha(c, 0.16));
        p.galley(Pos2::new(r.left() + 9.0, r.center().y - g.size().y / 2.0), g, c);
        right = r.left() - 10.0;
    }
    let text_w = right - rect.left() - 60.0;
    let tg = kit::fit_text(ui, title, 16.5, Color32::WHITE, text_w);
    let ty = if sub.is_empty() { rect.center().y - tg.size().y / 2.0 } else { rect.top() + 9.0 };
    ui.painter().galley(Pos2::new(rect.left() + 60.0, ty), tg, Color32::WHITE);
    if !sub.is_empty() {
        ui.painter().galley(Pos2::new(rect.left() + 60.0, rect.top() + 33.0), kit::fit_text(ui, sub, 13.0, theme::ON_SURFACE_VAR, text_w), Color32::WHITE);
    }
    resp
}

fn mode_glyph(m: Mode) -> &'static str {
    match m {
        Mode::Button => icon::RADIO_BUTTON,
        Mode::ToggleButton => icon::TOGGLE_RIGHT,
        Mode::Trigger => icon::ARROW_LINE_DOWN,
        Mode::Joystick => icon::JOYSTICK,
        Mode::Trackpad => icon::CIRCLE_DASHED,
        Mode::Dpad => icon::ARROWS_OUT_CARDINAL,
        Mode::Scroll => icon::MOUSE_SCROLL,
        Mode::ForceSensor => icon::GAUGE,
        Mode::Grab => icon::HAND_GRABBING,
        Mode::ScalarConstant => icon::HASH,
        Mode::None => icon::PROHIBIT,
    }
}

fn kind_glyph(k: ActionKind) -> &'static str {
    match k {
        ActionKind::Boolean => icon::CURSOR_CLICK,
        ActionKind::Vector1 => icon::SLIDERS_HORIZONTAL,
        ActionKind::Vector2 => icon::JOYSTICK,
        ActionKind::Vibration => icon::VIBRATE,
        ActionKind::Pose => icon::HAND,
        _ => icon::CIRCLE,
    }
}

/// What a slot needs, in words.
fn kind_words(k: ActionKind) -> &'static str {
    match k {
        ActionKind::Boolean => "an on / off action",
        ActionKind::Vector1 => "an action that takes an amount (0 to 1)",
        ActionKind::Vector2 => "an action that takes a direction",
        _ => "a matching action",
    }
}

fn popups(ctx: &egui::Context, e: &mut Editor, st: &mut LibState, subj: Option<&Subject>, cmd: &mut Option<Cmd>) {
    let Some(popup) = e.popup.clone() else { return };
    let set_name = e.manifest.set(&e.set).map_or(String::new(), |s| s.name.clone());
    let mut close: bool;
    match popup {
        Popup::AddMode { hand, input } => {
            let Some(def) = e.ctrl.input(&input) else {
                e.popup = None;
                return;
            };
            let title = input_label(hand, def.label);
            let mut chosen = None;
            close = modal(ctx, 560.0, &title, "Use it as…", |ui| {
                for &m in def.modes {
                    if pick_row(ui, mode_glyph(m), m.label(), m.blurb(), None, false).clicked() {
                        chosen = Some(m);
                    }
                }
            });
            if let Some(m) = chosen {
                let index = e.add(hand, &input, m);
                // Straight on to what its first slot does.
                e.popup = m.slots().iter().find(|s| def.has_slot(s)).map(|s| Popup::Action { index, slot: s.key });
                st.sound_select = true;
                return;
            }
        }
        Popup::ChangeMode { index } => {
            let Some(src) = e.doc.sources(&e.set).into_iter().find(|s| s.index == index) else {
                e.popup = None;
                return;
            };
            let Some((hand, def)) = src.hand.zip(e.ctrl.input(&src.input)) else {
                e.popup = None;
                return;
            };
            let mut chosen = None;
            close = modal(ctx, 560.0, &input_label(hand, def.label), "Use it as…", |ui| {
                for &m in def.modes {
                    if pick_row(ui, mode_glyph(m), m.label(), m.blurb(), None, m.id() == src.mode).clicked() {
                        chosen = Some(m);
                    }
                }
            });
            if let Some(m) = chosen {
                if m.id() != src.mode {
                    e.change_mode(index, m);
                }
                close = true;
            }
        }
        Popup::Action { index, slot } => {
            let sources = e.doc.sources(&e.set);
            let Some(src) = sources.iter().find(|s| s.index == index).cloned() else {
                e.popup = None;
                return;
            };
            let Some(sl) = Mode::from_id(&src.mode).and_then(|m| m.slots().iter().find(|s| s.key == slot)) else {
                e.popup = None;
                return;
            };
            let ctrl = e.ctrl;
            let name_of = |s: &Source| s.hand.map_or(s.input.clone(), |h| input_label(h, ctrl.input(&s.input).map_or(s.input.as_str(), |d| d.label)));
            let title = format!("{} · {}", name_of(&src), sl.label);
            let current = src.output(slot).map(str::to_ascii_lowercase);
            // Each fitting action of the set, and where else this set uses it.
            let mut rows: Vec<(String, String, bool, Vec<String>)> = e
                .manifest
                .actions
                .iter()
                .filter(|a| a.set == e.set && a.kind == sl.kind)
                .map(|a| {
                    let used: Vec<String> = sources
                        .iter()
                        .filter(|s| s.index != index)
                        .flat_map(|s| {
                            s.outputs.iter().filter(|(_, o)| o.eq_ignore_ascii_case(&a.key)).map(|(k, _)| {
                                let slot_label = Mode::from_id(&s.mode).and_then(|m| m.slots().iter().find(|x| x.key == k)).map_or(k.as_str(), |x| x.label);
                                format!("{} {}", name_of(s), slot_label.to_lowercase())
                            })
                        })
                        .collect();
                    (a.key.clone(), a.name.clone(), a.mandatory, used)
                })
                .collect();
            rows.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
            let mut chosen: Option<Option<String>> = None;
            let sub = format!("What it does in {set_name}");
            close = modal(ctx, 600.0, &title, &sub, |ui| {
                if pick_row(ui, icon::PROHIBIT, "None", "Does nothing", None, current.is_none()).clicked() {
                    chosen = Some(None);
                }
                if rows.is_empty() {
                    kit::empty_state(ui, kind_glyph(sl.kind), &format!("Nothing in {set_name} fits here"), &format!("{} needs {}", sl.label, kind_words(sl.kind)));
                }
                for (key, name, mandatory, used) in &rows {
                    let sub = match used.len() {
                        0 => String::new(),
                        1 => format!("Also on {}", used[0]),
                        n => format!("Also on {} and {} more", used[0], n - 1),
                    };
                    let is_current = current.as_deref() == Some(key.as_str());
                    let badge = (*mandatory && used.is_empty() && !is_current).then_some(("Required", AMBER));
                    if pick_row(ui, kind_glyph(sl.kind), name, &sub, badge, is_current).clicked() {
                        chosen = Some(Some(key.clone()));
                    }
                }
            });
            if let Some(action) = chosen {
                e.bind(index, slot, action.as_deref());
                st.sound_select = true;
                close = true;
            }
        }
        Popup::Controller => {
            let Some(g) = subj else {
                e.popup = None;
                return;
            };
            let mut chosen = None;
            close = modal(ctx, 520.0, "Controller", "Each controller has its own binding", |ui| {
                for &ty in &g.controllers {
                    let Some(c) = controller_for(e.target, ty) else { continue };
                    let sub = match (g.personal.contains(&ty), e.own()) {
                        (true, _) => "Your personal binding",
                        (false, true) => "Monadeck's default controls",
                        (false, false) => "The game's own binding",
                    };
                    let in_hands = if ty == "udcap_gloves" { st.binds.gloves } else { st.binds.holding == Some(ty) };
                    let holding = in_hands.then_some(("In your hands", theme::PRIMARY));
                    if pick_row(ui, controller_glyph(ty, Hand::Right), c.name, sub, holding, ty == e.ctrl.ty).clicked() {
                        chosen = Some(ty);
                    }
                }
            });
            if let Some(ty) = chosen.filter(|t| *t != e.ctrl.ty) {
                if e.dirty() {
                    e.popup = Some(Popup::Leave(Leave::ToController(ty)));
                } else {
                    e.popup = None;
                    *cmd = Some(Cmd::Open(ty));
                }
                return;
            }
        }
        Popup::Haptics => {
            let vib: Vec<(String, String)> = e.manifest.actions.iter().filter(|a| a.set == e.set && a.kind == ActionKind::Vibration).map(|a| (a.key.clone(), a.name.clone())).collect();
            let set = e.set.clone();
            let mirror = e.mirror;
            close = modal(ctx, 620.0, "Haptics", &format!("Which vibration each hand plays in {set_name}"), |ui| {
                if vib.is_empty() {
                    kit::empty_state(ui, icon::VIBRATE, &format!("No vibration in {set_name}"), "The game doesn't vibrate the controllers here");
                    return;
                }
                for hand in [Hand::Left, Hand::Right] {
                    let cur = e.doc.haptic(&set, hand).map(|a| a.to_ascii_lowercase());
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(format!("{} controller", if hand == Hand::Left { "Left" } else { "Right" })).size(15.0).color(theme::ON_SURFACE_VAR));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                        if kit::choice_chip(ui, "Off", cur.is_none()).clicked() {
                            e.doc.set_haptic(&set, hand, None, mirror);
                        }
                        for (key, name) in &vib {
                            if kit::choice_chip(ui, name, cur.as_deref() == Some(key.as_str())).clicked() {
                                e.doc.set_haptic(&set, hand, Some(key), mirror);
                            }
                        }
                    });
                    ui.add_space(8.0);
                }
            });
        }
        Popup::Poses => {
            let poses: Vec<(String, String)> = e.manifest.actions.iter().filter(|a| a.set == e.set && a.kind == ActionKind::Pose).map(|a| (a.key.clone(), a.name.clone())).collect();
            let set = e.set.clone();
            let mirror = e.mirror;
            close = modal(ctx, 700.0, "Poses", &format!("Where {set_name} follows your hands"), |ui| {
                if poses.is_empty() {
                    kit::empty_state(ui, icon::HAND, &format!("No poses in {set_name}"), "The game tracks your hands through another set, or not at all");
                    return;
                }
                let bound = e.doc.poses(&set);
                let options: Vec<&str> = std::iter::once("Off").chain(core::POSE_POINTS.iter().map(|p| p.1)).collect();
                for (key, name) in &poses {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(name).size(16.5).color(Color32::WHITE));
                    for hand in [Hand::Left, Hand::Right] {
                        let cur = bound.iter().find(|b| b.hand == hand && b.action.eq_ignore_ascii_case(key)).map(|b| b.point.clone());
                        let sel = match &cur {
                            None => 0,
                            Some(pt) => core::POSE_POINTS.iter().position(|p| p.0 == pt).map_or(1, |i| i + 1),
                        };
                        kit::row(ui, if hand == Hand::Left { "Left hand" } else { "Right hand" }, "", 420.0, |ui| {
                            if let Some(i) = kit::segmented(ui, &options, sel) {
                                let pt = (i > 0).then(|| core::POSE_POINTS[i - 1].0);
                                e.doc.set_pose(&set, key, hand, pt, mirror);
                            }
                        });
                    }
                    ui.add_space(4.0);
                }
            });
        }
        Popup::Chords => {
            close = chords_popup(ctx, e, &set_name);
        }
        Popup::Leave(leave) => {
            let name = subj.map_or("this game", |g| g.name.as_str());
            let mut choice = None;
            let blocker = e.save_blocker();
            close = modal(ctx, 520.0, "Save your changes?", &format!("You changed {name}'s controls"), |ui| {
                if let Some(why) = &blocker {
                    ui.add(egui::Label::new(egui::RichText::new(format!("{}  {why}", icon::WARNING)).size(14.0).color(AMBER)).wrap());
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if kit::button_enabled(ui, icon::FLOPPY_DISK, "Save", kit::Tone::Primary, 140.0, blocker.is_none()).clicked() {
                        choice = Some(0);
                    }
                    ui.add_space(8.0);
                    if kit::button(ui, icon::TRASH, "Don't save", kit::Tone::Danger, 0.0).clicked() {
                        choice = Some(1);
                    }
                    ui.add_space(8.0);
                    if kit::button(ui, "", "Keep editing", kit::Tone::Neutral, 0.0).clicked() {
                        choice = Some(2);
                    }
                });
            });
            match choice {
                Some(0) => {
                    e.popup = None;
                    *cmd = Some(Cmd::SaveThen(leave));
                    return;
                }
                Some(1) => {
                    e.popup = None;
                    e.doc = e.saved.clone();
                    *cmd = Some(match leave {
                        Leave::ToList => Cmd::Close,
                        Leave::ToController(ty) => Cmd::Open(ty),
                    });
                    return;
                }
                Some(_) => close = true,
                None => {}
            }
        }
    }
    if close {
        e.popup = None;
    }
}

/// An edit picked in the chords picker (applied once it's drawn).
enum ChordEdit {
    Action(usize, String),
    Remove(usize),
    Toggle(usize, Hand, &'static str),
    New(String),
}

/// Buttons held together: each chord's action, and its buttons picked per hand.
fn chords_popup(ctx: &egui::Context, e: &mut Editor, set_name: &str) -> bool {
    let set = e.set.clone();
    let actions: Vec<(String, String)> = e.manifest.actions.iter().filter(|a| a.set == set && a.kind == ActionKind::Boolean).map(|a| (a.key.clone(), a.name.clone())).collect();
    let chords = e.doc.chords(&set);
    let ctrl = e.ctrl;
    let mut edits: Vec<ChordEdit> = Vec::new();
    let close = modal(ctx, 720.0, "Chords", &format!("Buttons held together · {set_name}"), |ui| {
        if chords.is_empty() {
            kit::empty_state(ui, icon::LINK_SIMPLE, "No chords yet", "A chord does something while all its buttons are held");
        }
        for c in &chords {
            egui::Frame::default().fill(theme::SURFACE_CONTAINER).corner_radius(16).inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("DOES").size(12.5).strong().color(theme::ON_SURFACE_VAR));
                    ui.add_space(6.0);
                    for (key, name) in &actions {
                        if kit::choice_chip(ui, name, c.action.eq_ignore_ascii_case(key)).clicked() {
                            edits.push(ChordEdit::Action(c.index, key.clone()));
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if kit::small_icon(ui, icon::TRASH, "Remove this chord").clicked() {
                            edits.push(ChordEdit::Remove(c.index));
                        }
                    });
                });
                ui.add_space(8.0);
                for hand in [Hand::Left, Hand::Right] {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(62.0, 42.0), Sense::hover());
                        ui.painter().text(Pos2::new(rect.left(), rect.center().y), Align2::LEFT_CENTER, if hand == Hand::Left { "Left" } else { "Right" }, FontId::proportional(15.0), theme::ON_SURFACE_VAR);
                        for d in ctrl.inputs.iter().filter(|d| d.on(hand) && d.bindable()) {
                            let on = c.inputs.iter().any(|i| i.hand == Some(hand) && i.input == d.id);
                            if kit::choice_chip(ui, d.label, on).clicked() {
                                edits.push(ChordEdit::Toggle(c.index, hand, d.id));
                            }
                        }
                    });
                }
                if c.inputs.len() == 1 {
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("{}  Pick at least two buttons", icon::WARNING)).size(13.0).color(AMBER));
                }
            });
            ui.add_space(6.0);
        }
        ui.add_space(6.0);
        if let Some((first, _)) = actions.first() {
            if kit::button(ui, icon::PLUS, "New chord", kit::Tone::Neutral, 0.0).clicked() {
                edits.push(ChordEdit::New(first.clone()));
            }
        }
    });
    for edit in edits {
        match edit {
            ChordEdit::Action(i, action) => e.doc.set_chord_action(&set, i, &action),
            ChordEdit::Remove(i) => e.doc.remove_chord(&set, i),
            ChordEdit::Toggle(i, hand, id) => e.doc.toggle_chord_input(&set, i, hand, id, "click"),
            ChordEdit::New(action) => {
                e.doc.add_chord(&set, &action);
            }
        }
    }
    close
}
