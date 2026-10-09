<script lang="ts">
  // Detached card above the deck (SteamVR-style), shown when the steamvr_lh
  // driver is selected but no room setup matches the tracking universe — so the
  // floor and forward direction would be off. Mirrors CapToast/PreflightBanner.
  // Only rendered (by +page.svelte) once a calibration can run right away:
  // without SteamVR once VR runs and the headset is up (it reads the headset's
  // pose), or through SteamVR's vrcmd while the service is stopped.
  import { app, runFloorCalibration } from "$lib/state.svelte";

  let { dismissed = $bindable(false) }: { dismissed?: boolean } = $props();

  const native = $derived(!!app.floorCal?.native);
  const blocked = $derived(native ? !app.service.connected : app.service.running);
</script>

<div class="toast" role="alert">
  <div class="title">Floor not calibrated</div>
  <div class="desc">
    {#if native}
      Your play space isn't set, so your floor height and forward direction are
      off. Stand the headset upright on the floor in the middle of your play area,
      facing your "forward", then set it.
    {:else}
      The SteamVR Lighthouse driver has no play space set, so your floor height and
      forward direction will be off. Put your headset on the floor in the middle of
      your play area (controllers off), facing your "forward", then calibrate.
    {/if}
  </div>
  {#if app.floorCalResult && !app.floorCalResult.ok}
    <div class="hint err">{app.floorCalResult.msg}</div>
  {/if}
  <div class="acts">
    <button class="btn ghost" onclick={() => (dismissed = true)}>Skip</button>
    <button
      class="btn primary"
      onclick={runFloorCalibration}
      disabled={app.calibratingFloor || blocked}
    >
      {app.calibratingFloor ? "Calibrating…" : native ? "Set floor" : "Calibrate floor"}
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
