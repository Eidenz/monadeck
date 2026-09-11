<div align="center">

# 🎮 Monadeck

**A SteamVR-style launcher, dashboard and desktop overlay for Monado and WiVRn on Linux**

Built for a Monado + xrizer workflow.

<table>
<tr>
<td align="center" valign="top"><img src="screenshots/dashboard.jpg" width="260" alt="In-headset dashboard"><br><sub>Game library dashboard</sub></td>
<td align="center" valign="top"><img src="screenshots/screens.jpg" width="260" alt="Desktop screens and keyboard in VR"><br><sub>Desktop screens & keyboard</sub></td>
<td align="center" valign="top"><img src="screenshots/watch.jpg" width="260" alt="Wrist watch"><br><sub>Wrist watch</sub></td>
</tr>
<tr>
<td align="center" valign="top"><img src="screenshots/photos.jpg" width="260" alt="Screenshots in the headset"><br><sub>Screenshots & photos</sub></td>
<td align="center" valign="top"><img src="screenshots/playspace.jpg" width="260" alt="Playspace tools"><br><sub>Playspace tools</sub></td>
<td align="center" valign="top"><img src="screenshots/desktop.jpg" width="260" alt="Desktop control panel"><br><sub>Desktop control panel</sub></td>
</tr>
</table>

> **AI usage:** This project was developed with AI assistance (Anthropic's Claude), under human direction, testing, and review.

</div>

## What it does

Monadeck is two halves that share one configuration: an in-headset overlay you live in while you're in VR, and a small desktop control panel that looks after your Monado runtime.

- **Launch games from a curved in-headset dashboard.** Steam and non-Steam titles with their artwork, favourites, collections, playtime and search; flat Unreal Engine games go through [UEVR](https://github.com/praydog/UEVR) automatically.
- **Bring your desktop into VR.** Monitors become movable, resizable, curved screens driven by a real mouse, with a full VR keyboard, docking, saved layouts and a WayVR-style playspace drag. Captured straight into the GPU through PipeWire.
- **A wrist watch that keeps you in the loop.** Clock and time zones, battery levels, desktop and XSOverlay-protocol notifications, media controls, new screenshots to review, translate or share, and four quick buttons of your choice. Fold it down to a clock-only pill at the wrist when you want less on your arm. Wear it on either wrist; each wrist, with controllers or with gloves, remembers its own spot.
- **Let games talk to the overlay.** An OSC listener (off by default; Settings › OSC control) lets VRChat avatar parameters, VRCOSC and friends toggle the watch, the minimal watch, the dashboard, screens and keyboard, or raise a toast: `/monadeck/watch`, `/monadeck/watch/mini`, `/monadeck/dashboard`, `/monadeck/screens`, `/monadeck/keyboard`, `/monadeck/notify "title" "body"`, or avatar parameters named `MonadeckWatch`, `MonadeckWatchMini`, `MonadeckDashboard`, `MonadeckScreens`, `MonadeckKeyboard`.
- **Control the runtime without taking the headset off.** Nudge and rotate your play area, pick which app the headset shows, freeze an app's controllers, set a timer.
- **Set up Monado in one click.** The desktop app installs a prebuilt build of the Monado fork and xrizer, runs the service with your environment, switches runtimes without breaking an existing SteamVR setup, and manages launch options and binding overrides.
- **Or stream to a standalone headset with WiVRn.** Pick the WiVRn backend in Settings → General and Monadeck runs `wivrn-server` instead: pair headsets by PIN, see the stream status, pick the encoder, and the same overlay and plugins launch with each headset session. Needs [WiVRn](https://github.com/WiVRn/WiVRn) installed from your distro.

It deliberately doesn't build Monado from source or manage drivers. For that, [Envision](https://gitlab.com/gabmus/envision) is the right tool; Monadeck can sit next to it.

## Requirements

- **Linux** with a Wayland desktop (the screen mirror uses the desktop portal and PipeWire; KDE is what it's tested on).
- A **Monado**-based OpenXR runtime and **xrizer** for OpenVR games, though not up front: Monadeck can install a prebuilt build of the fork and the latest xrizer for you (Settings → General → *Install built-in*), or use your own. For a standalone headset, install **WiVRn** (26.6 or newer) and choose it as the runtime instead.
- **Steam** (with Proton for Windows games) for your library and cover art.
- Some features need the [Monado fork](https://github.com/Eidenz/Monado): controller freeze, in-headset screenshots, device hotplug. They hide themselves on stock Monado.

## Install

Grab the `.rpm`, `.deb` or `.AppImage` from the [releases page](https://github.com/Eidenz/monadeck/releases), or build it yourself:

```bash
cd desktop
pnpm install
pnpm tauri build
```

The bundles land in `desktop/src-tauri/target/release/bundle/`. The in-headset overlay is inside the package, so there's nothing else to set up.

## Using it

1. Open **Settings → General**. Install the built-in Monado and xrizer, or point Monadeck at your own build prefix and xrizer path (it also tries to autodetect both).
2. **Start the runtime**, then register xrizer/OpenXR. Your existing config is backed up automatically. If Monado lacks `CAP_SYS_NICE`, accept the prompt to apply it.
3. Put the headset on. **Left system button** summons or dismisses the dashboard, **point + trigger** selects, **grip** grabs and moves things.
4. To mirror your desktop, show a screen from the watch or the bottom bar and approve the share dialog once on the desktop; the approval is remembered. The full gesture list lives under **Settings → Controllers → Help** in the headset.

**Artwork tip:** covers that came down as AVIF (some SteamGridDB downloads are, even with a `.png` name) won't decode. Re-save them as PNG/JPEG and hit **Settings → Refresh library**.

## Development

```bash
# Desktop app (the control panel + UI)
cd desktop && pnpm install && pnpm tauri dev

# Overlay on its own (normally the desktop app launches it)
cargo run -p monadeck-overlay
```

A Rust workspace: `crates/core` (runtime orchestration, library and artwork scanning, shared config) and `crates/overlay` (the OpenXR overlay), plus `desktop/`, a Tauri 2 + SvelteKit (Svelte 5) app kept out of the root workspace so its webkit dependencies stay out of the core build. Config lives under `~/.config/monadeck/`.

Optional: `crates/overlay/translate.env` and `picsur.env` (see the `.example` files) bake in a translation endpoint and a Picsur share target for screenshots. The overlay has self-tests for the capture path, keyboard, notifications and text-field focus (`--desktop-selftest`, `--keyboard-selftest`, `--notify-selftest`, `--a11y-selftest`), `--toast-preview [dir]` renders every notification card to PNGs without a headset, and `MONADECK_OVERLAY_FLAT` forces flat panels on runtimes without cylinder layers.

libmonado is loaded through the `wayvr-org/libmonado-rs` pin, which `dlopen`s whatever `libmonado.so` the active runtime points at.

## Credits

[**Monado**](https://gitlab.freedesktop.org/monado/monado) (OpenXR runtime) and [**xrizer**](https://github.com/Supreeeme/xrizer) by Supreeeme (OpenVR on OpenXR); [**WiVRn**](https://github.com/WiVRn/WiVRn) by Guillaume Meunier and Patrick Nicolas (standalone-headset streaming, driven through its D-Bus interface); [**WayVR**](https://github.com/wlx-team/wayvr) by galister and the wlx team, whose desktop overlay, watch and playspace drag this borrows from, plus the [`libmonado-rs`](https://github.com/wayvr-org/libmonado-rs) pin; [**UEVR**](https://github.com/praydog/UEVR) by praydog and the [**chihuahua**](https://github.com/keton/chihuahua) injector by keton, run through [**protontricks**](https://github.com/Matoking/protontricks); [**go-bsb-cams**](https://github.com/Red-M/go-bsb-cams) by Red-M (Beyond eye cameras); [**Envision**](https://gitlab.com/gabmus/envision) for the ground it covered first. Default background: ["Table Mountain 2"](https://polyhaven.com/a/table_mountain_2) by Greg Zaal, Poly Haven (CC0). Icons: [Phosphor](https://phosphoricons.com).

## License

MIT.
