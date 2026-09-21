<script lang="ts">
  // Floats above the deck after the kwin freeze watch fired. Two variants:
  // the headset served a corrupt EDID on cold start and kwin adopted it as a
  // desktop monitor (we disabled that output), or kwin just got stuck
  // retrying a failing commit when the panel woke (only stopping Monado
  // clears it — the watch does that, then starts it once more on its own).
  // The report is live: it moves from "starting again" to the outcome.
  import { app, dismissFreeze } from "$lib/state.svelte";

  const outputs = $derived(app.freeze?.disabled_outputs ?? []);
  const stopped = $derived(app.freeze?.service_stopped ?? false);
  const restart = $derived(app.freeze?.restart ?? "none");
  const adopted = $derived(outputs.length > 0 && !stopped);
  const cause =
    "The desktop compositor got stuck retrying a failing display change when the headset woke up.";
</script>

<div class="toast" class:ok={restart === "ok"} role="alert">
  {#if stopped && restart === "pending"}
    <div class="title">Recovering from a display freeze…</div>
    <div class="desc">
      {cause} Monadeck stopped Monado to release the headset, which clears it,
      and is starting it again. Nothing to do.
    </div>
  {:else if stopped && restart === "ok"}
    <div class="title">Recovered from a display freeze</div>
    <div class="desc">
      {cause} Monadeck stopped Monado to release the headset and started it
      again. Everything is back up.
    </div>
  {:else if stopped && restart === "failed"}
    <div class="title">Display freeze cleared, but Monado didn't come back</div>
    <div class="desc">
      {cause} Monadeck stopped Monado to release the headset, but starting it
      again failed{app.freeze?.restart_error ? `: ${app.freeze.restart_error}` : "."}
      Start it once the screen is back, or replug the headset.
    </div>
  {:else if stopped}
    <div class="title">Stopped after a second display freeze</div>
    <div class="desc">
      {cause} Monadeck already restarted Monado once for this and it froze
      again, so it stays stopped this time. Replug the headset, then start it.
    </div>
  {:else if adopted}
    <div class="title">Recovered from a display freeze</div>
    <div class="desc">
      The headset woke up with a garbled EDID and the desktop compositor
      briefly adopted it as a monitor ({outputs.join(", ")}), freezing the
      screen. Monadeck released it. If the headset stays dark, Monado missed
      it: stop and start once.
    </div>
  {:else}
    <div class="title">Display freeze detected</div>
    <div class="desc">
      {cause} If it doesn't clear within a second, Monadeck stops Monado to
      release the headset and starts it again on its own.
    </div>
  {/if}
  <div class="acts">
    <button class="btn ghost" onclick={dismissFreeze}>Dismiss</button>
  </div>
</div>

<style>
  .toast.ok {
    border-color: hsl(var(--ok) / 0.55);
  }
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
