<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  // SteamVR's deck has only minimize + close (no maximize). The settings window
  // hides instead of closing so its event listener stays alive for re-opening.
  let { closeAction = "close" }: { closeAction?: "close" | "hide" } = $props();
  const win = getCurrentWindow();

  function onClose() {
    if (closeAction === "hide") win.hide();
    else win.close();
  }
</script>

<div class="controls">
  <button class="ctl state-layer" aria-label="Minimize" onclick={() => win.minimize()}>
    <svg width="11" height="11" viewBox="0 0 11 11"><rect y="5" width="11" height="1.2" fill="currentColor" /></svg>
  </button>
  <button class="ctl close state-layer" aria-label="Close" onclick={onClose}>
    <svg width="11" height="11" viewBox="0 0 11 11"><path d="M1 1l9 9M10 1l-9 9" stroke="currentColor" stroke-width="1.2" /></svg>
  </button>
</div>

<style>
  .controls {
    display: flex;
    gap: 2px;
  }
  .ctl {
    display: grid;
    place-items: center;
    width: 28px;
    height: 24px;
    border: none;
    background: transparent;
    color: hsl(var(--muted));
    border-radius: var(--radius-s);
  }
  .ctl:hover {
    color: hsl(var(--foreground));
  }
  .ctl.close:hover {
    background: hsl(var(--danger) / 0.85);
    color: white;
  }
</style>
