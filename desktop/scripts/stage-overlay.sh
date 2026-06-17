#!/usr/bin/env bash
# Build the in-headset overlay and stage it as a Tauri sidecar so
# `tauri build`/`tauri dev` bundle it alongside the desktop app (same pattern
# NemuriXR uses). The overlay lives in the ROOT workspace (../Cargo.toml).
set -euo pipefail
cd "$(dirname "$0")/.."

profile="${1:-release}"
triple="$(rustc -vV | sed -n 's/^host: //p')"

flag=""
outdir="debug"
if [ "$profile" = "release" ]; then
  flag="--release"
  outdir="release"
fi

echo "Building monadeck-overlay ($profile) for ${triple}…"
cargo build $flag -p monadeck-overlay --manifest-path ../Cargo.toml

mkdir -p src-tauri/binaries
cp "../target/${outdir}/monadeck-overlay" "src-tauri/binaries/monadeck-overlay-${triple}"
echo "Staged src-tauri/binaries/monadeck-overlay-${triple} ($profile)"
