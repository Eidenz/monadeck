//! The play area's boundary (SteamVR's chaperone walls), which the overlay
//! draws: outlines on the floor, in the room setup's frame (the one every
//! device is tracked in, before the playspace offset).
//!
//! With SteamVR's Lighthouse driver it lives where SteamVR keeps it,
//! `collision_bounds` in `chaperone_info.vrchap` (one vertical quad per wall)
//! for the universe Monado uses: a boundary drawn in SteamVR's room setup shows
//! up as it is, and one drawn here goes back to SteamVR. Other tracking has no
//! vrchap; Monadeck keeps the outline in its data dir instead.

use crate::paths::monadeck_data_dir;
use crate::room_setup;
use serde_json::{json, Value};
use std::path::PathBuf;

/// How tall the walls are written (SteamVR's own: 8 ft).
pub const WALL_HEIGHT: f64 = 2.4384;

/// Points closer than this are the same corner (metres).
const SAME_POINT: f64 = 0.01;
/// SteamVR's quick-calibration play area, back once the walls are cleared.
const QUICK_PLAY_AREA: [f64; 2] = [3.0, 2.0];

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Boundary {
    /// Closed outlines, each corner as (x, z) on the floor.
    pub loops: Vec<Vec<[f64; 2]>>,
}

/// The 3 × 2 m area SteamVR's quick calibration (and Monadeck's room setup)
/// leaves behind: no boundary anyone drew.
pub(crate) fn placeholder() -> Value {
    json!([
        [[1.5, 0, 1], [1.5, 5, 1], [1.5, 5, -1], [1.5, 0, -1]],
        [[1.5, 0, -1], [1.5, 5, -1], [-1.5, 5, -1], [-1.5, 0, -1]],
        [[-1.5, 0, -1], [-1.5, 5, -1], [-1.5, 5, 1], [-1.5, 0, 1]],
        [[-1.5, 0, 1], [-1.5, 5, 1], [1.5, 5, 1], [1.5, 0, 1]]
    ])
}

fn own_path() -> PathBuf {
    monadeck_data_dir().join("boundary.json")
}

/// Whether a boundary can be kept: SteamVR's Lighthouse tracking needs a room
/// setup first (its universe entry is where the walls go, and without one the
/// floor isn't at zero).
pub fn ready(lighthouse: bool) -> bool {
    !lighthouse || room_setup::current_entry().is_some()
}

/// The boundary in effect, None when nobody drew one.
pub fn load(lighthouse: bool) -> Option<Boundary> {
    load_with_play_area(lighthouse).map(|(b, _)| b)
}

/// The boundary in effect, and (SteamVR's files only) the play area games
/// are told about with it: SteamVR's `play_area`, whoever wrote it.
pub fn load_with_play_area(lighthouse: bool) -> Option<(Boundary, Option<[f64; 2]>)> {
    let (b, area) = if lighthouse {
        let (_, entry) = room_setup::current_entry()?;
        let size = entry["play_area"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>());
        let area = size.filter(|v| v.len() == 2 && v[0] > 0.1 && v[1] > 0.1).map(|v| [v[0], v[1]]);
        (from_collision_bounds(&entry["collision_bounds"]), area)
    } else {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(own_path()).ok()?).ok()?;
        (Boundary { loops: loops_from(&v["loops"]) }, None)
    };
    (!b.loops.is_empty() && !is_placeholder(&b)).then_some((b, area))
}

/// Keep `b` as the boundary (with SteamVR's files, the play area games get
/// along with it).
pub fn save(lighthouse: bool, b: &Boundary) -> Result<(), String> {
    if lighthouse {
        room_setup::set_walls(to_collision_bounds(b), play_area_json(b))
    } else {
        let loops: Vec<Vec<[f64; 2]>> = b.loops.clone();
        let text = serde_json::to_string_pretty(&json!({ "loops": loops })).map_err(|e| e.to_string())?;
        let path = own_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("Couldn't write {}: {e}", path.display()))
    }
}

/// Forget the boundary (SteamVR's file gets its quick-calibration box back).
pub fn clear(lighthouse: bool) -> Result<(), String> {
    if lighthouse {
        room_setup::set_walls(placeholder(), json!(QUICK_PLAY_AREA))
    } else {
        match std::fs::remove_file(own_path()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("Couldn't remove the boundary: {e}")),
            _ => Ok(()),
        }
    }
}

/// Rewrite games' play area from the walls: a boundary saved before Monadeck
/// wrote one still has SteamVR's quick-calibration 3 × 2 m.
pub fn refresh_play_area() -> Result<(), String> {
    let (_, entry) = room_setup::current_entry().ok_or("There's no room setup yet.")?;
    let walls = entry["collision_bounds"].clone();
    let area = play_area_for_walls(&walls);
    room_setup::set_walls(walls, area)
}

/// SteamVR's quick-calibration play area: what a boundary saved before
/// Monadeck wrote play areas still carries.
pub fn is_quick_play_area(area: [f64; 2]) -> bool {
    area == QUICK_PLAY_AREA
}

/// Move the room setup's centre into the middle of the roomiest rectangle in
/// the boundary (facing the same way), like SteamVR's room setup: games get
/// the most room, and start you in its middle. The play area games get then.
pub fn centre_play_area() -> Result<[f64; 2], String> {
    let (universe, entry) = room_setup::current_entry().ok_or("There's no room setup yet.")?;
    let b = from_collision_bounds(&entry["collision_bounds"]);
    if b.loops.is_empty() || is_placeholder(&b) {
        return Err("Draw a boundary first.".into());
    }
    let (centre, size) = roomiest(&b).ok_or("No rectangle fits in this boundary.")?;
    room_setup::shift_centre(&universe, centre)?;
    Ok(size)
}

// --- The play area ---------------------------------------------------------------------
// A rectangle with its sides along the room setup's axes (x across, z
// forward-back): OpenVR and OpenXR both have games' play area centred on the
// origin, so it's the largest one centred there that fits.

/// Rectangle half-widths are tried this far apart (metres).
const RECT_STEP: f64 = 0.01;

/// SteamVR's `play_area` for `b`: [0, 0] when no rectangle fits around the
/// origin (it's outside the walls).
fn play_area_json(b: &Boundary) -> Value {
    json!(play_area(b).unwrap_or([0.0, 0.0]))
}

/// The play area games get: the largest rectangle centred on the room
/// setup's origin that fits inside the walls. (width along x, depth along z);
/// None when the origin is outside them.
pub fn play_area(b: &Boundary) -> Option<[f64; 2]> {
    centred_rect(&sides(&b.loops), [0.0, 0.0], RECT_STEP)
}

/// Where the largest rectangle fits anywhere inside the walls: its centre,
/// and its size.
pub fn roomiest(b: &Boundary) -> Option<([f64; 2], [f64; 2])> {
    let sides = sides(&b.loops);
    let points = || b.loops.iter().flatten();
    let (min, max) = points().fold(([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]), |(lo, hi), p| {
        ([lo[0].min(p[0]), lo[1].min(p[1])], [hi[0].max(p[0]), hi[1].max(p[1])])
    });
    let largest = |centres: Vec<[f64; 2]>, step: f64| {
        centres
            .into_iter()
            .filter_map(|c| Some((c, centred_rect(&sides, c, step)?)))
            .max_by(|(_, a), (_, b)| (a[0] * a[1]).total_cmp(&(b[0] * b[1])))
    };
    // Coarse over the whole room, then finer around the best spot.
    let coarse = 0.2;
    let across = |lo: f64, hi: f64| (0..).map(move |i| lo + coarse * (i as f64 + 0.5)).take_while(move |v| *v < hi);
    let grid = across(min[0], max[0]).flat_map(|x| across(min[1], max[1]).map(move |z| [x, z])).collect();
    let (around, _) = largest(grid, 0.02)?;
    let near = (-10..=10).flat_map(|i| (-10..=10).map(move |j| [around[0] + i as f64 * 0.02, around[1] + j as f64 * 0.02])).collect();
    largest(near, RECT_STEP)
}

fn sides(loops: &[Vec<[f64; 2]>]) -> Vec<([f64; 2], [f64; 2])> {
    loops.iter().flat_map(|l| (0..l.len()).map(move |i| (l[i], l[(i + 1) % l.len()]))).collect()
}

/// Inside the walls (even-odd).
fn inside(sides: &[([f64; 2], [f64; 2])], p: [f64; 2]) -> bool {
    sides
        .iter()
        .filter(|(a, b)| (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]))
        .count()
        % 2
        == 1
}

/// How far up and down from `c` a rectangle `half_w` either side of it can
/// reach before a wall cuts in.
fn half_depth(sides: &[([f64; 2], [f64; 2])], c: [f64; 2], half_w: f64) -> f64 {
    let mut room = f64::INFINITY;
    for (a, b) in sides {
        let (a, b) = ([a[0] - c[0], a[1] - c[1]], [b[0] - c[0], b[1] - c[1]]);
        let (lo, hi) = (a[0].min(b[0]), a[0].max(b[0]));
        if hi < -half_w || lo > half_w {
            continue;
        }
        // The side's z where it's within the rectangle's width.
        let (z0, z1) = if hi - lo < 1e-12 {
            (a[1], b[1])
        } else {
            let z = |x: f64| a[1] + (b[1] - a[1]) * (x - a[0]) / (b[0] - a[0]);
            (z(lo.max(-half_w)), z(hi.min(half_w)))
        };
        if z0.signum() != z1.signum() || z0 == 0.0 {
            return 0.0;
        }
        room = room.min(z0.abs().min(z1.abs()));
    }
    room
}

/// The largest rectangle centred on `c` that fits, half-widths tried `step`
/// apart: (width, depth).
fn centred_rect(sides: &[([f64; 2], [f64; 2])], c: [f64; 2], step: f64) -> Option<[f64; 2]> {
    if !inside(sides, c) {
        return None;
    }
    let reach = sides.iter().map(|(a, _)| (a[0] - c[0]).abs()).fold(0.0, f64::max);
    let mut best = (0.0, 0.0, 0.0);
    let mut half_w = step;
    while half_w <= reach {
        let half_d = half_depth(sides, c, half_w);
        if half_d <= 0.0 {
            break;
        }
        if half_w * half_d > best.0 {
            best = (half_w * half_d, half_w, half_d);
        }
        half_w += step;
    }
    (best.0 > 0.0 && best.2.is_finite()).then_some([best.1 * 2.0, best.2 * 2.0])
}

fn loops_from(v: &Value) -> Vec<Vec<[f64; 2]>> {
    let point = |p: &Value| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?]);
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l.as_array().map(|ps| ps.iter().filter_map(point).collect::<Vec<_>>()))
        .filter(|l| l.len() >= 3)
        .collect()
}

fn same(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < SAME_POINT
}

/// SteamVR's walls back into outlines: each quad's bottom edge, chained while
/// one wall starts where the last ended.
pub(crate) fn from_collision_bounds(v: &Value) -> Boundary {
    let mut loops: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut chain: Vec<[f64; 2]> = Vec::new();
    let mut flush = |chain: &mut Vec<[f64; 2]>| {
        if chain.len() >= 2 && same(chain[0], chain[chain.len() - 1]) {
            chain.pop();
        }
        if chain.len() >= 3 {
            loops.push(std::mem::take(chain));
        }
        chain.clear();
    };
    for quad in v.as_array().into_iter().flatten() {
        let pts: Vec<[f64; 3]> = quad
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?, p.get(2)?.as_f64()?]))
            .collect();
        let Some(floor) = pts.iter().map(|p| p[1]).reduce(f64::min) else { continue };
        let bottom: Vec<[f64; 2]> = pts.iter().filter(|p| p[1] - floor < 0.05).map(|p| [p[0], p[2]]).collect();
        let (Some(&a), Some(&b)) = (bottom.first(), bottom.last()) else { continue };
        if same(a, b) {
            continue;
        }
        match chain.last() {
            Some(&end) if same(end, a) => chain.push(b),
            _ => {
                flush(&mut chain);
                chain.extend([a, b]);
            }
        }
    }
    flush(&mut chain);
    Boundary { loops }
}

pub(crate) fn to_collision_bounds(b: &Boundary) -> Value {
    let h = WALL_HEIGHT;
    let mut walls = Vec::new();
    for l in &b.loops {
        for (i, a) in l.iter().enumerate() {
            let b = l[(i + 1) % l.len()];
            walls.push(json!([[a[0], 0.0, a[1]], [a[0], h, a[1]], [b[0], h, b[1]], [b[0], 0.0, b[1]]]));
        }
    }
    Value::Array(walls)
}

pub(crate) fn play_area_for_walls(walls: &Value) -> Value {
    play_area_json(&from_collision_bounds(walls))
}

fn is_placeholder(b: &Boundary) -> bool {
    let p = from_collision_bounds(&placeholder());
    b.loops.len() == 1 && b.loops[0].len() == p.loops[0].len() && b.loops[0].iter().zip(&p.loops[0]).all(|(a, b)| same(*a, *b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quick_calibration_box_is_no_boundary() {
        let b = from_collision_bounds(&placeholder());
        assert_eq!(b.loops, vec![vec![[1.5, 1.0], [1.5, -1.0], [-1.5, -1.0], [-1.5, 1.0]]]);
        assert!(is_placeholder(&b));
    }

    #[test]
    fn walls_round_trip() {
        let b = Boundary { loops: vec![vec![[0.0, 0.0], [2.5, 0.2], [2.4, -3.0], [-0.3, -2.0], [-1.0, -0.8]]] };
        let back = from_collision_bounds(&to_collision_bounds(&b));
        assert_eq!(back.loops.len(), 1);
        assert!(back.loops[0].iter().zip(&b.loops[0]).all(|(a, b)| same(*a, *b)));
        assert!(!is_placeholder(&back));
    }

    fn close(a: [f64; 2], b: [f64; 2], tol: f64) -> bool {
        (a[0] - b[0]).abs() <= tol && (a[1] - b[1]).abs() <= tol
    }

    #[test]
    fn play_area_is_centred_on_the_origin() {
        // A 4 × 3 m room with the origin 0.5 m right of its middle.
        let room = Boundary { loops: vec![vec![[-2.5, -1.5], [1.5, -1.5], [1.5, 1.5], [-2.5, 1.5]]] };
        let p = play_area(&room).unwrap();
        assert!(close(p, [3.0, 3.0], 0.021), "{p:?}");
        // Anywhere, it'd be the whole room, centred half a metre left.
        let (c, r) = roomiest(&room).unwrap();
        assert!(close(c, [-0.5, 0.0], 0.03), "{c:?}");
        assert!(close(r, [4.0, 3.0], 0.03), "{r:?}");
        // Outside the walls: none.
        let away = Boundary { loops: vec![vec![[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0]]] };
        assert_eq!(play_area(&away), None);
    }

    #[test]
    fn a_corner_cut_into_the_room_keeps_out_of_the_play_area() {
        // An L: the top right quarter of a 4 × 4 m square is furniture.
        let l = Boundary { loops: vec![vec![[-2.0, -2.0], [2.0, -2.0], [2.0, 0.5], [0.5, 0.5], [0.5, 2.0], [-2.0, 2.0]]] };
        let [w, d] = play_area(&l).unwrap();
        // Whatever it picked stays clear of the cut-out corner at (0.5, 0.5).
        assert!(w / 2.0 <= 0.5 + 0.011 || d / 2.0 <= 0.5 + 0.011, "{w} × {d}");
        assert!(w * d > 3.9, "{w} × {d}");
    }

    #[test]
    fn steamvr_walls_with_two_rooms_split_into_loops() {
        // Two triangles, walls written top-first: still read by their bottoms.
        let tri = |o: f64| {
            let p = [[o, 0.0], [o + 1.0, 0.0], [o, 1.0]];
            (0..3)
                .map(|i| {
                    let (a, b) = (p[i], p[(i + 1) % 3]);
                    json!([[a[0], 2.0, a[1]], [a[0], 0.0, a[1]], [b[0], 0.0, b[1]], [b[0], 2.0, b[1]]])
                })
                .collect::<Vec<_>>()
        };
        let v = Value::Array([tri(0.0), tri(5.0)].concat());
        let b = from_collision_bounds(&v);
        assert_eq!(b.loops.len(), 2);
        assert_eq!(b.loops[1], vec![[5.0, 0.0], [6.0, 0.0], [5.0, 1.0]]);
    }
}
