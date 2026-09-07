<script lang="ts">
  // Shown on the deck when no valid Monado prefix is set (the service binary
  // isn't found). The actual install happens in Settings, where there's room for
  // progress/feedback — this is just the first-run nudge. Dismissable.
  import { openSettings } from "$lib/windows";
  import { app } from "$lib/state.svelte";

  let { dismissed = $bindable(false) }: { dismissed?: boolean } = $props();
  const isWivrn = $derived(app.config?.backend === "wivrn");

  async function setup() {
    await openSettings("general");
    dismissed = true;
  }
</script>

<div class="toast" role="alert">
  <div class="title">{isWivrn ? "WiVRn not found" : "No Monado runtime"}</div>
  <div class="desc">
    {#if isWivrn}
      Monadeck couldn't find <code>wivrn-server</code>. Install WiVRn from your
      distro, or point Settings at it (or switch back to Monado).
    {:else}
      Monadeck couldn't find <code>monado-service</code>. Install the built-in build
      or point it at your own, both are in Settings.
    {/if}
  </div>
  <div class="acts">
    <button class="btn ghost" onclick={() => (dismissed = true)}>Dismiss</button>
    <button class="btn primary" onclick={setup}>Set up</button>
  </div>
</div>

<style>
  .toast {
    background: hsl(var(--surface) / 0.97);
    border: 1px solid hsl(var(--warn) / 0.5);
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
  code {
    background: hsl(var(--background) / 0.7);
    padding: 0 4px;
    border-radius: 4px;
    font-size: 11px;
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
</style>
