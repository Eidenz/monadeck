# Desktop viewer roadmap

The WayVR-style screen mirror inside the monadeck overlay (branch
`feature/desktop-overlay`). Edit this file freely: reorder, strike, add. Items
marked **done** are on the branch; everything else is a proposal.

## Done

- Portal → PipeWire → DMA-BUF → quad layer capture path (GPU zero-copy, SHM
  fallback), restore token persisted so the share dialog appears once.
- Laser → absolute uinput mouse: hover moves, trigger left-clicks, A
  right-clicks, thumbstick scrolls, grip moves a screen. Screens persist while
  the dashboard is dismissed.
- Laser fades out 2 s after entering a screen.
- Approved screens live on the bottom bar (centered), with a user-defined order
  (Desktop page ▲/▼). Desktop page is settings only.
- Full VR keyboard: F-row, ISO main block, nav cluster, numpad, Copy/Cut/Paste.
  Labels from the real xkb layout (all configured layouts). One-shot modifier
  latches (tap a latched modifier again within 1.5 s to send it alone, e.g.
  Super for the app launcher), Caps toggle. Floats freely; docks under a screen
  when released within 18 cm of its dock and then follows that screen. Top bar:
  layout switcher (drives KDE via org.kde.keyboard), clipboard preview
  (wl-paste), latched modifiers, dock/undock, close.
- Bottom bar shows screens as numbers in a fixed order; keyboard pill is narrower.
- Mouse: trigger click waits for a 14 px move before it becomes a drag; B is a
  frozen click (cursor never moves while held) for fiddly targets.
- Grip gestures on a screen (WayVR-style): trigger + push/pull the hand resizes
  it, stick up/down pushes it away/closer, trigger + stick left/right curves it
  (cylinder layer, per screen).

- **Layouts**: named presets of the whole arrangement (each screen's shown
  state, pose, width, curvature + the keyboard's dock/free pose). Desktop page:
  save current as new (named with the on-panel keyboard), apply, save over,
  delete (two-tap confirm). The last used one is restored on start (toggle).
  Stored in `~/.config/monadeck/desktop_layouts.json`.

- Layouts: rename, reorder, keyboard size saved per layout.
- Per-screen opacity (color-scale-bias layer). ("Keep in game" was dropped:
  you're always in-game and the menu is independent anyway.)
- Keyboard: key repeat while held, Shift double-tap = lock (third tap clears),
  click sound, size stepper (Behaviour section).
- Capture pauses after ~2 s with the screen more than ~75° off your gaze and
  resumes the moment you look back (toggle in Behaviour).
- Dashboard always wins the ray when it's under the laser (it is always
  composited on top), gaps between its panels included. Default width no longer
  touches hand-sized screens. Hiding a screen and showing it again spawns it
  fresh in front of you.
- Live readout above a gripped screen: size · distance · curve.
- **Wrist watch** on the left controller (WayVR offsets): batteries, clock +
  date, extra time zones (`watch_timezones` in overlay.json, IANA names),
  quick buttons (keyboard · recenter playspace · next layout · freeze game
  controllers), and a bottom row with the menu button + numbered screen
  toggles. Pointed at with the right hand; it wins over anything behind it.
  Toggle in Desktop → Behaviour.

- 360° background (equirect2 layer, CC0 Table Mountain 2, `skybox_path` for
  your own) while no game runs. Settings → Background.
- Capture limits: PipeWire maxFramerate cap (default 90, KWin honours it) and
  optional VR downscale. Desktop → Behaviour.
- **monado-frame folded in**: new screenshots land as a card in the watch's
  clock area (preview/date, ‹ › queue, open/dismiss, haptic tick); floating photo
  windows (3, grip to move: copy · delete · translate · share); Photos page in
  the rail with the paged gallery, finger-frame gesture settings (gestures.json)
  and photo settings (QR, crop, cleanup, skip-wrist). Translate/Share come from
  `crates/overlay/{translate,picsur}.env` at build time (gitignored).

- **Notifications** (WayVR's role): desktop notifications via a D-Bus monitor
  and XSOverlay-protocol messages on udp/42069 (VRCX etc.), queued as toasts
  with app icons. Settings → Notifications; `--notify-selftest` to check.

- Keyboard follows screens & text fields (Desktop → Behaviour, off by default):
  hides with its docked screen and returns with it; pops up under the last-used
  screen when a text field gets focus (AT-SPI, `--a11y-selftest`).
- Docking edge indicator: a teal bar on the target's edge while a gripped screen
  is in the snap zone, plus "release to dock …" in the readout.
- Watch: now-playing line with prev/play/next (MPRIS); notification history
  (last 3) behind a bell badge; the four quick buttons are configurable in
  Settings → Wrist watch (keyboard, recenter, layouts, freeze, timer,
  screenshot, screens, mute, photos).
- Layout restore starts hidden by default (Desktop → Layouts → "Start hidden"):
  the last layout is loaded and stashed, so nothing is on screen until a
  double-B brings it up exactly where it was.
- Scroll speed and drag threshold are sliders in Desktop → Behaviour.
- Settings → Controllers → Help: an in-headset card listing every gesture
  (dashboard, screens, keyboard, watch, photos).
- monado-frame retired (README notice); LinuxWiki updated for the built-in
  desktop viewer / watch / screenshots.

## Proposed next

(empty — see `docs/ideas-parked.md` for the parked list)

## Known limitations

- Absolute uinput coordinates map onto the union of all outputs; if KWin's
  workspace geometry disagrees with xdg_output (mixed scale factors), clicks may
  land off by the scale. All three of your outputs are scale 1, so untested.
- No key repeat, no held keys (a click is press+release).
- Layout switching only drives KDE (org.kde.keyboard); elsewhere it relabels the
  VR keyboard without changing what the compositor types.
- Clipboard preview needs `wl-paste` (wl-clipboard) on PATH.
- The first show of a screen that the saved token does not cover triggers the
  portal dialog on the desktop; VR can't approve it.
