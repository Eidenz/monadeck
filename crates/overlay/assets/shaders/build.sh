#!/bin/sh
# Rebuild the 3D layer's SPIR-V after editing a shader (the .spv files are
# committed, so building Monadeck doesn't need a shader compiler).
set -e
cd "$(dirname "$0")"
for s in scene.vert scene.frag; do
    glslc -O "$s" -o "$s.spv"
done
