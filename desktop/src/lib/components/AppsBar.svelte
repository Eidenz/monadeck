<script lang="ts">
  // Connected XR tools/overlays (wayvr, nemurixr, …). libmonado control
  // connections are filtered out in the backend; the running game is filtered
  // here — it's the "Now Playing" card's job, not this row's — so this stays
  // clean even when several tools share one libmonado.
  import { app } from "$lib/state.svelte";

  // Drop the game: the primary app, or any focused non-overlay client.
  const apps = $derived(
    app.clients.filter((c) => !c.primary && !(c.focused && !c.overlay)),
  );
</script>

<div class="apps">
  <span class="hdr">Apps</span>
  <div class="list">
    {#if apps.length === 0}
      <span class="empty">none connected</span>
    {:else}
      {#each apps as c (c.name)}
        <span class="chip" class:focused={c.focused} title={c.overlay ? "overlay" : "app"}>
          <span class="dot"></span>{c.name}
        </span>
      {/each}
    {/if}
  </div>
</div>

<style>
  .apps {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 26px;
  }
  .hdr {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.7px;
    color: hsl(var(--muted));
    flex: none;
  }
  .list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    min-width: 0;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: hsl(var(--surface-2) / 0.8);
    border: 1px solid hsl(var(--border) / 0.7);
    border-radius: 99px;
    padding: 3px 10px 3px 8px;
    font-size: 12px;
    color: hsl(var(--foreground) / 0.9);
  }
  .chip.focused {
    border-color: hsl(var(--primary) / 0.6);
    color: hsl(var(--foreground));
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: hsl(var(--muted));
  }
  .chip.focused .dot {
    background: hsl(var(--ok));
    box-shadow: 0 0 6px hsl(var(--ok) / 0.7);
  }
  .empty {
    font-size: 12px;
    color: hsl(var(--muted));
  }
</style>
