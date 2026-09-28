<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import GameTile from "./GameTile.svelte";
  import { bind, scan, openGame, openOwn, addCustomPath, removeCustomPath } from "./store.svelte";

  const customized = $derived(bind.games.filter((g) => g.personal.length > 0).length);
  const shown = $derived(
    bind.games.filter((g) => {
      if (bind.onlyCustomized && g.personal.length === 0) return false;
      const q = bind.query.trim().toLowerCase();
      return !q || g.name.toLowerCase().includes(q);
    }),
  );
  let folders = $state(false);

  async function addFolder() {
    const picked = await openDialog({ directory: true, multiple: false, title: "Add a folder to look for VR games in" });
    if (typeof picked === "string") await addCustomPath(picked);
  }
  const folderName = (p: string) => p.split("/").filter(Boolean).pop() ?? p;
</script>

<div class="list">
  <header class="head">
    <div class="title">
      <span class="glyph"><Icon name="game-controller" size={26} /></span>
      <h1>Controller bindings</h1>
      <span class="sub">What each button does in your SteamVR games</span>
    </div>
    <div class="tools">
      <label class="search">
        <Icon name="magnifying-glass" size={15} />
        <input placeholder="Search games" bind:value={bind.query} />
      </label>
      <div class="seg">
        <button class:on={!bind.onlyCustomized} onclick={() => (bind.onlyCustomized = false)}>All games</button>
        <button class:on={bind.onlyCustomized} onclick={() => (bind.onlyCustomized = true)}>Customized · {customized}</button>
      </div>
      <div class="folders-wrap">
        <button class="icon-btn" title="Folders to look for games in" onclick={() => (folders = !folders)}><Icon name="folder-simple-plus" /></button>
        {#if folders}
          <button class="scrim" aria-label="Close" onclick={() => (folders = false)}></button>
          <div class="pop">
            <div class="pop-title">Also look in</div>
            {#each bind.customPaths as p (p)}
              <div class="folder">
                <span title={p}>{folderName(p)}</span>
                <button class="x" title="Stop looking here" onclick={() => removeCustomPath(p)}><Icon name="x" size={13} /></button>
              </div>
            {:else}
              <p class="pop-empty">Only your Steam libraries for now.</p>
            {/each}
            <button class="add" onclick={addFolder}><Icon name="plus" size={14} /> Add a folder</button>
          </div>
        {/if}
      </div>
      <button class="icon-btn" title="Look for games again" disabled={bind.scanning} onclick={scan}>
        <span class:spin={bind.scanning}><Icon name="arrows-clockwise" /></span>
      </button>
    </div>
  </header>

  <div class="scroll">
    <button class="own state-layer" onclick={() => openOwn()}>
      <span class="chip"><Icon name="arrows-out-cardinal" size={24} /></span>
      <span class="own-text">
        <span class="own-name">Monadeck</span>
        <span class="own-sub">Its own controls: the dashboard, your screens, the mouse on them, the playspace drag</span>
      </span>
      {#if bind.opening === "own"}
        <span class="spinner"></span>
      {:else if bind.ownPersonal.length}
        <span class="badge"><Icon name="sparkle" size={12} /> Customized</span>
      {/if}
      <span class="caret"><Icon name="caret-right" size={16} /></span>
    </button>

    <div class="label">Games</div>
    {#if bind.games.length === 0}
      {#if bind.scanning || !bind.scanned}
        <div class="empty"><span class="spinner"></span> Looking for games with controller bindings…</div>
      {:else}
        <div class="empty">
          <Icon name="game-controller" size={34} />
          <b>No games with controller bindings yet</b>
          <span>Games made for SteamVR Input show up here once they're installed</span>
        </div>
      {/if}
    {:else if shown.length === 0}
      <div class="empty">
        <Icon name={bind.onlyCustomized && !bind.query ? "sparkle" : "magnifying-glass"} size={34} />
        <b>{bind.onlyCustomized && !bind.query ? "Nothing customized yet" : "No game matches"}</b>
        <span>{bind.onlyCustomized && !bind.query ? "Open a game and save a change: it shows up here" : "Try another name"}</span>
      </div>
    {:else}
      <div class="grid">
        {#each shown as g (g.actionsPath)}
          <GameTile game={g} opening={bind.opening === g.actionsPath} onpick={() => openGame(g)} />
        {/each}
      </div>
    {/if}
    <p class="note">
      <Icon name="info" size={15} />
      <span>
        Not seeing a game? Only games made for SteamVR Input have bindings to change. Games that run on OpenXR directly
        (Phasmophobia, Bonelab…) and older SteamVR games come with fixed controls.
      </span>
    </p>
  </div>
</div>

<style>
  .list {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 16px 22px 12px;
    flex-wrap: wrap;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .glyph {
    color: hsl(var(--primary));
  }
  h1 {
    margin: 0;
    font-size: 21px;
    font-weight: 600;
  }
  .sub {
    color: hsl(var(--muted));
    font-size: 13px;
    margin-left: 4px;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 7px;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    border-radius: 99px;
    padding: 0 12px;
    height: 34px;
    color: hsl(var(--muted));
  }
  .search:focus-within {
    border-color: hsl(var(--primary));
  }
  .search input {
    background: transparent;
    border: none;
    outline: none;
    color: hsl(var(--foreground));
    font: inherit;
    font-size: 13px;
    width: 170px;
  }
  .seg {
    display: flex;
    gap: 2px;
    padding: 3px;
    background: hsl(var(--background));
    border: 1px solid hsl(var(--border) / 0.6);
    border-radius: 11px;
  }
  .seg button {
    border: none;
    background: transparent;
    color: hsl(var(--muted));
    border-radius: 8px;
    padding: 5px 12px;
    font-size: 12.5px;
  }
  .seg button:hover {
    color: hsl(var(--foreground));
  }
  .seg button.on {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
    font-weight: 600;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 10px;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
  }
  .icon-btn:hover:not(:disabled) {
    border-color: hsl(var(--primary) / 0.6);
  }
  .icon-btn:disabled {
    opacity: 0.5;
  }
  .spin {
    display: block;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .folders-wrap {
    position: relative;
  }
  .scrim {
    position: fixed;
    inset: 0;
    background: transparent;
    border: none;
    z-index: 40;
    cursor: default;
  }
  .pop {
    position: absolute;
    right: 0;
    top: 40px;
    z-index: 50;
    width: 280px;
    background: hsl(var(--surface));
    border: 1px solid hsl(var(--border));
    border-radius: 12px;
    box-shadow: 0 14px 34px hsl(0 0% 0% / 0.5);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .pop-title {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: hsl(var(--muted));
    padding: 4px 6px;
  }
  .pop-empty {
    margin: 2px 6px 6px;
    font-size: 12.5px;
    color: hsl(var(--muted));
  }
  .folder {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 6px 6px 10px;
    border-radius: 8px;
    background: hsl(var(--surface-2));
    font-size: 13px;
  }
  .folder span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .x {
    border: none;
    background: transparent;
    color: hsl(var(--muted));
    display: grid;
    place-items: center;
    padding: 3px;
    border-radius: 6px;
  }
  .x:hover {
    color: hsl(var(--danger));
  }
  .add {
    display: flex;
    align-items: center;
    gap: 6px;
    justify-content: center;
    margin-top: 2px;
    padding: 7px;
    border-radius: 8px;
    border: 1px dashed hsl(var(--border));
    background: transparent;
    color: hsl(var(--foreground));
    font-size: 12.5px;
  }
  .add:hover {
    border-color: hsl(var(--primary));
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 22px 22px;
  }
  .own {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 14px;
    text-align: left;
    padding: 12px 16px;
    border-radius: 14px;
    background: hsl(var(--surface));
    border: 1px solid hsl(var(--border) / 0.6);
    color: hsl(var(--foreground));
  }
  .own:hover {
    border-color: hsl(var(--primary) / 0.55);
  }
  .chip {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 12px;
    background: hsl(var(--primary) / 0.18);
    color: hsl(var(--primary));
    flex: none;
  }
  .own-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .own-name {
    font-size: 15.5px;
    font-weight: 600;
  }
  .own-sub {
    font-size: 12.5px;
    color: hsl(var(--muted));
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11.5px;
    color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.14);
    border-radius: 99px;
    padding: 3px 10px;
  }
  .caret {
    color: hsl(var(--muted));
  }
  .label {
    margin: 18px 2px 10px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: hsl(var(--muted));
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
    gap: 16px 14px;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 40px 0;
    color: hsl(var(--muted));
    font-size: 13px;
  }
  .empty b {
    color: hsl(var(--foreground));
    font-weight: 500;
    font-size: 14.5px;
  }
  .spinner {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid hsl(var(--primary) / 0.25);
    border-top-color: hsl(var(--primary));
    animation: spin 0.8s linear infinite;
    flex: none;
  }
  .note {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 22px 2px 0;
    font-size: 12.5px;
    color: hsl(var(--muted));
    line-height: 1.45;
  }
  .note :global(.icon) {
    margin-top: 1px;
  }
</style>
