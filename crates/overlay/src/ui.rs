// The in-headset library UI, drawn into the panel's egui context each frame.
// SteamVR-dashboard-inspired: left icon rail, search bar, a hero banner for the
// selected game, browsable game grids, and a bottom status bar.
use egui_phosphor::regular as icon;

use crate::games::LoadedGame;
use crate::gfx::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Home,
    Library,
    Tags,
    Settings,
}

/// All mutable UI state for the launcher panel. The render loop owns one of
/// these; `build` reads and mutates it, and surfaces actions for the loop.
pub struct LibState {
    pub games: Vec<LoadedGame>,
    pub scanning: bool,
    pub search: String,
    pub nav: Nav,
    pub selected: Option<usize>,
    /// Set by `build` when the user activates Play; the loop drains it.
    pub launch_request: Option<usize>,
    /// Set by `build` when the user asks to recenter the panel.
    pub recenter_request: bool,
}

impl LibState {
    pub fn new() -> Self {
        Self {
            games: Vec::new(),
            scanning: true,
            search: String::new(),
            nav: Nav::Home,
            selected: None,
            launch_request: None,
            recenter_request: false,
        }
    }
}

const TILE_W: f32 = 168.0;
const TILE_H: f32 = 252.0; // 2:3 portrait capsule, like Steam's grid.

pub fn build(ctx: &egui::Context, st: &mut LibState) {
    left_rail(ctx, st);
    bottom_bar(ctx, st);
    if !matches!(st.nav, Nav::Settings) {
        top_bar(ctx, st);
    }
    central(ctx, st);
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
                    (icon::TAG, Nav::Tags),
                ] {
                    if nav_button(ui, glyph, st.nav == nav) {
                        st.nav = nav;
                    }
                    ui.add_space(6.0);
                }
                let avail = ui.available_height();
                ui.add_space((avail - 52.0).max(0.0));
                if nav_button(ui, icon::GEAR, st.nav == Nav::Settings) {
                    st.nav = Nav::Settings;
                }
            });
        });
}

fn nav_button(ui: &mut egui::Ui, glyph: &str, active: bool) -> bool {
    let fg = if active { egui::Color32::BLACK } else { theme::ON_SURFACE_VAR };
    let fill = if active { theme::PRIMARY } else { egui::Color32::TRANSPARENT };
    let btn = egui::Button::new(egui::RichText::new(glyph).size(24.0).color(fg))
        .min_size(egui::vec2(48.0, 48.0))
        .fill(fill)
        .frame(true);
    ui.add(btn).clicked()
}

fn top_bar(ctx: &egui::Context, st: &mut LibState) {
    let frame = egui::Frame::default()
        .fill(egui::Color32::from_rgb(13, 16, 20))
        .inner_margin(egui::Margin::symmetric(18, 12));
    egui::TopBottomPanel::top("search").exact_height(58.0).frame(frame).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            ui.label(egui::RichText::new(icon::MAGNIFYING_GLASS).size(20.0).color(theme::ON_SURFACE_VAR));
            ui.add_space(8.0);
            ui.add_sized(
                egui::vec2(ui.available_width() - 8.0, 30.0),
                egui::TextEdit::singleline(&mut st.search)
                    .hint_text("Search for games…")
                    .frame(false),
            );
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
            Nav::Tags => tags_view(ui, st),
            Nav::Settings => settings_view(ui, st),
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
        empty_note(ui, st);
        return;
    }
    let mut newly = None;
    egui::ScrollArea::horizontal().id_salt("home-row").show(ui, |ui| {
        ui.horizontal(|ui| {
            for &i in &shown {
                if tile(ui, &st.games[i], st.selected == Some(i), TILE_W, TILE_H).clicked() {
                    newly = Some(i);
                }
                ui.add_space(14.0);
            }
        });
    });
    if newly.is_some() {
        st.selected = newly;
    }
}

fn grid_view(ui: &mut egui::Ui, st: &mut LibState, title: &str) {
    ui.label(egui::RichText::new(title).heading().strong());
    ui.add_space(10.0);
    let shown = filtered(st);
    if shown.is_empty() {
        empty_note(ui, st);
        return;
    }
    let mut newly = None;
    let mut launch = None;
    egui::ScrollArea::vertical().id_salt("grid").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for &i in &shown {
                let r = tile(ui, &st.games[i], st.selected == Some(i), TILE_W, TILE_H);
                if r.clicked() {
                    newly = Some(i);
                }
                if r.double_clicked() {
                    launch = Some(i);
                }
            }
        });
    });
    if newly.is_some() {
        st.selected = newly;
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
        empty_note(ui, st);
        return;
    }
    let groups: [(&str, fn(&str) -> bool); 3] = [
        ("Steam", |s| s == "steam" || s.contains("proton")),
        ("Custom / Non-Steam", |s| s == "custom"),
        ("xrizer overrides", |s| s.contains("xrizer")),
    ];
    let mut newly = None;
    egui::ScrollArea::vertical().id_salt("tags").show(ui, |ui| {
        for (label, pred) in groups {
            let group: Vec<usize> = shown.iter().copied().filter(|&i| pred(&st.games[i].source)).collect();
            if group.is_empty() {
                continue;
            }
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!("{label}  ·  {}", group.len())).strong().color(theme::ON_SURFACE_VAR));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for &i in &group {
                    if tile(ui, &st.games[i], st.selected == Some(i), TILE_W, TILE_H).clicked() {
                        newly = Some(i);
                    }
                }
            });
            ui.add_space(14.0);
        }
    });
    if newly.is_some() {
        st.selected = newly;
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
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Brings the panel back in front of you. You can also grip to grab and move it.")
            .small()
            .color(theme::ON_SURFACE_VAR),
    );

    ui.add_space(20.0);
    ui.separator();
    ui.add_space(12.0);
    ui.label(egui::RichText::new(format!("{} games in your library", st.games.len())).color(theme::ON_SURFACE_VAR));
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Point with a controller, trigger to select, Play to launch.")
            .small()
            .color(theme::ON_SURFACE_VAR),
    );
}

// --- hero banner ------------------------------------------------------------

fn hero(ui: &mut egui::Ui, st: &mut LibState) {
    let sel = st.selected.filter(|&i| i < st.games.len());
    let launch = match sel {
        Some(i) if st.games[i].hero.is_some() => hero_landscape(ui, &st.games[i]),
        Some(i) => hero_portrait(ui, &st.games[i]),
        None => {
            hero_empty(ui);
            false
        }
    };
    if launch {
        st.launch_request = sel;
    }
}

fn hero_card() -> egui::Frame {
    egui::Frame::default()
        .fill(egui::Color32::from_rgb(17, 21, 27))
        .corner_radius(14)
        .inner_margin(egui::Margin::same(16))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 44, 54)))
}

/// Wide banner using the game's landscape hero art, with the title overlaid over
/// a bottom gradient and a Play button — the SteamVR featured look.
fn hero_landscape(ui: &mut egui::Ui, g: &LoadedGame) -> bool {
    let w = ui.available_width();
    let h = (w * 0.30).clamp(190.0, 300.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    paint_cover(ui.painter(), rect, g.hero.as_ref(), &g.name);

    // Bottom-up dark gradient (banded) so the title stays legible over any art.
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

    let play_rect = egui::Rect::from_min_size(
        egui::pos2(rect.right() - 16.0 - 150.0, rect.bottom() - 16.0 - 46.0),
        egui::vec2(150.0, 46.0),
    );
    let play = egui::Button::new(
        egui::RichText::new(format!("{}  Play", icon::PLAY)).size(19.0).color(egui::Color32::BLACK),
    )
    .fill(theme::PRIMARY);
    ui.put(play_rect, play).clicked()
}

/// Fallback when there's no landscape art: portrait cover + title/Play beside it.
fn hero_portrait(ui: &mut egui::Ui, g: &LoadedGame) -> bool {
    let mut launch = false;
    hero_card().show(ui, |ui| {
        ui.set_min_height(248.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(172.0, 258.0), egui::Sense::hover());
            paint_cover(ui.painter(), rect, g.texture.as_ref(), &g.name);
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(&g.name).size(30.0).strong());
                ui.add_space(4.0);
                ui.label(egui::RichText::new(sub_label(g)).color(theme::ON_SURFACE_VAR));
                ui.add_space(20.0);
                let play = egui::Button::new(
                    egui::RichText::new(format!("{}  Play", icon::PLAY)).size(20.0).color(egui::Color32::BLACK),
                )
                .fill(theme::PRIMARY)
                .min_size(egui::vec2(160.0, 48.0));
                if ui.add(play).clicked() {
                    launch = true;
                }
            });
        });
    });
    launch
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

/// Search-filtered game indices (case-insensitive substring on the name).
fn filtered(st: &LibState) -> Vec<usize> {
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
                "No VR games found yet."
            } else {
                "No games match your search."
            })
            .color(theme::ON_SURFACE_VAR),
        );
    });
}

/// Source + last-played, e.g. "Steam · played 3 days ago".
fn sub_label(g: &LoadedGame) -> String {
    match played_ago(g.last_played) {
        Some(a) => format!("{} · played {a}", source_label(&g.source)),
        None => source_label(&g.source),
    }
}

/// A coarse "time since last played" string, or None if never played / unknown.
fn played_ago(ts: Option<u64>) -> Option<String> {
    let ts = ts.filter(|&t| t > 0)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
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

fn source_label(source: &str) -> String {
    if source.contains("proton") {
        "Steam · Proton".into()
    } else if source == "steam" {
        "Steam".into()
    } else if source.contains("xrizer") {
        "xrizer override".into()
    } else if source == "custom" {
        "Non-Steam".into()
    } else {
        source.to_string()
    }
}

fn tile(ui: &mut egui::Ui, game: &LoadedGame, selected: bool, w: f32, h: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::click());
    paint_cover(ui.painter(), rect, game.texture.as_ref(), &game.name);
    if resp.hovered() {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(8), egui::Color32::from_white_alpha(20));
    }
    if selected {
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(8),
            egui::Stroke::new(3.0, theme::PRIMARY),
            egui::StrokeKind::Inside,
        );
    }
    resp.on_hover_text(&game.name)
}

/// Draw a cover into `rect` with center-crop (object-fit: cover), or a named
/// placeholder when there's no texture.
fn paint_cover(painter: &egui::Painter, rect: egui::Rect, tex: Option<&egui::TextureHandle>, name: &str) {
    let radius = egui::CornerRadius::same(8);
    if let Some(tex) = tex {
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
        painter.rect_filled(rect, radius, egui::Color32::from_rgb(12, 14, 18));
        painter.image(tex.id(), rect, uv, egui::Color32::WHITE);
    } else {
        painter.rect_filled(rect, radius, theme::SURFACE_CONTAINER_HIGH);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            short(name),
            egui::FontId::proportional(15.0),
            theme::ON_SURFACE_VAR,
        );
    }
}

fn short(name: &str) -> String {
    if name.chars().count() > 28 {
        format!("{}…", name.chars().take(27).collect::<String>())
    } else {
        name.to_string()
    }
}
