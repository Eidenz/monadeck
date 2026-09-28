<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import WindowControls from "$lib/components/WindowControls.svelte";
  import Icon from "$lib/bindings/Icon.svelte";
  import GameList from "$lib/bindings/GameList.svelte";
  import Editor from "$lib/bindings/Editor.svelte";
  import { bind, init, scan } from "$lib/bindings/store.svelte";

  onMount(() => {
    const win = getCurrentWindow();
    let unfocus: (() => void) | undefined;
    // Look for games whenever the window is shown: we can't rely on a
    // cross-window event (a hidden window's webview may register its listener
    // after the one-shot emit fired), so react to its own visibility / focus.
    (async () => {
      await init();
      if (await win.isVisible()) scan();
      unfocus = await win.onFocusChanged(({ payload: focused }) => {
        if (focused && !bind.editing) scan();
      });
    })();
    return () => unfocus?.();
  });
</script>

<div class="win">
  <header class="topbar" data-tauri-drag-region>
    <span class="ttl" data-tauri-drag-region>Monad<b>eck</b> · Controller bindings</span>
    <div class="spacer" data-tauri-drag-region></div>
    <WindowControls closeAction="hide" />
  </header>

  {#if bind.editing}
    <Editor />
  {:else}
    <GameList />
  {/if}

  {#if bind.notice}
    <div class="notice" role="status"><Icon name="check" size={16} /> {bind.notice}</div>
  {/if}
  {#if bind.error && !bind.editing}
    <div class="error" role="alert">
      <Icon name="warning" size={16} />
      {bind.error}
      <button aria-label="Dismiss" onclick={() => (bind.error = "")}><Icon name="x" size={14} /></button>
    </div>
  {/if}
</div>

<style>
  .win {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: hsl(var(--background));
    position: relative;
  }
  .topbar {
    display: flex;
    align-items: center;
    height: 40px;
    padding: 0 6px 0 14px;
    border-bottom: 1px solid hsl(var(--border) / 0.6);
    flex: none;
  }
  .ttl {
    font-size: 13px;
    color: hsl(var(--muted));
  }
  .ttl b {
    color: hsl(var(--foreground));
    font-weight: 600;
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .notice,
  .error {
    position: absolute;
    left: 50%;
    bottom: 76px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 16px;
    border-radius: 99px;
    font-size: 13px;
    z-index: 70;
    box-shadow: 0 10px 26px hsl(0 0% 0% / 0.45);
  }
  .notice {
    background: hsl(168 60% 18%);
    border: 1px solid hsl(var(--primary) / 0.6);
    color: hsl(var(--foreground));
  }
  .notice :global(.icon) {
    color: hsl(var(--primary));
  }
  .error {
    background: hsl(var(--danger) / 0.2);
    border: 1px solid hsl(var(--danger) / 0.55);
    color: hsl(var(--foreground));
  }
  .error button {
    border: none;
    background: transparent;
    color: hsl(var(--muted));
    display: grid;
    place-items: center;
  }
</style>
