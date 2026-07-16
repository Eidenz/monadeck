<script lang="ts">
  // Floats above the deck after the kwin freeze watch fired: the headset served
  // a corrupt EDID on cold start, kwin adopted it as a desktop monitor and
  // froze every output retrying a failing modeset. We disabled the adopted
  // output to unfreeze the desktop; monado itself likely failed to grab the
  // headset, so the fix is a stop + start (occasionally a replug).
  import { app } from "$lib/state.svelte";

  const outputs = $derived(app.freeze?.disabled_outputs ?? []);
  const recovered = $derived(outputs.length > 0);
</script>

<div class="toast" role="alert">
  {#if recovered}
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
      The desktop compositor is stuck retrying a failing display change,
      likely from the headset waking up with a garbled EDID — but the culprit
      output couldn't be identified. If the screen is frozen, unplug the
      headset to recover, then start again.
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
