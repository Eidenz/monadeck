<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import CapToast from "$lib/components/CapToast.svelte";
  import CrashToast from "$lib/components/CrashToast.svelte";
  import PreflightBanner from "$lib/components/PreflightBanner.svelte";
  import NoRuntimeBanner from "$lib/components/NoRuntimeBanner.svelte";
  import DeviceStrip from "$lib/components/DeviceStrip.svelte";
  import AppsBar from "$lib/components/AppsBar.svelte";
  import {
    app,
    loadInitial,
    refreshStatus,
    refreshSnapshot,
    refreshConfig,
    start,
    stop,
  } from "$lib/state.svelte";
  import { eyetrackingStatus } from "$lib/api";

  let dismissed = $state(false);
  // Beyond eye tracking running -> show an eye in the device strip.
  let eyeRunning = $state(false);
  let preflightDismissed = $state(false);
  let noRuntimeDismissed = $state(false);
  const showToast = $derived(app.caps === "needs_setcap" && !dismissed);
  const showCrash = $derived(app.crash !== null);
  const showPreflight = $derived(
    app.preflight !== null && !app.preflight.all_ok && !preflightDismissed,
  );
  // No valid Monado prefix (service binary missing) → offer setup.
  const showNoRuntime = $derived(app.caps === "no_binary" && !noRuntimeDismissed);

  // The HMD only appears in the device snapshot (its icon turns green) once Monado
  // actually has the headset ready — a truer "ready" signal than the raw IPC
  // connection, which flips well before the headset is up. Same check DeviceStrip
  // uses for the head slot.
  const headsetReady = $derived(
    app.devices.some((d) => d.role === "head" || d.kind === "hmd"),
  );

  // The detected "game": the primary app (fall back to a focused non-overlay).
  const game = $derived(
    app.service.connected
      ? (app.clients.find((c) => c.primary) ??
          app.clients.find((c) => c.focused && !c.overlay) ??
          null)
      : null,
  );

  // Stopped → Warming up… (headset not ready) → Ready (headset up, idle) →
  // Now Playing (a game is running).
  const heading = $derived(
    !app.service.running
      ? "Stopped"
      : !headsetReady
        ? "Warming up…"
        : game
          ? "Now Playing"
          : "Ready",
  );

  // The window auto-sizes to the deck's measured content — compact by default,
  // growing as devices/apps appear. Notices float detached BELOW the deck: the
  // window only ever grows downward from a fixed top-left (a resize is anchored
  // top-left on KWin/Wayland), so the deck stays pinned in place. We deliberately
  // never reposition the window — moving it to grow upward is a no-op on Wayland
  // and only ever shoved the deck down.
  const WIN_W = 380;
  let contentH = $state(0); // measured deck (+ error) height
  let toastSlotH = $state(0); // measured notice card height
  const toastH = $derived(
    showToast || showCrash || showPreflight || showNoRuntime ? toastSlotH : 0,
  );

  let desiredH: number | null = null;
  let applying = false;

  function requestResize(total: number) {
    desiredH = total;
    if (!applying) drainResize();
  }
  async function drainResize() {
    applying = true;
    const win = getCurrentWindow();
    while (desiredH !== null) {
      const total = desiredH;
      desiredH = null;
      try {
        await win.setSize(new LogicalSize(WIN_W, Math.max(1, Math.round(total))));
      } catch {
        // transient window-op failures are harmless; next change re-applies
      }
    }
    applying = false;
  }

  $effect(() => {
    if (contentH > 0) requestResize(contentH + toastH);
  });

  onMount(() => {
    (async () => {
      await loadInitial();
      // Auto-start the service on launch when enabled (and not already up).
      if (
        app.config?.auto_start &&
        app.config?.monado_prefix &&
        !app.service.running
      ) {
        start();
      }
    })();
    const t = setInterval(async () => {
      await refreshStatus();
      await refreshSnapshot();
      await refreshConfig();
      try {
        eyeRunning = (await eyetrackingStatus()).running;
      } catch {
        eyeRunning = false;
      }
    }, 1500);
    return () => clearInterval(t);
  });
</script>

<div class="deck-window">
  <div class="content" bind:clientHeight={contentH}>
    <div class="deck">
      <TitleBar />
      <div class="body">
        <div class="status-row">
          <div class="heading-wrap">
            <div class="heading">{heading}</div>
            {#if game}<div class="game" title={game.name}>{game.name}</div>{/if}
          </div>
          {#if app.service.running}
            <button class="pwr stop" onclick={stop} disabled={app.busy}>Stop</button>
          {:else}
            <button
              class="pwr start"
              onclick={start}
              disabled={app.busy || !app.config?.monado_prefix}
              title={app.config?.monado_prefix
                ? "Start monado-service"
                : "Set the Monado prefix in Settings first"}
            >
              {app.busy ? "…" : "Start"}
            </button>
          {/if}
        </div>
        <div class="divider"></div>
        <DeviceStrip devices={app.devices} eyeActive={eyeRunning} />
        <div class="divider"></div>
        <AppsBar />
      </div>
    </div>

    {#if app.error}
      <div class="err" role="alert">
        <span>{app.error}</span>
        <button onclick={() => (app.error = "")} aria-label="Dismiss">✕</button>
      </div>
    {/if}
  </div>

  {#if showToast || showCrash || showPreflight || showNoRuntime}
    <div class="toast-slot" bind:clientHeight={toastSlotH}>
      {#if showCrash}<CrashToast />{/if}
      {#if showNoRuntime}<NoRuntimeBanner bind:dismissed={noRuntimeDismissed} />{/if}
      {#if showPreflight}<PreflightBanner bind:dismissed={preflightDismissed} />{/if}
      {#if showToast}<CapToast bind:dismissed />{/if}
    </div>
  {/if}
</div>

<style>
  .deck-window {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  /* Notices float detached below the deck: inset on the sides with a gap above,
     the surrounding transparency showing the desktop (SteamVR-style). */
  .toast-slot {
    flex: none;
    padding: 12px 8px 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  /* Natural-height content (deck + any error), measured to size the window. */
  .content {
    flex: none;
  }
  /* The deck fills the window edge-to-edge with square corners — no rounding,
     so the transparent window leaves no halo/border around it. */
  .deck {
    overflow: visible;
    background:
      radial-gradient(120% 90% at 30% -10%, hsl(195 35% 18% / 0.6), transparent 60%),
      linear-gradient(180deg, hsl(var(--surface)), hsl(var(--background)));
  }
  .body {
    padding: 6px 16px 16px;
  }
  .status-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .heading-wrap {
    min-width: 0;
  }
  .heading {
    font-size: 21px;
    font-weight: 600;
    color: hsl(var(--foreground));
    letter-spacing: 0.2px;
  }
  .game {
    font-size: 12px;
    color: hsl(var(--primary));
    margin-top: 1px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pwr {
    flex: none;
    border: none;
    border-radius: var(--radius-s);
    padding: 5px 16px;
    font-size: 12.5px;
    font-weight: 700;
  }
  .pwr.start {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
  }
  .pwr.stop {
    background: hsl(var(--danger) / 0.9);
    color: white;
  }
  .pwr:disabled {
    opacity: 0.45;
  }
  .divider {
    height: 1px;
    background: hsl(var(--border) / 0.8);
    margin: 12px 0 14px;
  }
  .err {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 12px;
    background: hsl(var(--danger) / 0.16);
    border: 1px solid hsl(var(--danger) / 0.4);
    border-radius: var(--radius);
    font-size: 12px;
  }
  .err button {
    background: transparent;
    border: none;
    color: hsl(var(--muted));
  }
</style>
