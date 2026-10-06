#version 450
// The 3D layer's one vertex shader: models, glove hands and the floor grid.
// See src/scene/gpu.rs for the push constants' layout.

layout(location = 0) in vec3 in_pos;
layout(location = 1) in vec3 in_nrm;
layout(location = 2) in vec2 in_uv;

layout(push_constant) uniform Push {
    mat4 mvp;
    // Rows of the model-to-eye rotation; w carries the tint (r, g, b).
    vec4 n0;
    vec4 n1;
    vec4 n2;
    // x: opacity, y: mode (0 textured, 1 tint, 2 grid), zw: grid centre.
    vec4 params;
} pc;

layout(location = 0) out vec3 v_nrm;
layout(location = 1) out vec2 v_uv;

void main() {
    gl_Position = pc.mvp * vec4(in_pos, 1.0);
    v_nrm = vec3(dot(pc.n0.xyz, in_nrm), dot(pc.n1.xyz, in_nrm), dot(pc.n2.xyz, in_nrm));
    v_uv = in_uv;
}
