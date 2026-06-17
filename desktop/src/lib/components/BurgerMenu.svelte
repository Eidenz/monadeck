<script lang="ts">
  import { openSettings, openBindings } from "$lib/windows";

  let open = $state(false);

  const items = [
    { label: "Settings", run: () => openSettings() },
    { label: "Binding editor", run: () => openBindings() },
  ];

  function pick(run: () => void) {
    open = false;
    run();
  }
</script>

<div class="burger">
  <button
    class="trigger state-layer"
    aria-label="Menu"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    <svg width="16" height="16" viewBox="0 0 16 16">
      <path d="M2 4h12M2 8h12M2 12h12" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
    </svg>
  </button>

  {#if open}
    <button class="scrim" aria-label="Close menu" onclick={() => (open = false)}></button>
    <div class="menu" role="menu">
      {#each items as it (it.label)}
        <button class="item state-layer" role="menuitem" onclick={() => pick(it.run)}>
          {it.label}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .burger {
    position: relative;
  }
  .trigger {
    display: grid;
    place-items: center;
    width: 28px;
    height: 24px;
    border: none;
    background: transparent;
    color: hsl(var(--muted));
    border-radius: var(--radius-s);
  }
  .trigger:hover {
    color: hsl(var(--foreground));
  }
  .scrim {
    position: fixed;
    inset: 0;
    background: transparent;
    border: none;
    z-index: 40;
    cursor: default;
  }
  .menu {
    position: absolute;
    top: 30px;
    left: 0;
    z-index: 50;
    min-width: 160px;
    padding: 5px;
    display: flex;
    flex-direction: column;
    /* opaque for readability over the deck */
    background: hsl(var(--surface));
    border: 1px solid hsl(var(--border));
    border-radius: var(--radius);
    box-shadow: 0 12px 30px hsl(0 0% 0% / 0.5);
  }
  .item {
    text-align: left;
    background: transparent;
    border: none;
    color: hsl(var(--foreground));
    padding: 8px 10px;
    font-size: 13px;
    border-radius: var(--radius-s);
  }
</style>
