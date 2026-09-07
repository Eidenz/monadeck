//! Toasts: the floating notification cards, drawn on their own composition layer
//! so they show over a game too. One model for everything that pops up outside
//! the dashboard — desktop / XSOverlay notifications, the timer and low-battery
//! alarms, action feedback while the dashboard is away, the boot greeting, and
//! the live readout while a screen is gripped.
//!
//! Every toast goes through [`Toasts`]: one card at a time, queued in arrival
//! order, alarms cutting in front, and the card placed in the lower view the
//! moment it starts showing (so it's read at a glance without blocking what
//! you're looking at). A card unfolds from a thin accent line — the line grows
//! out from its centre, then the card opens up and down from it — and folds
//! back the same way when it's done; a hairline along the bottom edge drains
//! toward the centre while it dwells.
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use egui_phosphor::regular as icon;
use openxr as xr;

use crate::gfx::theme;
use crate::mathx;

/// Where a card sits: this far ahead of the head, this far below the gaze.
const DIST: f32 = 1.3;
const DROP: f32 = 0.42;
/// Entrance: the line grows, then the card unfolds (they overlap a little).
const LINE_IN: f32 = 0.14;
const OPEN_DELAY: f32 = 0.10;
const OPEN_IN: f32 = 0.24;
pub(crate) const ENTER: f32 = OPEN_DELAY + OPEN_IN;
/// Exit: the card folds shut, then the line shrinks away.
const FOLD_OUT: f32 = 0.16;
const LINE_OUT: f32 = 0.10;
pub(crate) const EXIT: f32 = FOLD_OUT + LINE_OUT;
/// With more cards waiting, the one on screen gets at most this long more.
const HURRY: f32 = 1.6;
/// A flood (Discord…) keeps only the latest few.
const QUEUE_CAP: usize = 8;

/// What a toast is about — picks its glyph, accent colour, size and dwell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A desktop (D-Bus) or XSOverlay notification: app eyebrow, icon, body.
    Notification,
    /// The countdown reached zero.
    Timer,
    /// A device is running low.
    Battery,
    /// A neutral hint ("No screen selected").
    Info,
    /// Something didn't happen and needs the user ("Launch it in Steam first").
    Warning,
    /// "Done" feedback for an action taken while the dashboard was hidden.
    Confirm,
    /// The boot greeting.
    Welcome,
    /// Live size / distance / curve while a screen is gripped (updated in place).
    Readout,
}

/// Where a notification came from (shown as a small glyph in the eyebrow).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Desktop,
    XsOverlay,
}

/// How much card a kind gets.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Size {
    /// One-line pill: glyph + title.
    Compact,
    /// Icon chip + title + body.
    Card,
}

impl Kind {
    fn glyph(self) -> &'static str {
        match self {
            Kind::Notification => icon::BELL,
            Kind::Timer => icon::TIMER,
            Kind::Battery => icon::BATTERY_WARNING,
            Kind::Info => icon::INFO,
            Kind::Warning => icon::WARNING,
            Kind::Confirm => icon::CHECK_CIRCLE,
            Kind::Welcome => icon::HAND_WAVING,
            Kind::Readout => icon::ARROWS_OUT_CARDINAL,
        }
    }

    fn accent(self) -> egui::Color32 {
        match self {
            Kind::Notification => egui::Color32::from_rgb(150, 190, 255),
            Kind::Timer | Kind::Welcome => theme::PRIMARY,
            Kind::Battery => egui::Color32::from_rgb(255, 192, 74),
            Kind::Info => egui::Color32::from_rgb(150, 190, 255),
            Kind::Warning => egui::Color32::from_rgb(255, 140, 96),
            Kind::Confirm => egui::Color32::from_rgb(104, 224, 150),
            Kind::Readout => theme::ON_SURFACE_VAR,
        }
    }

    fn size(self) -> Size {
        match self {
            Kind::Confirm => Size::Compact,
            _ => Size::Card,
        }
    }

    /// Default dwell (seconds fully open, between the unfold and the fold).
    fn dwell(self) -> f32 {
        match self {
            Kind::Notification => 5.0,
            Kind::Timer | Kind::Battery | Kind::Warning | Kind::Welcome => 6.0,
            Kind::Info => 3.5,
            Kind::Confirm => 2.0,
            Kind::Readout => 0.7,
        }
    }

    /// Alarms cut in front of whatever is queued.
    fn urgent(self) -> bool {
        matches!(self, Kind::Timer | Kind::Battery | Kind::Warning | Kind::Welcome | Kind::Confirm)
    }
}

/// One card. Build with [`Toast::new`] + the chained setters, hand to [`Toasts`].
pub struct Toast {
    pub kind: Kind,
    /// Eyebrow above the title (the sending app, for notifications).
    pub app: String,
    pub title: String,
    pub body: String,
    pub source: Option<Source>,
    /// Seconds fully open.
    pub secs: f32,
    /// App icon to upload on first draw (textures are per egui context).
    pub icon: Option<egui::ColorImage>,
    pub icon_tex: Option<egui::TextureHandle>,
    /// Accent override (a notification takes the colour of its app icon).
    pub accent: Option<egui::Color32>,
    pub pose: xr::Posef,
    /// `pose` was given by the caller (a readout floats over its screen) —
    /// don't place it in front of the head.
    placed: bool,
    /// Re-place in front of the head every frame (the boot greeting must find
    /// you even if you put the headset on late).
    pub follow: bool,
    shown_at: Option<Instant>,
    until: Instant,
}

impl Toast {
    pub fn new(kind: Kind, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            kind,
            app: String::new(),
            title: title.into(),
            body: body.into(),
            source: None,
            secs: kind.dwell(),
            icon: None,
            icon_tex: None,
            accent: None,
            pose: xr::Posef::IDENTITY,
            placed: false,
            follow: false,
            shown_at: None,
            until: Instant::now(),
        }
    }

    pub fn secs(mut self, secs: f32) -> Self {
        self.secs = secs;
        self
    }

    pub fn app(mut self, app: impl Into<String>) -> Self {
        self.app = app.into();
        self
    }

    pub fn source(mut self, source: Source) -> Self {
        self.source = Some(source);
        self
    }

    /// An app icon; the card's accent takes the icon's dominant colour.
    pub fn icon(mut self, icon: Option<egui::ColorImage>) -> Self {
        if let Some(img) = &icon {
            self.accent = accent_from_icon(img);
        }
        self.icon = icon;
        self
    }

    pub fn follow(mut self) -> Self {
        self.follow = true;
        self
    }

    pub fn at(mut self, pose: xr::Posef) -> Self {
        self.pose = pose;
        self.placed = true;
        self
    }

    fn accent(&self) -> egui::Color32 {
        self.accent.unwrap_or_else(|| self.kind.accent())
    }

    /// Full lifetime from the first frame: unfold + dwell + fold.
    fn lifetime(&self) -> f32 {
        ENTER + self.secs + EXIT
    }

    fn start(&mut self, now: Instant) {
        self.shown_at = Some(now);
        self.until = now + Duration::from_secs_f32(self.lifetime());
    }

    /// Animation state at `now`: (line width 0..1, card openness 0..1, dwell
    /// progress 0..1).
    fn phases(&self, now: Instant) -> (f32, f32, f32) {
        let age = self.shown_at.map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32());
        let remain = self.until.saturating_duration_since(now).as_secs_f32();
        let line_in = ease_out((age / LINE_IN).clamp(0.0, 1.0));
        let open_in = ease_out(((age - OPEN_DELAY) / OPEN_IN).clamp(0.0, 1.0));
        let open_out = ease_in(((remain - LINE_OUT) / FOLD_OUT).clamp(0.0, 1.0));
        let line_out = (remain / LINE_OUT).clamp(0.0, 1.0);
        let dwell = self.lifetime() - ENTER - EXIT;
        let progress = if dwell > 0.0 { ((age - ENTER) / dwell).clamp(0.0, 1.0) } else { 1.0 };
        (line_in.min(line_out), open_in.min(open_out), progress)
    }
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in(t: f32) -> f32 {
    t * t * t
}

/// The dominant colour of an app icon, lifted so it reads on the dark card.
/// `None` for icons that are essentially grey (the kind's own accent is better).
fn accent_from_icon(img: &egui::ColorImage) -> Option<egui::Color32> {
    let (mut r, mut g, mut b, mut w) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for p in &img.pixels {
        let a = p.a() as f32 / 255.0;
        if a < 0.5 {
            continue;
        }
        let (pr, pg, pb) = (p.r() as f32, p.g() as f32, p.b() as f32);
        let (mx, mn) = (pr.max(pg).max(pb), pr.min(pg).min(pb));
        // Weight by saturation: the brand colour, not the white/black bits.
        let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
        let wt = a * sat * sat;
        r += pr * wt;
        g += pg * wt;
        b += pb * wt;
        w += wt;
    }
    let n = img.pixels.len().max(1) as f32;
    if w < n * 0.02 {
        return None; // mostly grey
    }
    let (r, g, b) = (r / w, g / w, b / w);
    // Lift to a readable brightness without washing the hue out.
    let mx = r.max(g).max(b).max(1.0);
    let k = (190.0 / mx).max(1.0);
    let lift = |c: f32| ((c * k).min(255.0) * 0.85 + 255.0 * 0.15).round() as u8;
    Some(egui::Color32::from_rgb(lift(r), lift(g), lift(b)))
}

/// The queue: one card showing, the rest waiting in arrival order.
pub struct Toasts {
    current: Option<Toast>,
    queue: VecDeque<Toast>,
}

impl Toasts {
    pub fn new() -> Self {
        Self { current: None, queue: VecDeque::new() }
    }

    /// Queue a card. Alarms go to the front and hurry whatever is showing; the
    /// rest wait their turn (a flood keeps only the newest few).
    pub fn push(&mut self, t: Toast) {
        if t.kind.urgent() {
            self.queue.push_front(t);
            if let Some(c) = &mut self.current {
                hurry(c, Instant::now(), 0.6);
            }
        } else {
            self.queue.push_back(t);
            while self.queue.len() > QUEUE_CAP {
                if let Some(i) = self.queue.iter().position(|t| !t.kind.urgent()) {
                    self.queue.remove(i);
                } else {
                    break;
                }
            }
        }
    }

    /// The live readout while a screen is gripped: updates the showing card in
    /// place (no re-animation), or takes over from whatever is up, which
    /// returns to the queue for afterwards.
    pub fn readout(&mut self, title: impl Into<String>, body: impl Into<String>, pose: xr::Posef) {
        let now = Instant::now();
        match &mut self.current {
            Some(c) if c.kind == Kind::Readout => {
                c.title = title.into();
                c.body = body.into();
                c.pose = pose;
                c.until = now + Duration::from_secs_f32(Kind::Readout.dwell() + EXIT);
            }
            _ => {
                if let Some(mut c) = self.current.take() {
                    // Its icon texture (if uploaded) stays valid: same panel context.
                    c.shown_at = None;
                    self.queue.push_front(c);
                }
                let mut t = Toast::new(Kind::Readout, title, body).at(pose);
                t.start(now);
                self.current = Some(t);
            }
        }
    }

    /// Expire the showing card, start the next. Returns the kind that just
    /// started (for a chime). Cards wait for a head pose to be placed.
    pub fn tick(&mut self, now: Instant, hmd: Option<&xr::Posef>) -> Option<Kind> {
        if self.current.as_ref().is_some_and(|c| now >= c.until) {
            self.current = None;
        }
        if let Some(c) = &mut self.current {
            if !self.queue.is_empty() && c.kind == Kind::Notification {
                hurry(c, now, HURRY);
            }
            return None;
        }
        let placed = self.queue.front().is_some_and(|t| t.placed);
        if !placed && hmd.is_none() {
            return None;
        }
        let mut t = self.queue.pop_front()?;
        if !t.placed {
            t.pose = mathx::toast_pose(hmd.expect("checked above"), DIST, DROP);
        }
        t.start(now);
        let kind = t.kind;
        self.current = Some(t);
        Some(kind)
    }

    pub fn active(&self) -> bool {
        self.current.is_some()
    }

    /// Per-frame upkeep for the showing card: upload a pending icon into the
    /// toast panel's context, ease a following card toward the head.
    pub fn update(&mut self, ctx: &egui::Context, hmd: Option<&xr::Posef>) -> Option<(xr::Posef, usize)> {
        let queued = self.queue.len();
        let t = self.current.as_mut()?;
        if let Some(img) = t.icon.take() {
            t.icon_tex = Some(ctx.load_texture("toast-icon", img, egui::TextureOptions::LINEAR));
        }
        if t.follow {
            if let Some(h) = hmd {
                // Ease toward the spot in front of the head rather than sticking to it.
                let target = mathx::toast_pose(h, DIST, DROP);
                let k = 0.08;
                t.pose.position.x += (target.position.x - t.pose.position.x) * k;
                t.pose.position.y += (target.position.y - t.pose.position.y) * k;
                t.pose.position.z += (target.position.z - t.pose.position.z) * k;
                t.pose.orientation = target.orientation;
            }
        }
        Some((t.pose, queued))
    }

    /// Draw the showing card into the toast panel.
    pub fn draw(&self, ctx: &egui::Context, now: Instant, queued: usize) {
        if let Some(t) = &self.current {
            draw(ctx, t, now, queued);
        }
    }
}

/// Bring a card's end forward so it's gone within `secs` (fold included).
fn hurry(t: &mut Toast, now: Instant, secs: f32) {
    let soonest = now + Duration::from_secs_f32(secs + EXIT);
    if t.until > soonest {
        t.until = soonest;
    }
}

// --- drawing -----------------------------------------------------------------

const MAX_W: f32 = 572.0;
const MIN_W: f32 = 340.0;
const PAD_X: f32 = 18.0;
const PAD_Y: f32 = 14.0;
const CHIP: f32 = 54.0;
const GAP: f32 = 15.0;
const RADIUS: u8 = 20;

/// The card: `open` scales its unfold (clip height + content alpha), `line`
/// the accent line it unfolds from. The panel is cleared transparent, so the
/// rounded card floats centred, with a soft shadow for depth over a game.
fn draw(ctx: &egui::Context, t: &Toast, now: Instant, queued: usize) {
    let (line, open, progress) = t.phases(now);
    let accent = t.accent();
    let screen = ctx.screen_rect();
    let centre = screen.center();
    let size = t.kind.size();

    // --- layout --------------------------------------------------------------
    let text_w = MAX_W - PAD_X * 2.0 - if size == Size::Card { CHIP + GAP } else { 0.0 };
    let alpha = |c: egui::Color32, a: f32| c.gamma_multiply(a.clamp(0.0, 1.0));
    let text_a = ease_out(open);
    let job = |text: &str, px: f32, color: egui::Color32, rows: usize| {
        // Hard row caps so a long title, a multi-line body or an unbroken URL
        // can't spill past the card (Discord loves all three).
        let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut job = egui::text::LayoutJob::simple(one_line, egui::FontId::proportional(px), alpha(color, text_a), text_w);
        // Word wrap with a row cap; egui still splits a single over-long word.
        job.wrap = egui::text::TextWrapping { max_width: text_w, max_rows: rows, break_anywhere: false, overflow_character: Some('…') };
        job
    };
    let eyebrow = (size == Size::Card && !t.app.is_empty()).then(|| {
        let glyph = match t.source {
            Some(Source::Desktop) => icon::DESKTOP,
            Some(Source::XsOverlay) => icon::GOGGLES,
            None => icon::SPARKLE,
        };
        ctx.fonts(|f| f.layout_job(job(&format!("{glyph}  {}", t.app), 13.0, accent, 1)))
    });
    let title = match size {
        Size::Compact => ctx.fonts(|f| f.layout_job(job(&format!("{}  {}", t.kind.glyph(), t.title), 16.5, egui::Color32::WHITE, 1))),
        Size::Card => ctx.fonts(|f| f.layout_job(job(&t.title, 20.0, egui::Color32::WHITE, 1))),
    };
    let body_rows = if t.kind == Kind::Notification { 2 } else { 3 };
    let body = (size == Size::Card && !t.body.is_empty()).then(|| ctx.fonts(|f| f.layout_job(job(&t.body, 15.0, theme::ON_SURFACE_VAR, body_rows))));

    let text_h = eyebrow.as_ref().map_or(0.0, |g| g.size().y + 3.0) + title.size().y + body.as_ref().map_or(0.0, |g| g.size().y + 3.0);
    let content_w = eyebrow.as_ref().map_or(0.0f32, |g| g.size().x).max(title.size().x).max(body.as_ref().map_or(0.0, |g| g.size().x));
    let (w, h) = match size {
        Size::Compact => (content_w + PAD_X * 2.0 + 6.0, title.size().y + 20.0),
        Size::Card => ((content_w + CHIP + GAP + PAD_X * 2.0).clamp(MIN_W, MAX_W), text_h.max(CHIP) + PAD_Y * 2.0),
    };
    let rect = egui::Rect::from_center_size(centre, egui::vec2(w, h));
    let radius = match size {
        Size::Compact => egui::CornerRadius::same((h / 2.0) as u8),
        Size::Card => egui::CornerRadius::same(RADIUS),
    };

    // Painted straight onto the background layer: no widgets, no Area (whose
    // first frame is a hidden sizing pass).
    {
        let full = ctx.layer_painter(egui::LayerId::background());

        // --- the card, unfolding from its centre line ---------------------------
        if open > 0.001 {
            // A little room for the "+N" tab and the shadow, which hang past the card.
            let clip = egui::Rect::from_center_size(centre, egui::vec2(w + 70.0, (h + 22.0) * open));
            let p = full.with_clip_rect(clip);
            let card_a = ease_out(open);
            p.add(
                egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: alpha(egui::Color32::from_black_alpha(130), card_a) }
                    .as_shape(rect, radius),
            );
            // Fill carries a whisper of the accent; the stroke a little more.
            let fill = mix(egui::Color32::from_rgb(24, 28, 35), accent, 0.06);
            p.rect_filled(rect, radius, alpha(fill, card_a));
            p.rect_stroke(rect, radius, egui::Stroke::new(1.0, alpha(mix(egui::Color32::from_rgb(46, 54, 64), accent, 0.35), card_a)), egui::StrokeKind::Inside);

            match size {
                Size::Compact => {
                    p.galley(egui::pos2(rect.left() + PAD_X + 3.0, rect.center().y - title.size().y / 2.0), title.clone(), egui::Color32::WHITE);
                }
                Size::Card => {
                    // Tinted icon chip: the app's icon, or the kind's glyph.
                    let chip = egui::Rect::from_min_size(egui::pos2(rect.left() + PAD_X, rect.center().y - CHIP / 2.0), egui::vec2(CHIP, CHIP));
                    p.rect_filled(chip, egui::CornerRadius::same(15), alpha(accent, 0.16 * card_a));
                    match &t.icon_tex {
                        Some(tex) => {
                            let img = egui::Rect::from_center_size(chip.center(), egui::vec2(44.0, 44.0));
                            p.add(
                                egui::epaint::RectShape::filled(img, egui::CornerRadius::same(10), alpha(egui::Color32::WHITE, card_a))
                                    .with_texture(tex.id(), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))),
                            );
                        }
                        None => {
                            p.text(chip.center(), egui::Align2::CENTER_CENTER, t.kind.glyph(), egui::FontId::proportional(28.0), alpha(accent, card_a));
                        }
                    }
                    let mut y = rect.center().y - text_h / 2.0;
                    let x = chip.right() + GAP;
                    if let Some(g) = &eyebrow {
                        p.galley(egui::pos2(x, y), g.clone(), accent);
                        y += g.size().y + 3.0;
                    }
                    p.galley(egui::pos2(x, y), title.clone(), egui::Color32::WHITE);
                    y += title.size().y + 3.0;
                    if let Some(g) = &body {
                        p.galley(egui::pos2(x, y), g.clone(), theme::ON_SURFACE_VAR);
                    }
                }
            }

            // Dwell hairline along the bottom edge, draining toward the centre
            // (the entrance line, in reverse) — not for the moment-long readouts.
            if size == Size::Card && t.kind != Kind::Readout && t.secs >= 3.0 {
                let inset = RADIUS as f32;
                let half = (w / 2.0 - inset) * (1.0 - progress);
                if half > 0.5 {
                    let y = (rect.bottom() - 5.0).round();
                    let bar = egui::Rect::from_min_max(egui::pos2(rect.center().x - half, y), egui::pos2(rect.center().x + half, y + 3.0));
                    p.rect_filled(bar, egui::CornerRadius::ZERO, alpha(accent, 0.6 * card_a));
                }
            }

            // More waiting: a "+N" tab on the top-right corner.
            if queued > 0 && size == Size::Card {
                let label = format!("+{queued}");
                let g = ctx.fonts(|f| f.layout_no_wrap(label, egui::FontId::proportional(12.0), egui::Color32::BLACK));
                let tab = egui::Rect::from_center_size(egui::pos2(rect.right() - 26.0, rect.top() + 1.0), g.size() + egui::vec2(14.0, 6.0));
                p.rect_filled(tab, egui::CornerRadius::same(9), alpha(accent, card_a));
                p.galley(tab.min + egui::vec2(7.0, 3.0), g, egui::Color32::BLACK);
            }
        }

        // --- the accent line the card unfolds from / folds back into -------------
        let line_a = (1.0 - open * 1.4).clamp(0.0, 1.0);
        if line > 0.001 && line_a > 0.001 {
            let half = (w / 2.0) * line;
            let seg = egui::Rect::from_center_size(centre, egui::vec2(half * 2.0, 2.5));
            full.rect_filled(seg.expand2(egui::vec2(0.0, 5.0)), egui::CornerRadius::same(6), alpha(accent, 0.18 * line_a));
            full.rect_filled(seg, egui::CornerRadius::same(2), alpha(accent, line_a));
            full.rect_filled(seg.shrink2(egui::vec2(half * 0.55, 0.0)), egui::CornerRadius::same(2), alpha(egui::Color32::WHITE, 0.6 * line_a));
        }
    }
}

fn mix(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_lifecycle_and_ordering() {
        let mut ts = Toasts::new();
        let hmd = xr::Posef::IDENTITY;
        let t0 = Instant::now();
        ts.push(Toast::new(Kind::Notification, "a", "").secs(1.0));
        ts.push(Toast::new(Kind::Notification, "b", "").secs(1.0));
        ts.push(Toast::new(Kind::Timer, "alarm", ""));
        // Nothing shows without a head pose.
        assert_eq!(ts.tick(t0, None), None);
        // The alarm cut to the front.
        assert_eq!(ts.tick(t0, Some(&hmd)), Some(Kind::Timer));
        assert_eq!(ts.queue.len(), 2);
        let (line, open, _) = ts.current.as_ref().unwrap().phases(t0);
        assert!(line < 0.01 && open < 0.01);
        let mid = t0 + Duration::from_secs_f32(1.0);
        let (line, open, _) = ts.current.as_ref().unwrap().phases(mid);
        assert!(line > 0.99 && open > 0.99);
        // Past its lifetime it's gone and the next one starts.
        let later = t0 + Duration::from_secs_f32(Kind::Timer.dwell() + ENTER + EXIT + 0.01);
        assert_eq!(ts.tick(later, Some(&hmd)), Some(Kind::Notification));
        assert_eq!(ts.current.as_ref().unwrap().title, "a");
        // With one waiting, a notification is hurried along.
        let c = ts.current.as_ref().unwrap();
        assert!(c.until <= later + Duration::from_secs_f32(HURRY + EXIT + 0.001));
    }

    #[test]
    fn readout_updates_in_place_and_yields() {
        let mut ts = Toasts::new();
        let hmd = xr::Posef::IDENTITY;
        let t0 = Instant::now();
        ts.push(Toast::new(Kind::Info, "hint", ""));
        assert_eq!(ts.tick(t0, Some(&hmd)), Some(Kind::Info));
        ts.readout("Screen 1", "1.2 m", xr::Posef::IDENTITY);
        let shown = ts.current.as_ref().unwrap().shown_at;
        ts.readout("Screen 1", "1.3 m", xr::Posef::IDENTITY);
        assert_eq!(ts.current.as_ref().unwrap().shown_at, shown);
        assert_eq!(ts.current.as_ref().unwrap().body, "1.3 m");
        // The hint went back to the front of the queue.
        assert_eq!(ts.queue.len(), 1);
        assert_eq!(ts.queue.front().unwrap().kind, Kind::Info);
    }

    #[test]
    fn flood_keeps_the_newest() {
        let mut ts = Toasts::new();
        for i in 0..20 {
            ts.push(Toast::new(Kind::Notification, format!("n{i}"), ""));
        }
        assert_eq!(ts.queue.len(), QUEUE_CAP);
        assert_eq!(ts.queue.back().unwrap().title, "n19");
    }

    #[test]
    fn icon_accent_prefers_the_brand_colour() {
        // Discord-ish: blurple with white glyph pixels.
        let mut px = vec![egui::Color32::from_rgb(88, 101, 242); 64];
        px.extend(std::iter::repeat(egui::Color32::WHITE).take(32));
        let img = egui::ColorImage { size: [96, 1], pixels: px };
        let c = accent_from_icon(&img).unwrap();
        assert!(c.b() > c.r() && c.b() > c.g());
        let grey = egui::ColorImage { size: [4, 1], pixels: vec![egui::Color32::GRAY; 4] };
        assert!(accent_from_icon(&grey).is_none());
    }
}
