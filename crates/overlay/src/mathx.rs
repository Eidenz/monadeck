// Small pose / quaternion / raycast helpers (openxr Posef-based).
// Ported from monado-frame — the proven in-headset overlay foundation.
use openxr as xr;

pub fn quat_rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let tx = 2.0 * (y * v[2] - z * v[1]);
    let ty = 2.0 * (z * v[0] - x * v[2]);
    let tz = 2.0 * (x * v[1] - y * v[0]);
    [
        v[0] + w * tx + (y * tz - z * ty),
        v[1] + w * ty + (z * tx - x * tz),
        v[2] + w * tz + (x * ty - y * tx),
    ]
}

pub fn q_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let (ax, ay, az, aw) = (a[0], a[1], a[2], a[3]);
    let (bx, by, bz, bw) = (b[0], b[1], b[2], b[3]);
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

pub fn qf(q: &xr::Quaternionf) -> [f32; 4] {
    [q.x, q.y, q.z, q.w]
}

pub fn vec3f(v: [f32; 3]) -> xr::Vector3f {
    xr::Vector3f { x: v[0], y: v[1], z: v[2] }
}

pub fn quatf(q: [f32; 4]) -> xr::Quaternionf {
    xr::Quaternionf { x: q[0], y: q[1], z: q[2], w: q[3] }
}

pub fn pose_compose(a: &xr::Posef, b: &xr::Posef) -> xr::Posef {
    let q = q_mul(qf(&a.orientation), qf(&b.orientation));
    let rp = quat_rotate(qf(&a.orientation), [b.position.x, b.position.y, b.position.z]);
    xr::Posef {
        orientation: quatf(q),
        position: vec3f([a.position.x + rp[0], a.position.y + rp[1], a.position.z + rp[2]]),
    }
}

pub fn pose_invert(a: &xr::Posef) -> xr::Posef {
    let iq = [-a.orientation.x, -a.orientation.y, -a.orientation.z, a.orientation.w];
    let ip = quat_rotate(iq, [a.position.x, a.position.y, a.position.z]);
    xr::Posef { orientation: quatf(iq), position: vec3f([-ip[0], -ip[1], -ip[2]]) }
}

pub fn locate_pose(aim: &xr::Space, base: &xr::Space, time: xr::Time) -> Option<xr::Posef> {
    let loc = aim.locate(base, time).ok()?;
    let need = xr::SpaceLocationFlags::POSITION_VALID | xr::SpaceLocationFlags::ORIENTATION_VALID;
    if loc.location_flags.contains(need) {
        Some(loc.pose)
    } else {
        None
    }
}

/// Raycast a controller aim pose onto a quad; returns (u, v, distance) on hit,
/// with (u, v) in [0,1] and (0.5, 0.5) at the quad centre.
pub fn raycast(pose: &xr::Posef, quad: &xr::Posef, size_m: (f32, f32)) -> Option<(f32, f32, f32)> {
    let o = [pose.position.x, pose.position.y, pose.position.z];
    let q = qf(&pose.orientation);
    let qq = qf(&quad.orientation);
    let dir = quat_rotate(q, [0.0, 0.0, -1.0]);
    let normal = quat_rotate(qq, [0.0, 0.0, 1.0]);
    let axis_x = quat_rotate(qq, [1.0, 0.0, 0.0]);
    let axis_y = quat_rotate(qq, [0.0, 1.0, 0.0]);
    let c = [quad.position.x, quad.position.y, quad.position.z];

    let denom = dir[0] * normal[0] + dir[1] * normal[1] + dir[2] * normal[2];
    if denom.abs() < 1e-6 {
        return None;
    }
    let co = [c[0] - o[0], c[1] - o[1], c[2] - o[2]];
    let t = (co[0] * normal[0] + co[1] * normal[1] + co[2] * normal[2]) / denom;
    if t <= 0.0 {
        return None;
    }
    let p = [o[0] + dir[0] * t, o[1] + dir[1] * t, o[2] + dir[2] * t];
    let off = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
    let lx = off[0] * axis_x[0] + off[1] * axis_x[1] + off[2] * axis_x[2];
    let ly = off[0] * axis_y[0] + off[1] * axis_y[1] + off[2] * axis_y[2];
    if lx.abs() > size_m.0 * 0.5 || ly.abs() > size_m.1 * 0.5 {
        return None;
    }
    Some((lx / size_m.0 + 0.5, 0.5 - ly / size_m.1, t))
}

/// Raycast a controller aim pose onto a cylinder layer's inner surface. `cyl`
/// is the cylinder pose (axis = +Y, arc centred on -Z), `radius` the cylinder
/// radius, `central_angle` the arc width (rad), `height` the panel height (m).
/// Returns (u, v, distance) with (0.5, 0.5) at the centre — same convention as
/// [`raycast`], so the laser/pointer code is identical for flat or curved.
pub fn raycast_cylinder(
    aim: &xr::Posef,
    cyl: &xr::Posef,
    radius: f32,
    central_angle: f32,
    height: f32,
) -> Option<(f32, f32, f32)> {
    let q = qf(&cyl.orientation);
    let inv = [-q[0], -q[1], -q[2], q[3]];
    let o = [
        aim.position.x - cyl.position.x,
        aim.position.y - cyl.position.y,
        aim.position.z - cyl.position.z,
    ];
    let lo = quat_rotate(inv, o); // ray origin in cylinder-local space
    let ld = quat_rotate(inv, forward(aim)); // ray dir in cylinder-local space

    // Intersect with the infinite cylinder x^2 + z^2 = radius^2. The viewer sits
    // inside it, so we take the positive (exit) root.
    let a = ld[0] * ld[0] + ld[2] * ld[2];
    if a < 1e-6 {
        return None;
    }
    let b = 2.0 * (lo[0] * ld[0] + lo[2] * ld[2]);
    let c = lo[0] * lo[0] + lo[2] * lo[2] - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b + disc.sqrt()) / (2.0 * a);
    if t <= 0.0 {
        return None;
    }
    let hx = lo[0] + ld[0] * t;
    let hy = lo[1] + ld[1] * t;
    let hz = lo[2] + ld[2] * t;
    let angle = hx.atan2(-hz); // 0 at the -Z arc centre
    if angle.abs() > central_angle * 0.5 || hy.abs() > height * 0.5 {
        return None;
    }
    Some((0.5 + angle / central_angle, 0.5 - hy / height, t))
}

/// Forward direction (-Z) of a pose, normalised.
pub fn forward(pose: &xr::Posef) -> [f32; 3] {
    quat_rotate(qf(&pose.orientation), [0.0, 0.0, -1.0])
}

pub fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-6 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Quaternion from orthonormal basis columns (rotation mapping ex->x, ey->y, ez->z).
pub fn quat_from_axes(x: [f32; 3], y: [f32; 3], z: [f32; 3]) -> [f32; 4] {
    let (m00, m10, m20) = (x[0], x[1], x[2]);
    let (m01, m11, m21) = (y[0], y[1], y[2]);
    let (m02, m12, m22) = (z[0], z[1], z[2]);
    let tr = m00 + m11 + m22;
    if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s]
    }
}

/// A pose `dist` metres ahead of the head, facing the user. `tilt` uses the
/// head's own up (so the panel pitches to match your gaze) instead of world-up
/// (always vertical).
pub fn front_pose(h: &xr::Posef, dist: f32, lateral: f32, height: f32, tilt: bool) -> xr::Posef {
    let fwd = normalize(forward(h));
    let up = if tilt {
        normalize(quat_rotate(qf(&h.orientation), [0.0, 1.0, 0.0]))
    } else {
        [0.0, 1.0, 0.0]
    };
    let right = normalize(cross(fwd, up));
    let o = [h.position.x, h.position.y, h.position.z];
    let pos = [
        o[0] + fwd[0] * dist + right[0] * lateral,
        o[1] + fwd[1] * dist + right[1] * lateral + height,
        o[2] + fwd[2] * dist + right[2] * lateral,
    ];
    let z = normalize([o[0] - pos[0], o[1] - pos[1], o[2] - pos[2]]); // face the head
    let x = normalize(cross(up, z));
    let y = cross(z, x);
    xr::Posef { orientation: quatf(quat_from_axes(x, y, z)), position: vec3f(pos) }
}

/// Quaternion for a rotation of `angle` radians about `axis`.
pub fn quat_from_axis_angle(axis: [f32; 3], angle: f32) -> [f32; 4] {
    let a = normalize(axis);
    let s = (angle * 0.5).sin();
    [a[0] * s, a[1] * s, a[2] * s, (angle * 0.5).cos()]
}

/// A notification pose: `dist` ahead of the head and `drop` below the gaze (in
/// head-local space, so it sits in the lower view at any head pitch), facing the
/// head and tilted to match it.
pub fn toast_pose(h: &xr::Posef, dist: f32, drop: f32) -> xr::Posef {
    let q = qf(&h.orientation);
    let fwd = normalize(quat_rotate(q, [0.0, 0.0, -1.0]));
    let up = normalize(quat_rotate(q, [0.0, 1.0, 0.0]));
    let o = [h.position.x, h.position.y, h.position.z];
    let pos = [
        o[0] + fwd[0] * dist - up[0] * drop,
        o[1] + fwd[1] * dist - up[1] * drop,
        o[2] + fwd[2] * dist - up[2] * drop,
    ];
    let z = normalize([o[0] - pos[0], o[1] - pos[1], o[2] - pos[2]]); // face head
    let x = normalize(cross(up, z));
    let y = cross(z, x);
    xr::Posef { orientation: quatf(quat_from_axes(x, y, z)), position: vec3f(pos) }
}

/// A pose offset from `anchor` in the anchor's own frame (+X right, +Y up, +Z
/// toward the viewer), keeping the anchor's orientation. Places the floating
/// rail/bottom panels relative to the main panel.
pub fn offset_pose(anchor: &xr::Posef, dx: f32, dy: f32, dz: f32) -> xr::Posef {
    let q = qf(&anchor.orientation);
    let r = quat_rotate(q, [1.0, 0.0, 0.0]);
    let u = quat_rotate(q, [0.0, 1.0, 0.0]);
    let f = quat_rotate(q, [0.0, 0.0, 1.0]);
    xr::Posef {
        orientation: anchor.orientation,
        position: vec3f([
            anchor.position.x + r[0] * dx + u[0] * dy + f[0] * dz,
            anchor.position.y + r[1] * dx + u[1] * dy + f[1] * dz,
            anchor.position.z + r[2] * dx + u[2] * dy + f[2] * dz,
        ]),
    }
}

/// A pose `dist` metres ahead of the head at a fixed identity-LOCAL orientation
/// (used as the launcher's default anchored placement).
pub fn posef(p: [f32; 3]) -> xr::Posef {
    xr::Posef {
        orientation: xr::Quaternionf { x: 0.0, y: 0.0, z: 0.0, w: 1.0 },
        position: xr::Vector3f { x: p[0], y: p[1], z: p[2] },
    }
}
