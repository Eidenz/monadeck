//! Column-major 4×4 matrices for the 3D layer (`m[col * 4 + row]`), built
//! from OpenXR poses.
use crate::mathx::{q_mul, quat_rotate};
use openxr as xr;

pub type Mat4 = [f32; 16];

pub const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}

/// Rotation `q` (x, y, z, w), then translation `p`, scaled by `s` first.
pub fn from_trs(p: [f32; 3], q: [f32; 4], s: [f32; 3]) -> Mat4 {
    let x = quat_rotate(q, [s[0], 0.0, 0.0]);
    let y = quat_rotate(q, [0.0, s[1], 0.0]);
    let z = quat_rotate(q, [0.0, 0.0, s[2]]);
    [x[0], x[1], x[2], 0.0, y[0], y[1], y[2], 0.0, z[0], z[1], z[2], 0.0, p[0], p[1], p[2], 1.0]
}

pub fn from_pose(p: &xr::Posef) -> Mat4 {
    from_trs(pos(p), quat(p), [1.0; 3])
}

/// The view matrix of an eye at `p`: the inverse of its pose.
pub fn view(p: &xr::Posef) -> Mat4 {
    let inv = conj(quat(p));
    let t = quat_rotate(inv, pos(p));
    from_trs([-t[0], -t[1], -t[2]], inv, [1.0; 3])
}

/// Vulkan clip space (y down, depth 0..1) from an OpenXR field of view, as
/// OpenXR's `xr_linear.h` builds it for Vulkan.
pub fn projection(fov: &xr::Fovf, near: f32, far: f32) -> Mat4 {
    let (l, r) = (fov.angle_left.tan(), fov.angle_right.tan());
    let (u, d) = (fov.angle_up.tan(), fov.angle_down.tan());
    let w = r - l;
    let h = d - u;
    let mut m = [0.0; 16];
    m[0] = 2.0 / w;
    m[8] = (r + l) / w;
    m[5] = 2.0 / h;
    m[9] = (u + d) / h;
    m[10] = -far / (far - near);
    m[14] = -(far * near) / (far - near);
    m[11] = -1.0;
    m
}

/// The upper 3×3 of `m` as rows, for turning normals into eye space (rigid
/// transforms only: no non-uniform scale on models).
pub fn rotation_rows(m: &Mat4) -> [[f32; 3]; 3] {
    let n = |c: usize| {
        let v = [m[c * 4], m[c * 4 + 1], m[c * 4 + 2]];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let (x, y, z) = (n(0), n(1), n(2));
    [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]]
}

pub fn pos(p: &xr::Posef) -> [f32; 3] {
    [p.position.x, p.position.y, p.position.z]
}

pub fn quat(p: &xr::Posef) -> [f32; 4] {
    [p.orientation.x, p.orientation.y, p.orientation.z, p.orientation.w]
}

pub fn conj(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[1], -q[2], q[3]]
}

pub fn pose(p: [f32; 3], q: [f32; 4]) -> xr::Posef {
    xr::Posef {
        orientation: xr::Quaternionf { x: q[0], y: q[1], z: q[2], w: q[3] },
        position: xr::Vector3f { x: p[0], y: p[1], z: p[2] },
    }
}

/// SteamVR's `rotate_xyz` (degrees): Rz · Ry · Rx, as Monado reads it.
pub fn euler_xyz_deg(r: [f32; 3]) -> [f32; 4] {
    let axis = |a: [f32; 3], deg: f32| {
        let h = deg.to_radians() / 2.0;
        [a[0] * h.sin(), a[1] * h.sin(), a[2] * h.sin(), h.cos()]
    };
    q_mul(axis([0.0, 0.0, 1.0], r[2]), q_mul(axis([0.0, 1.0, 0.0], r[1]), axis([1.0, 0.0, 0.0], r[0])))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(m: &Mat4, v: [f32; 3]) -> [f32; 4] {
        let mut o = [0.0; 4];
        for (r, out) in o.iter_mut().enumerate() {
            *out = m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2] + m[12 + r];
        }
        o
    }

    #[test]
    fn view_undoes_the_pose() {
        let q = euler_xyz_deg([10.0, 40.0, -5.0]);
        let p = pose([0.3, 1.6, -0.2], q);
        let m = mul(&view(&p), &from_pose(&p));
        for (a, b) in m.iter().zip(IDENTITY) {
            assert!((a - b).abs() < 1e-5, "{m:?}");
        }
    }

    #[test]
    fn projection_maps_the_fov_edges() {
        let fov = xr::Fovf { angle_left: -0.8, angle_right: 0.7, angle_up: 0.75, angle_down: -0.85 };
        let m = projection(&fov, 0.05, 100.0);
        let ndc = |v: [f32; 3]| {
            let c = apply(&m, v);
            [c[0] / c[3], c[1] / c[3], c[2] / c[3]]
        };
        // The right edge at 1 m goes to x = 1, the top edge to y = -1 (Vulkan's y is down).
        assert!((ndc([0.7f32.tan(), 0.0, -1.0])[0] - 1.0).abs() < 1e-4);
        assert!((ndc([0.0, 0.75f32.tan(), -1.0])[1] + 1.0).abs() < 1e-4);
        // Depth 0 at the near plane, 1 at the far one.
        assert!(ndc([0.0, 0.0, -0.05])[2].abs() < 1e-4);
        assert!((ndc([0.0, 0.0, -100.0])[2] - 1.0).abs() < 1e-4);
    }
}
