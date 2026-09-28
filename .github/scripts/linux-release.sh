#!/usr/bin/env bash
# Build Monadeck's Linux packages (.deb, .rpm, .AppImage) the way releases are
# made: on Debian 12, so they run there and on anything newer (the binaries ask
# for glibc 2.35 at most). CI runs it in a `debian:12` container; to try it
# locally:
#
#   podman run --rm -v "$PWD":/src:Z -w /src debian:12 \
#     bash -c '.github/scripts/linux-release.sh deps && .github/scripts/linux-release.sh build'
#
#   deps   system packages, Rust, Node + pnpm (as root, in the container)
#   build  tests, then the packages into dist/ with their SHA256SUMS
#
# The overlay's Translate and Share buttons are for local builds only: they
# need crates/overlay/{translate,picsur}.env, which stay out of git, so these
# packages come without them.
set -euo pipefail
cd "$(dirname "$0")/../.."

NODE_VERSION=22.20.0
PNPM_VERSION=10

deps() {
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends \
    build-essential ca-certificates curl wget file git pkg-config xz-utils xdg-utils \
    cmake python3 clang libclang-dev \
    libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev \
    libssl-dev libxdo-dev libdbus-1-dev \
    libpipewire-0.3-dev libspa-0.2-dev libasound2-dev libxkbcommon-dev libwayland-dev
  if ! command -v rustup >/dev/null && [ ! -x "$HOME/.cargo/bin/rustup" ]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  fi
  if ! node --version 2>/dev/null | grep -q "^v${NODE_VERSION%%.*}\."; then
    curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
      | tar -xJ -C /usr/local --strip-components=1
  fi
  npm install -g "pnpm@${PNPM_VERSION}"
}

build() {
  # shellcheck disable=SC1091
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
  export APPIMAGE_EXTRACT_AND_RUN=1 # no FUSE in a container

  local version
  version=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' desktop/src-tauri/tauri.conf.json)
  if [ "${GITHUB_REF_TYPE:-}" = tag ] && [ "${GITHUB_REF_NAME#v}" != "$version" ]; then
    echo "Tag ${GITHUB_REF_NAME} doesn't match the app version ${version} (tauri.conf.json)" >&2
    exit 1
  fi

  # Release profile throughout: the overlay's build below reuses it.
  cargo test --release --locked --workspace
  (
    cd desktop
    pnpm install --frozen-lockfile
    pnpm check
    pnpm tauri build
    cd src-tauri
    cargo test --release --locked
  )

  rm -rf dist
  mkdir -p dist
  local bundle=desktop/src-tauri/target/release/bundle
  cp "$bundle"/deb/*.deb "$bundle"/rpm/*.rpm "$bundle"/appimage/*.AppImage dist/
  (cd dist && sha256sum -- * >SHA256SUMS)
  ls -l dist
}

case "${1:-}" in
  deps) deps ;;
  build) build ;;
  *)
    echo "usage: $0 deps|build" >&2
    exit 2
    ;;
esac
