//! The boundary's meshes, rebuilt every frame it shows: walls cut into small
//! cells whose vertices carry how strongly they show (a glow around whatever
//! comes close, nothing further off), and flat lines on the floor.
use openxr as xr;

use super::models::Vertex;
use crate::boundary::{dist, segment_distance, sides, P2};

/// How tall the walls are drawn (metres; they thin out towards the top).
pub const HEIGHT: f32 = 2.4;
/// Mesh cells: fine enough for the glow to look round.
const CELL: f32 = 0.1;
/// The floor lines' width.
const LINE: f32 = 0.03;

/// The boundary, as the 3D layer draws it.
pub struct Walls {
    /// The room setup's frame, in the eye views' space.
    pub room: xr::Posef,
    /// Outlines on the floor, (x, z) in the room setup's frame.
    pub loops: Vec<Vec<P2>>,
    /// The last outline is still being drawn: its closing side is only hinted.
    pub open: bool,
    /// What brings the walls up (room frame), and from how far.
    pub near: Vec<[f32; 3]>,
    pub reach: f32,
    /// Every wall shows at least this much (fully once you're outside).
    pub base: f32,
    /// The outline on the floor (0: none).
    pub floor: f32,
}

pub type MeshData = (Vec<Vertex>, Vec<u32>);

/// Something near a side: how close it came (0 at `reach`, 1 right up
/// against it), and where it is against the wall.
struct Approach {
    k: f32,
    at: P2,
    y: f32,
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn approaches(a: P2, b: P2, near: &[[f32; 3]], reach: f32) -> Vec<Approach> {
    near.iter()
        .filter_map(|p| {
            let (d, at) = segment_distance([p[0], p[2]], a, b);
            let k = smooth((reach - d) / (reach * 0.75));
            (k > 0.0).then_some(Approach { k, at, y: p[1] })
        })
        .collect()
}

/// How strongly the wall shows at `(xz, y)`: a patch around each approach
/// that grows as it gets closer.
fn strength(xz: P2, y: f32, near: &[Approach], base: f32) -> f32 {
    near.iter().fold(base, |s, n| {
        let radius = 0.45 + 0.6 * n.k;
        let r = dist(xz, n.at).hypot(y - n.y);
        s.max(n.k * (1.0 - smooth((r - radius * 0.35) / (radius * 0.65))))
    })
}

/// Both faces of a quad grid's cells (walls are seen from either side).
fn both_faces(idx: &mut Vec<u32>, a: u32, b: u32, c: u32, d: u32) {
    idx.extend([a, b, c, b, d, c, a, c, b, b, c, d]);
}

pub fn build(w: &Walls, wall: &mut MeshData, line: &mut MeshData) {
    wall.0.clear();
    wall.1.clear();
    line.0.clear();
    line.1.clear();
    let last = w.loops.len().saturating_sub(1);
    for (li, l) in w.loops.iter().enumerate() {
        if l.len() < 2 {
            continue;
        }
        let mut along = 0.0;
        for (si, (a, b)) in sides(l).enumerate() {
            let closing = si == l.len() - 1;
            if closing && l.len() < 3 {
                break;
            }
            let hint = w.open && li == last && closing;
            let len = dist(a, b);
            if w.floor > 0.0 {
                floor_line(line, a, b, if hint { w.floor * 0.35 } else { w.floor });
            }
            if !hint {
                let near = approaches(a, b, &w.near, w.reach);
                if w.base > 0.0 || !near.is_empty() {
                    wall_side(wall, a, b, along, &near, w.base);
                }
            }
            along += len;
        }
    }
}

fn wall_side((v, idx): &mut MeshData, a: P2, b: P2, along: f32, near: &[Approach], base: f32) {
    let len = dist(a, b);
    let cols = ((len / CELL).ceil() as u32).max(1);
    let rows = (HEIGHT / CELL).ceil() as u32;
    let first = v.len() as u32;
    for c in 0..=cols {
        let t = c as f32 / cols as f32;
        let xz = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        for r in 0..=rows {
            let y = HEIGHT * r as f32 / rows as f32;
            v.push(Vertex { pos: [xz[0], y, xz[1]], nrm: [strength(xz, y, near, base), 0.0, 0.0], uv: [along + len * t, y] });
        }
    }
    let at = |c: u32, r: u32| first + c * (rows + 1) + r;
    for c in 0..cols {
        for r in 0..rows {
            both_faces(idx, at(c, r), at(c + 1, r), at(c, r + 1), at(c + 1, r + 1));
        }
    }
}

fn floor_line((v, idx): &mut MeshData, a: P2, b: P2, strength: f32) {
    let len = dist(a, b).max(1e-4);
    let d = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
    let (h, n) = (LINE / 2.0, [-d[1] * LINE / 2.0, d[0] * LINE / 2.0]);
    // Overlapping at the ends, so corners meet.
    let (a, b) = ([a[0] - d[0] * h, a[1] - d[1] * h], [b[0] + d[0] * h, b[1] + d[1] * h]);
    let first = v.len() as u32;
    for p in [[a[0] + n[0], a[1] + n[1]], [b[0] + n[0], b[1] + n[1]], [a[0] - n[0], a[1] - n[1]], [b[0] - n[0], b[1] - n[1]]] {
        v.push(Vertex { pos: [p[0], 0.004, p[1]], nrm: [strength, 0.0, 0.0], uv: [0.0; 2] });
    }
    both_faces(idx, first, first + 1, first + 2, first + 3);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walls(near: Vec<[f32; 3]>, base: f32) -> Walls {
        Walls {
            room: xr::Posef::IDENTITY,
            loops: vec![vec![[0.0, 0.0], [3.0, 0.0], [3.0, 2.0], [0.0, 2.0]]],
            open: false,
            near,
            reach: 0.4,
            base,
            floor: 1.0,
        }
    }

    #[test]
    fn only_the_wall_you_near_is_built() {
        let (mut wall, mut line) = Default::default();
        build(&walls(vec![[1.5, 1.2, 1.0]], 0.0), &mut wall, &mut line);
        assert!(wall.0.is_empty());
        assert_eq!(line.0.len(), 16, "four floor lines");
        // A hand 15 cm from the first side.
        build(&walls(vec![[1.5, 1.2, 0.15]], 0.0), &mut wall, &mut line);
        let bright = |v: &Vertex| v.nrm[0];
        let max = wall.0.iter().map(bright).fold(0.0, f32::max);
        assert!(max > 0.9, "{max}");
        // Right in front of the hand: strong; a metre and a half along: nothing.
        let at = |x: f32, y: f32| wall.0.iter().find(|v| (v.pos[0] - x).abs() < 0.01 && (v.pos[1] - y).abs() < 0.06 && v.pos[2] == 0.0).map(bright).unwrap();
        assert!(at(1.5, 1.2) > 0.9);
        assert_eq!(at(0.0, 1.2), 0.0);
        // Outside: everything.
        build(&walls(vec![], 1.0), &mut wall, &mut line);
        assert!(wall.0.iter().all(|v| v.nrm[0] == 1.0));
    }

    #[test]
    fn an_open_outline_hints_its_closing_side() {
        let (mut wall, mut line) = Default::default();
        let mut w = walls(vec![], 0.3);
        w.open = true;
        build(&w, &mut wall, &mut line);
        assert_eq!(line.0.len(), 16);
        assert!((line.0[12].nrm[0] - 0.35).abs() < 1e-6);
        // No wall on the closing side yet.
        assert!(wall.0.iter().all(|v| !(v.pos[0] == 0.0 && v.pos[2] > 0.05 && v.pos[2] < 1.95)));
    }
}
