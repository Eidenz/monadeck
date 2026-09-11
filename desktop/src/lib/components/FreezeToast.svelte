<script lang="ts">
  // Floats above the deck after the kwin freeze watch fired. Two variants:
  // the headset served a corrupt EDID on cold start and kwin adopted it as a
  // desktop monitor (we disabled that output), or kwin just got stuck
  // retrying a failing commit when the panel woke (only stopping Monado
  // clears it — the watch did that, and starts it once more).
  import { app } from "$lib/state.svelte";

  const outputs = $derived(app.freeze?.disabled_outputs ?? []);
  const stopped = $derived(app.freeze?.service_stopped ?? false);
  const restarting = $derived(app.freeze?.restarting ?? false);
  const recovered = $derived(outputs.length > 0 && !stopped);
</script>

<div class="toast" role="alert">
  {#if stopped}
    <div class="title">Recovered from a display freeze</div>
    <div class="desc">
      The desktop compositor got stuck retrying a failing display change when
      the headset woke up. Monadeck stopped Monado to release the headset,
      which clears it{restarting
        ? ", and is starting Monado again — the second try usually goes through."
        : ". That was the second try in a row, so it stays stopped; start again when the screen is back, or replug the headset."}
    </div>
  {:else if recovered}
    <div class="title">Recovered from a display freeze</div>
    <div class="desc">
      The headset woke up with a garbled EDID and the desktop compositor
      briefly adopted it as a monitor ({outputs.join(", ")}), freezing the
      screen. Monadeck released it, but Monado probably missed the headset —
      stop and start again to retry.
    </div>
  {:else}
    <div class="title">Display freeze detected</div>
    <div class="desc">
      The desktop compositor is stuck retrying a failing display change after
      the headset woke up, and no adopted output could be found to release.
      If it doesn't clear within a second, Monadeck stops Monado to release
      the headset and starts it again.
    </div>
  {/if}
  <div class="acts">
    <button class="btn ghost" onclick={() => (app.freeze = null)}>Dismiss</button>
  </div>
</div>

<style>
  .toast {
    background: hsl(var(--surface) / 0.97);
    border: 1px solid hsl(var(--danger) / 0.5);
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
</style>
