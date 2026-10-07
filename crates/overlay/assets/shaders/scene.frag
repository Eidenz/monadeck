#version 450
// Models lit from the eye with a rim in the tint (SteamVR's look over a
// game), flat tinted parts for glove hands, an anti-aliased floor grid
// that fades out around its centre, and the boundary: a lattice on its walls
// and lines on the floor, as strong as their vertices say. Output is
// premultiplied alpha.

layout(location = 0) in vec3 v_nrm;
layout(location = 1) in vec2 v_uv;

layout(set = 0, binding = 0) uniform sampler2D tex;

layout(push_constant) uniform Push {
    mat4 mvp;
    vec4 n0;
    vec4 n1;
    vec4 n2;
    vec4 params;
} pc;

layout(location = 0) out vec4 out_color;

float grid_line(vec2 uv, float width) {
    vec2 d = abs(fract(uv - 0.5) - 0.5) / max(fwidth(uv), vec2(1e-5));
    return 1.0 - clamp(min(d.x, d.y) - width, 0.0, 1.0);
}

// Lines `thick` metres wide every `cell` metres, never thinner than a pixel.
float lattice(vec2 uv, float cell, float thick) {
    vec2 fw = max(fwidth(uv), vec2(1e-5));
    vec2 d = abs(fract(uv / cell - 0.5) - 0.5) * cell;
    vec2 cov = 1.0 - clamp(d / fw - max(vec2(0.5), 0.5 * thick / fw), 0.0, 1.0);
    // Too fine to draw far away: fade to the fill instead of shimmering.
    vec2 fine = 1.0 - smoothstep(0.25, 0.6, fw / cell);
    return max(cov.x * fine.x, cov.y * fine.y);
}

void main() {
    float alpha = pc.params.x;
    int mode = int(pc.params.y + 0.5);
    vec3 tint = vec3(pc.n0.w, pc.n1.w, pc.n2.w);

    if (mode == 2) {
        // v_uv: metres on the floor. Lines every metre; faint quarters
        // only close by, where they help judge distance.
        float dist = length(v_uv - pc.params.zw);
        float major = grid_line(v_uv, 0.5);
        float minor = grid_line(v_uv * 4.0, 0.0) * 0.25 * (1.0 - smoothstep(1.0, 3.0, dist));
        float fade = 1.0 - smoothstep(2.0, 7.0, dist);
        float a = max(major, minor) * fade * alpha;
        out_color = vec4(tint * a, a);
        return;
    }

    if (mode == 3) {
        // v_uv: metres along the walls and up them. Thins out towards the top.
        float k = clamp(v_nrm.x, 0.0, 1.0);
        float lines = lattice(v_uv, 0.25, 0.012);
        float top = 1.0 - smoothstep(pc.params.z - 0.7, pc.params.z, v_uv.y);
        float a = k * alpha * mix(0.10, 0.95, lines) * top;
        out_color = vec4(tint * a, a);
        return;
    }
    if (mode == 4) {
        float a = clamp(v_nrm.x, 0.0, 1.0) * alpha;
        out_color = vec4(tint * a, a);
        return;
    }

    vec3 n = normalize(v_nrm);
    // Eye space looks down -Z: a surface facing the eye has n.z near 1.
    float facing = clamp(n.z, 0.0, 1.0);
    vec3 base = mode == 0 ? texture(tex, v_uv).rgb : tint;
    // Textures are dark (an Index is near black): lift them so the shape reads.
    vec3 lit = base * (0.6 + 0.8 * facing) + 0.03;
    float rim = pow(1.0 - facing, 3.0);
    vec3 col = mix(lit, tint, rim * (mode == 0 ? 0.45 : 0.25));
    float a = alpha * mix(0.9, 1.0, rim);
    out_color = vec4(col * a, a);
}
