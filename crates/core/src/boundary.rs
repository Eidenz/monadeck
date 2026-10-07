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
    let b = if lighthouse {
        let (_, entry) = room_setup::current_entry()?;
        from_collision_bounds(&entry["collision_bounds"])
    } else {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(own_path()).ok()?).ok()?;
        Boundary { loops: loops_from(&v["loops"]) }
    };
    (!b.loops.is_empty() && !is_placeholder(&b)).then_some(b)
}

/// Keep `b` as the boundary.
pub fn save(lighthouse: bool, b: &Boundary) -> Result<(), String> {
    if lighthouse {
        room_setup::set_collision_bounds(to_collision_bounds(b))
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
        room_setup::set_collision_bounds(placeholder())
    } else {
        match std::fs::remove_file(own_path()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("Couldn't remove the boundary: {e}")),
            _ => Ok(()),
        }
    }
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
