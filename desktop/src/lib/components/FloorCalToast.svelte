<script lang="ts">
  // Detached card above the deck (SteamVR-style), shown when the steamvr_lh
  // driver is selected but no chaperone/play-space has been set — so the floor
  // and forward direction would be off. Mirrors CapToast/PreflightBanner. Only
  // rendered (by +page.svelte) when vrcmd is available, so the action can work.
  import { app, runFloorCalibration } from "$lib/state.svelte";

  let { dismissed = $bindable(false) }: { dismissed?: boolean } = $props();

  // vrcmd needs exclusive access to the headset, so we can't calibrate while
  // monado is holding it.
  const blocked = $derived(app.service.running);
</script>

<div class="toast" role="alert">
  <div class="title">Floor not calibrated</div>
  <div class="desc">
    The SteamVR Lighthouse driver has no play space set, so your floor height and
    forward direction will be off. Put your headset on the floor in the middle of
    your play area (controllers off), then calibrate — the headset's facing sets
    “forward”.
  </div>
  {#if blocked}
    <div class="hint">Stop the service first — calibration needs the headset.</div>
  {:else if app.floorCalResult && !app.floorCalResult.ok}
    <div class="hint err">{app.floorCalResult.msg}</div>
  {/if}
  <div class="acts">
    <button class="btn ghost" onclick={() => (dismissed = true)}>Dismiss</button>
    <button
      class="btn primary"
      onclick={runFloorCalibration}
      disabled={app.calibratingFloor || blocked}
    >
      {app.calibratingFloor ? "Calibrating…" : "Calibrate floor"}
    </button>
  </div>
</div>

<style>
  .toast {
    background: hsl(var(--surface) / 0.97);
    border: 1px solid hsl(var(--border) / 0.8);
    border-radius: var(--radius);
    padding: 12px 14px;
    box-shadow: 0 12px 30px hsl(0 0% 0% / 0.5);
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .title {
    font-size: 13px;
    font-weight: 600;
    color: hsl(var(--foreground));
  }
  .desc {
    font-size: 12px;
    color: hsl(var(--muted));
    line-height: 1.45;
  }
  .hint {
    font-size: 11.5px;
    color: hsl(var(--warn));
  }
  .hint.err {
    color: hsl(var(--danger));
  }
  .acts {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 3px;
  }
  .btn {
    border-radius: var(--radius-s);
    padding: 6px 14px;
    font-size: 12.5px;
    font-weight: 600;
    border: 1px solid transparent;
  }
  .ghost {
    background: hsl(var(--surface-2));
    border-color: hsl(var(--border));
    color: hsl(var(--foreground));
  }
  .ghost:hover {
    background: hsl(var(--surface-2) / 0.7);
  }
  .primary {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
  }
  .primary:disabled {
    opacity: 0.6;
  }
</style>
