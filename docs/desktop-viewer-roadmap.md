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

## Proposed next

1. **Remember screen placement.** Persist each screen's pose, width and
   curvature per output name so DP-3 comes back where you left it instead of in
   front of your head every launch. Also remember the keyboard's dock target /
   free pose. Then: named **favourite layouts** (save/recall whole arrangements),
   switchable from the Desktop page.
2. **Opacity per screen**, and a "keep visible while playing" toggle so a
   game's own frames don't fight a screen you want to keep as a HUD.
3. **Middle click.** All face buttons are used now (trigger/A/B); a long-press
   or stick-click could carry it.
4. **Keyboard extras.** Key repeat while held (arrows, backspace), a
   press-and-hold Shift (lock on double tap), key-click sound, scale slider.
5. **Keyboard top-bar ideas** (not yet decided): current window title, an
   emoji picker, a "type from VR search box" text field that sends the whole
   string at once.
6. **Window capture.** The portal also offers single windows (source type
   Window). Same pipeline, smaller quads; useful for a chat window as a HUD.
7. **Mirror pause when hidden / when nobody is looking** to save GPU: pause
   the PipeWire stream when the screen is out of view for a while.
8. **Multi-cursor sanity.** Only one hand drives the mouse today (closest hit
   wins). Consider hand priority or a "dominant hand" setting.

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
