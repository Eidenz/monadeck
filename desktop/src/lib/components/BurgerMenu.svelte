<script lang="ts">
  // Opens the settings window directly (navigation lives in its sidebar).
  import { openSettings } from "$lib/windows";
</script>

<button
  class="trigger state-layer"
  aria-label="Open settings"
  title="Settings"
  onclick={() => openSettings()}
>
  <svg width="16" height="16" viewBox="0 0 16 16">
    <path d="M2 4h12M2 8h12M2 12h12" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
  </svg>
</button>

<style>
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
</style>
