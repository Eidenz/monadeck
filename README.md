# Monadeck

A compact, SteamVR-style launcher/orchestrator for a custom **Monado** build on
Linux. Built because Envision's design and PATH-bound plugin system don't fit a
fork-heavy, xrizer-only workflow.

What it does — and deliberately *doesn't*:

- ✅ Start/stop `monado-service` with your custom **environment variables**
- ✅ Register **xrizer** as the OpenVR runtime, and point OpenXR at monado —
  **backing up and restoring** your existing config so it never eats a SteamVR setup
- ✅ One-click **`CAP_SYS_NICE=eip`** via `pkexec` (the toast that pops after a rebuild)
- ✅ Live **device strip** (HMD / controllers / trackers + battery) via libmonado
- ✅ **Plugins launched by explicit path** — no `$PATH`, no `.desktop` files
- ❌ No building from source, no driver profiles, no dependency checking
  (that's what Envision is for)

## Stack

Same stack as `udcap-control` / `nemurixr`: **Tauri 2 + SvelteKit (Svelte 5) +
TypeScript + Vite + pnpm**, custom window chrome.

```
monadeck/
├── crates/core/          # monadeck-core — framework-agnostic orchestration (ported from Envision)
│   └── src/
│       ├── cmd_runner.rs     # spawn monado-service, stream logs
│       ├── active_runtime.rs # ~/.config/openxr/1/active_runtime.json (+ backup/restore)
│       ├── openvr_paths.rs    # ~/.config/openvr/openvrpaths.vrpath (+ backup/restore)
│       ├── setcap.rs          # getcap verify / pkexec setcap
│       ├── devices.rs         # libmonado auto_connect → device strip
│       ├── plugins.rs         # path-based plugin launch
│       └── config.rs          # ~/.config/monadeck/config.json
└── desktop/              # Tauri app (its own cargo workspace)
    ├── src/              # SvelteKit frontend
    └── src-tauri/        # Tauri commands wrapping the core
```

The `crates/core` ↔ `desktop/src-tauri` split (with `desktop` excluded from the
root workspace) keeps the heavy webkit dependency tree out of the core build.

## libmonado

Uses the same pin as NemuriXR: `wayvr-org/libmonado-rs` (dlopen-based, API 1.6).
It `dlopen`s whatever `libmonado.so` the active runtime points at — i.e. your
fork's 1.7.0 `.so`. Older client against a newer lib is the safe direction.

## Develop

```bash
cd desktop
pnpm install
pnpm tauri dev      # run the app
pnpm check          # type-check
cargo test -p monadeck-core --manifest-path ../Cargo.toml   # core unit tests
```

First run: open **Settings**, set your **Monado build prefix** (e.g.
`~/monado/build/install`) and your **xrizer runtime path**. Monadeck tries to
autodetect the prefix from `$PATH` / the current active runtime.
