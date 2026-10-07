//! The play area's boundary: drawing it (Settings › Boundary: a trigger
//! click drops a corner, holding it down traces), and when its walls show
//! (near one, or everywhere once you're outside). The 3D layer draws them
//! (`scene::Walls`); `monadeck_core::boundary` keeps them.

use openxr as xr;

use crate::mathx::{pose_compose, pose_invert, quat_rotate};
use crate::scene::{self, math, Item, Walls};

/// A point on the floor: (x, z) in the room setup's frame.
pub type P2 = [f32; 2];

/// Traced points are taken this far apart, then thinned to within
/// `TRACE_TOLERANCE` of the path (hand wobble and straight stretches go).
const TRACE_STEP: f32 = 0.05;
const TRACE_TOLERANCE: f32 = 0.04;
/// A press that travels less than this drops a corner instead of tracing.
const TRACE_MIN: f32 = 0.15;
/// Ending this close to the first corner closes the outline.
const CLOSE_DIST: f32 = 0.3;
/// Smaller than this (square metres) is a slip, not a room.
const MIN_AREA: f32 = 0.5;

/// What the Boundary page asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Start,
    Undo,
    Restart,
    Save,
    Cancel,
    Clear,
    /// Move the room setup's centre into the roomiest spot of the boundary.
    Centre,
}

/// A boundary's size, for the page.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Outline {
    pub corners: usize,
    /// Metres around (an outline still being drawn: so far).
    pub length: f32,
    pub area: f32,
}

// --- Geometry ----------------------------------------------------------------------

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

pub fn dist(a: P2, b: P2) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// How far `p` is from the segment `a`–`b`, and the closest point on it.
pub fn segment_distance(p: P2, a: P2, b: P2) -> (f32, P2) {
    let ab = sub(b, a);
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 < 1e-9 { 0.0 } else { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0) };
    let c = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    (dist(p, c), c)
}

/// Each loop's sides, closing ones included.
pub fn sides(l: &[P2]) -> impl Iterator<Item = (P2, P2)> + '_ {
    (0..l.len()).map(move |i| (l[i], l[(i + 1) % l.len()]))
}

/// Inside the outlines (even-odd: a loop inside another is a hole).
pub fn contains(loops: &[Vec<P2>], p: P2) -> bool {
    let mut inside = false;
    for l in loops {
        for (a, b) in sides(l) {
            if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                inside = !inside;
            }
        }
    }
    inside
}

/// The nearest wall's distance, over every outline.
pub fn nearest_wall(loops: &[Vec<P2>], p: P2) -> f32 {
    loops.iter().flat_map(|l| sides(l)).map(|(a, b)| segment_distance(p, a, b).0).fold(f32::INFINITY, f32::min)
}

fn cross(o: P2, a: P2, b: P2) -> f32 {
    let (u, v) = (sub(a, o), sub(b, o));
    u[0] * v[1] - u[1] * v[0]
}

fn segments_cross(a: P2, b: P2, c: P2, d: P2) -> bool {
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

/// Two sides that aren't neighbours cross.
pub fn crosses_itself(l: &[P2]) -> bool {
    let n = l.len();
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue; // the closing side meets the first
            }
            if segments_cross(l[i], l[(i + 1) % n], l[j], l[(j + 1) % n]) {
                return true;
            }
        }
    }
    false
}

pub fn area(l: &[P2]) -> f32 {
    (sides(l).map(|(a, b)| a[0] * b[1] - b[0] * a[1]).sum::<f32>() / 2.0).abs()
}

/// Ramer–Douglas–Peucker: the path with every point within `tol` of it gone.
pub fn simplify(points: &[P2], tol: f32) -> Vec<P2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (first, last) = (points[0], points[points.len() - 1]);
    let (mut worst, mut at) = (0.0, 0);
    for (i, p) in points.iter().enumerate().take(points.len() - 1).skip(1) {
        let d = segment_distance(*p, first, last).0;
        if d > worst {
            (worst, at) = (d, i);
        }
    }
    if worst <= tol {
        return vec![first, last];
    }
    let mut out = simplify(&points[..=at], tol);
    out.pop();
    out.extend(simplify(&points[at..], tol));
    out
}

pub fn outline(loops: &[Vec<P2>]) -> Outline {
    Outline {
        corners: loops.iter().map(Vec::len).sum(),
        length: loops.iter().flat_map(|l| sides(l)).map(|(a, b)| dist(a, b)).sum(),
        area: loops.iter().map(|l| area(l)).sum(),
    }
}

pub fn to_store(loops: &[Vec<P2>]) -> monadeck_core::boundary::Boundary {
    monadeck_core::boundary::Boundary { loops: loops.iter().map(|l| l.iter().map(|p| [p[0] as f64, p[1] as f64]).collect()).collect() }
}

pub fn from_store(b: &monadeck_core::boundary::Boundary) -> Vec<Vec<P2>> {
    b.loops.iter().map(|l| l.iter().map(|p| [p[0] as f32, p[1] as f32]).collect()).collect()
}

// --- Drawing one ---------------------------------------------------------------------

/// What a trigger did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Pressed: a corner went down where the controller is (a trace may follow).
    Pressed,
    Corner,
    Traced,
    /// Back at the first corner: the outline is done.
    Closed,
}

struct Held {
    hand: usize,
    /// Points before the press.
    start: usize,
    travel: f32,
}

/// A boundary being drawn.
#[derive(Default)]
pub struct Setup {
    pub points: Vec<P2>,
    /// Per corner or trace, the points before it (what Undo goes back to).
    steps: Vec<usize>,
    held: Option<Held>,
    down_prev: [bool; 2],
    last_hand: Option<usize>,
}

impl Setup {
    /// One hand's trigger this frame, and the floor under its controller
    /// (None while it isn't placing: pointing at a panel, or untracked).
    pub fn feed(&mut self, hand: usize, down: bool, at: Option<P2>) -> Option<Event> {
        let pressed = down && !self.down_prev[hand];
        self.down_prev[hand] = down;
        match &mut self.held {
            Some(h) if h.hand == hand => {
                if let (Some(p), Some(&last)) = (at, self.points.last()) {
                    let d = dist(p, last);
                    if d >= TRACE_STEP {
                        self.points.push(p);
                        h.travel += d;
                    }
                }
                if down {
                    return None;
                }
                let h = self.held.take().expect("held");
                Some(self.finish(h))
            }
            // The other hand is drawing.
            Some(_) => None,
            None => {
                let p = at.filter(|_| pressed)?;
                self.held = Some(Held { hand, start: self.points.len(), travel: 0.0 });
                self.last_hand = Some(hand);
                self.points.push(p);
                Some(Event::Pressed)
            }
        }
    }

    fn finish(&mut self, h: Held) -> Event {
        let traced = h.travel >= TRACE_MIN;
        if traced {
            let stroke = simplify(&self.points[h.start..], TRACE_TOLERANCE);
            self.points.truncate(h.start);
            self.points.extend(stroke);
        } else {
            self.points.truncate(h.start + 1);
        }
        self.steps.push(h.start);
        // Back at the first corner, once there's a shape (a lone trace around
        // the room closes on itself too; one that overshoots is cut there).
        if let Some(&first) = self.points.first() {
            let mut along = 0.0;
            for i in 1..self.points.len() {
                along += dist(self.points[i - 1], self.points[i]);
                let candidate = if traced { i > h.start } else { i == h.start };
                if candidate && i >= 3 && along > 1.0 && dist(self.points[i], first) < CLOSE_DIST {
                    self.points.truncate(i);
                    return Event::Closed;
                }
            }
        }
        if traced {
            Event::Traced
        } else {
            Event::Corner
        }
    }

    /// The hand drawing, or the last that did (the right one to begin with).
    pub fn hand(&self) -> usize {
        self.last_hand.unwrap_or(1)
    }

    pub fn undo(&mut self) {
        if self.held.is_none() {
            if let Some(n) = self.steps.pop() {
                self.points.truncate(n);
            }
        }
    }

    pub fn restart(&mut self) {
        *self = Self { down_prev: self.down_prev, last_hand: self.last_hand, ..Self::default() };
    }

    /// Keep the outline as the boundary.
    pub fn save(&self, lighthouse: bool) -> Result<(), String> {
        let l = self.result().map_err(str::to_string)?;
        monadeck_core::boundary::save(lighthouse, &to_store(&[l]))
    }

    /// The outline, if it makes a boundary.
    pub fn result(&self) -> Result<Vec<P2>, &'static str> {
        if self.points.len() < 3 {
            return Err("A boundary needs at least 3 corners");
        }
        if crosses_itself(&self.points) {
            return Err("The outline crosses itself · undo back past the crossing");
        }
        if area(&self.points) < MIN_AREA {
            return Err("That's too small for a play area");
        }
        Ok(self.points.clone())
    }

    pub fn outline(&self) -> Outline {
        let length = self.points.windows(2).map(|w| dist(w[0], w[1])).sum();
        Outline { corners: self.points.len(), length, area: if self.points.len() >= 3 { area(&self.points) } else { 0.0 } }
    }
}

// --- What the 3D layer shows ------------------------------------------------------------

/// Where things are this frame, in the eye views' space.
pub struct View {
    /// The room setup's frame.
    pub room: xr::Posef,
    pub head: Option<xr::Posef>,
    /// Each controller's aim pose (its tip).
    pub hands: [Option<xr::Posef>; 2],
    pub trackers: Vec<xr::Posef>,
    pub reach: f32,
    /// The outline shows on the floor.
    pub floor: bool,
}

impl View {
    fn in_room(&self, p: &xr::Posef) -> [f32; 3] {
        math::pos(&pose_compose(&pose_invert(&self.room), p))
    }

    fn in_space(&self, p: [f32; 3]) -> [f32; 3] {
        let r = quat_rotate(math::quat(&self.room), p);
        let o = math::pos(&self.room);
        [r[0] + o[0], r[1] + o[1], r[2] + o[2]]
    }
}

/// The boundary kept: walls near whatever comes close to them, all of them
/// once the head is outside, the outline on the floor. None while there's
/// nothing to show (no layer, no cost).
pub fn walls(loops: &[Vec<P2>], v: &View) -> Option<Item> {
    let head = v.head.as_ref().map(|h| v.in_room(h));
    let near: Vec<[f32; 3]> = head.into_iter().chain(v.hands.iter().flatten().map(|p| v.in_room(p))).chain(v.trackers.iter().map(|p| v.in_room(p))).collect();
    let outside = head.is_some_and(|h| !contains(loops, [h[0], h[2]]));
    let close = near.iter().any(|p| nearest_wall(loops, [p[0], p[2]]) < v.reach);
    (outside || close || v.floor).then(|| {
        Item::Walls(Box::new(Walls {
            room: v.room,
            loops: loops.to_vec(),
            open: false,
            near,
            reach: v.reach,
            base: if outside { 1.0 } else { 0.0 },
            floor: if v.floor { 0.8 } else { 0.0 },
        }))
    })
}

/// A boundary being drawn: a mark at each corner, a line down to the floor
/// under each controller, and the outline so far reaching to the hand that
/// draws it (its walls faint, the side that would close it a hint).
pub fn setup_items(s: &Setup, v: &View) -> Vec<Item> {
    let mut items = Vec::new();
    let mut live = None;
    for (hi, hand) in v.hands.iter().enumerate() {
        let Some(p) = hand else { continue };
        let tip = v.in_room(p);
        let floor = [tip[0], 0.0, tip[2]];
        items.push(Item::Rod { from: v.in_space(floor), to: math::pos(p), radius: 0.003, tint: scene::MARKER_TINT });
        items.push(Item::Dot { at: v.in_space(floor), radius: 0.014, tint: scene::MARKER_TINT });
        if hi == s.hand() {
            live = Some([tip[0], tip[2]]);
        }
    }
    for c in &s.points {
        items.push(Item::Dot { at: v.in_space([c[0], 0.0, c[1]]), radius: 0.022, tint: scene::MARKER_TINT });
    }
    let mut outline = s.points.clone();
    outline.extend(live.filter(|l| outline.last().is_none_or(|p| dist(*p, *l) > 0.02)));
    if outline.len() >= 2 {
        items.push(Item::Walls(Box::new(Walls { room: v.room, loops: vec![outline], open: true, near: Vec::new(), reach: v.reach, base: 0.12, floor: 1.0 })));
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<P2> {
        vec![[0.0, 0.0], [3.0, 0.0], [3.0, 2.0], [0.0, 2.0]]
    }

    #[test]
    fn inside_and_distance() {
        let l = vec![square()];
        assert!(contains(&l, [1.0, 1.0]));
        assert!(!contains(&l, [3.5, 1.0]));
        assert!((nearest_wall(&l, [2.6, 1.0]) - 0.4).abs() < 1e-5);
        assert!((nearest_wall(&l, [4.0, 1.0]) - 1.0).abs() < 1e-5);
        assert!((area(&square()) - 6.0).abs() < 1e-5);
    }

    #[test]
    fn crossings() {
        assert!(!crosses_itself(&square()));
        // A bow tie.
        assert!(crosses_itself(&[[0.0, 0.0], [2.0, 2.0], [2.0, 0.0], [0.0, 2.0]]));
    }

    #[test]
    fn simplify_keeps_corners_drops_wobble() {
        let mut path = Vec::new();
        for i in 0..=30 {
            path.push([i as f32 * 0.1, if i % 2 == 0 { 0.01 } else { -0.01 }]);
        }
        for i in 1..=20 {
            path.push([3.0, i as f32 * 0.1]);
        }
        let s = simplify(&path, TRACE_TOLERANCE);
        assert_eq!(s.len(), 3, "{s:?}");
        assert!(dist(s[1], [3.0, 0.0]) < 0.05);
    }

    #[test]
    fn corners_close_on_the_first() {
        let mut s = Setup::default();
        for p in square() {
            assert_eq!(s.feed(1, true, Some(p)), Some(Event::Pressed));
            assert_eq!(s.feed(1, false, Some(p)), Some(Event::Corner));
        }
        s.feed(1, true, Some([0.1, 0.1]));
        assert_eq!(s.feed(1, false, Some([0.1, 0.1])), Some(Event::Closed));
        assert_eq!(s.result().unwrap(), square());
    }

    #[test]
    fn a_wobbly_press_is_still_a_corner_and_undo_takes_it_back() {
        let mut s = Setup::default();
        s.feed(0, true, Some([1.0, 1.0]));
        s.feed(0, true, Some([1.06, 1.0]));
        assert_eq!(s.feed(0, false, Some([1.08, 1.0])), Some(Event::Corner));
        assert_eq!(s.points, vec![[1.0, 1.0]]);
        s.undo();
        assert!(s.points.is_empty());
    }

    #[test]
    fn one_trace_around_the_room_closes_itself() {
        let mut s = Setup::default();
        let mut walk = Vec::new();
        for (a, b) in sides(&square()) {
            for k in 0..20 {
                let t = k as f32 / 20.0;
                walk.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            }
        }
        // Overshoot past the start.
        walk.extend([[0.0, 0.0], [0.15, 0.0], [0.3, 0.0]]);
        s.feed(0, true, Some(walk[0]));
        for p in &walk[1..] {
            s.feed(0, true, Some(*p));
        }
        assert_eq!(s.feed(0, false, None), Some(Event::Closed));
        let r = s.result().unwrap();
        assert_eq!(r.len(), 4, "{r:?}");
        assert!((area(&r) - 6.0).abs() < 0.2);
    }

    #[test]
    fn only_one_hand_draws_at_a_time() {
        let mut s = Setup::default();
        s.feed(0, true, Some([0.0, 0.0]));
        assert_eq!(s.feed(1, true, Some([2.0, 0.0])), None);
        assert_eq!(s.feed(0, false, None), Some(Event::Corner));
        assert_eq!(s.points.len(), 1);
    }
}
