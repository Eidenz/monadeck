<script lang="ts">
  // Detached card above the deck, shown when a runtime prerequisite is missing
  // (xr-hardware udev rules, pkexec). On a properly set-up machine this never
  // appears — it earns its keep when Monadeck is run on someone else's box.
  // The install commands live in Settings › Environment; this is just the nudge.
  import { app } from "$lib/state.svelte";
  import { openSettings } from "$lib/windows";

  let { dismissed = $bindable(false) }: { dismissed?: boolean } = $props();

  const missing = $derived(app.preflight?.checks.filter((c) => !c.ok) ?? []);
  const hasImportant = $derived(missing.some((c) => c.severity === "important"));

  async function fix() {
    await openSettings("environment");
    dismissed = true;
  }
</script>

<div class="toast" class:warn={hasImportant} role="alert">
  <div class="title">VR prerequisites missing</div>
  <div class="desc">
    {#each missing as c (c.id)}
      <div class="item">
        <span class="dot" class:imp={c.severity === "important"}></span>
        {c.label}
      </div>
    {/each}
  </div>
  <div class="acts">
    <button class="btn ghost" onclick={() => (dismissed = true)}>Dismiss</button>
    <button class="btn primary" onclick={fix}>How to fix</button>
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
  .toast.warn {
    border-color: hsl(var(--warn) / 0.5);
  }
  .title {
    font-size: 13px;
    font-weight: 600;
    color: hsl(var(--foreground));
  }
  .desc {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: hsl(var(--muted));
  }
  .item {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 99px;
    background: hsl(var(--muted));
    flex: none;
  }
  .dot.imp {
    background: hsl(var(--warn));
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
