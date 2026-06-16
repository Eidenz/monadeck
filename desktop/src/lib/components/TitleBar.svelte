<script lang="ts">
  import { app } from "$lib/state.svelte";
  import WindowControls from "./WindowControls.svelte";
  import BurgerMenu from "./BurgerMenu.svelte";
</script>

<header class="bar" data-tauri-drag-region>
  <BurgerMenu />
  <div class="brand" data-tauri-drag-region>
    <span class="name">Monadeck</span>
    {#if app.version}<span class="ver">{app.version}</span>{/if}
  </div>
  <div class="spacer" data-tauri-drag-region></div>
  <WindowControls />
</header>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px 5px 8px;
    height: 36px;
    flex: none;
  }
  .brand {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .name {
    font-size: 13px;
    font-weight: 600;
    color: hsl(var(--foreground) / 0.9);
    letter-spacing: 0.2px;
  }
  .ver {
    font-size: 10.5px;
    color: hsl(var(--muted) / 0.8);
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
</style>
