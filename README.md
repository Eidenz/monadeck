<div align="center">

# 🎮 Monadeck

**A SteamVR-style XR orchestrator & overlay for Monado on Linux**

Built for a **Monado** and **xrizer**-only workflow, where Envision's design didn't fit.

<img src="screenshots/dashboard.jpg" width="800" alt="Monadeck in-headset dashboard"><br>
<sub>The in-headset game-library dashboard</sub>

<table>
<tr>
<td align="center" valign="top"><img src="screenshots/settings.png" width="260" alt="In-headset settings"><br><sub>In-headset settings</sub></td>
<td align="center" valign="top"><img src="screenshots/playspace.png" width="260" alt="Playspace tools"><br><sub>Playspace tools</sub></td>
<td align="center" valign="center"><img src="screenshots/desktop.png" width="260" alt="Desktop control panel"><br><sub>Desktop control panel</sub></td>
</tr>
</table>

> **AI usage:** This project was developed with AI assistance (Anthropic's Claude), under human direction, testing, and review.

</div>

## What it does

Monadeck is two halves that share one library: an **in-headset overlay** you live in while you're in VR, and a small **desktop control panel** that looks after your Monado runtime.

### In the headset

- **🎮 Your whole library, curved around you**: Steam games and non-Steam shortcuts together on a SteamVR-style dashboard, with cover art, hero banners, and logos pulled from your Steam artwork.
- **▶️ Launch and stop without leaving VR**: start a game from the dashboard; an active-game card shows what's running, with the cover, a Stop button, and how long the current session has gone.
- **🪟 Juggle multiple VR apps**: Monado can run several VR apps at once, so the **Monado** tab lists them and lets you pick which one your headset shows, freeze an app's controllers in place, or kill it. The freeze action only shows on a runtime that supports it.
- **🥽 Flat games in VR with UEVR**: toggle *VR Mod* on a non-Steam Unreal Engine game and Monadeck launches it with [UEVR](https://github.com/praydog/UEVR) injected automatically (via [chihuahua](https://github.com/keton/chihuahua), under Proton; needs `protontricks`).
- **🔍 Find things fast**: search with an on-screen keyboard, **⭐ favourite** the ones you reach for, and sort any list by *recent · name · playtime · size*.
- **🗂️ Your own collections**: group games into named collections (*Seated*, *Standing*, whatever) and browse them as their own shelves.
- **⏱️ A handy timer**: set a countdown from the Tools tab; when it's up it rings a chime and drops an in-headset notification, even over a running game. Low-battery warnings work the same way.
- **🛋️ Comfortable placement**: move the dashboard closer or further, resize it, flatten the curve, tilt it to match where you're looking, or just grab it and put it where you want.
- **🧭 Playspace tools, OVRAS-style**: nudge your floor and whole play area on any axis (height, forward/back, left/right) and rotate it, in fixed steps, with recenter and reset. Your offsets persist and re-apply when the runtime restarts.
- **🔋 At-a-glance status**: a bottom bar with the time and live controller/tracker batteries.

### On the desktop

- **📦 Get a runtime in one click**: install a prebuilt build of the Monado fork and the latest xrizer straight from GitHub (no compiling), or point Monadeck at your own.
- **🚀 Run your Monado service**: start and stop it with your own environment variables.
- **🎛️ Switch runtimes safely**: register **xrizer** as the OpenVR runtime and point OpenXR at Monado, backing up and restoring your existing config so it never eats a working SteamVR setup.
- **⚡ One-click `CAP_SYS_NICE`**: the permission Monado wants after a rebuild, applied with a single authorised click.
- **🎮 Manage your games**: per-game launch options and **xrizer controller-binding overrides**.
- **🧩 Plugins by explicit path**: no `$PATH`, no `.desktop` files.
- **🔌 Live device list**: HMD, controllers, trackers, and battery via libmonado.

It deliberately **doesn't build Monado from source** or manage drivers, though it *can* fetch a prebuilt build of the fork and the latest xrizer for you (see [Requirements](#requirements)). For source builds, driver profiles, and full dependency management, that's what [Envision](https://gitlab.com/gabmus/envision) is for.

## How it works

Two parts, one configuration:

- **Desktop app**: where you set up the runtime, manage your games, artwork, bindings, and launch options. It sits in a normal window with custom chrome.
- **In-headset overlay**: an OpenXR dashboard the desktop app launches for you when VR starts. The overlay is bundled inside the desktop app, so there's nothing extra to install.

## Requirements

- **Linux.** That's the only hard requirement to get started.
- A **Monado**-based OpenXR runtime and **xrizer** for OpenVR/SteamVR games, but not up front: Monadeck can **install a prebuilt build of the fork and the latest xrizer for you** (Settings → General → *Install built-in*), or use your own existing build / fork.
- **Steam** (with **Proton** for Windows games), which Monadeck reads for your library and Steam's cover art.
- A headset exposing info through **libmonado** for the live device and battery strip (optional).

## Install

### Build from source

```bash
cd desktop
pnpm install
pnpm tauri build
```

This produces a `.rpm`, `.deb`, and `.AppImage` in `desktop/src-tauri/target/release/bundle/`, install the one for your distro. On Fedora/Nobara the rpm and AppImage work out of the box; the `.deb` needs `dpkg`. The in-headset launcher is bundled inside the package, so there's nothing else to set up.

## Using it

1. Launch Monadeck and open **Settings → General**. Either click **Install built-in Monado** / **Install built-in xrizer** to download a prebuilt fork build and the latest xrizer, or set your own **Monado build prefix** (e.g. `~/monado/build/install`) and **xrizer runtime path**. Monadeck also tries to autodetect both from `$PATH` and your current active runtime.
2. **Start the runtime**, then register xrizer/OpenXR (your existing config is backed up automatically). If a rebuild left Monado without `CAP_SYS_NICE`, accept the prompt to apply it.
3. Put the headset on, the dashboard opens. **Press the system button on your left controller** to summon or dismiss it; **point + trigger** to select, **grip** to grab and move it.
4. Browse, search, favourite, and drop games into collections. The **Tools**, **Playspace**, and **Settings** tabs live at the bottom of the left rail; tune the dashboard under **Settings → Placement** and your floor / play area under **Playspace**.

**Artwork tip:** Monadeck reads **JPEG/PNG** cover and hero art from Steam's grid folder and library cache. If a cover came down as **AVIF** (some SteamGridDB downloads are, even with a `.png` name) it won't decode, so re-save it as PNG/JPEG, then hit **Settings → Refresh library** to re-scan without restarting.

## Development

```bash
# Desktop app (the control panel + UI)
cd desktop && pnpm install && pnpm tauri dev

# Overlay on its own (normally the desktop app launches it)
cargo run -p monadeck-overlay
```

The project is a Rust workspace: `crates/core` (runtime orchestration, library and artwork scanning, shared config) and `crates/overlay` (the OpenXR dashboard), alongside `desktop/`, a **Tauri 2 + SvelteKit (Svelte 5)** app with custom window chrome. `desktop` is excluded from the root workspace so its webkit dependency tree stays out of the core build. Config and overlay preferences live under `~/.config/monadeck/`.

Overlay environment variable: `MONADECK_OVERLAY_FLAT` forces a flat quad panel instead of the curved cylinder (for runtimes without cylinder-layer support, or comparison).

### libmonado

Uses the `wayvr-org/libmonado-rs` pin (dlopen-based). It `dlopen`s whatever `libmonado.so` the active runtime points at (your fork's), so an older client against a newer library stays on the safe side.

## License

MIT.
