<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { open } from "@tauri-apps/plugin-dialog";
  import WindowControls from "$lib/components/WindowControls.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import GameTile from "$lib/bindings/GameTile.svelte";
  import ControllerDiagram from "$lib/bindings/ControllerDiagram.svelte";
  import HandPanel from "$lib/bindings/HandPanel.svelte";
  import {
    editor,
    scan,
    selectGame,
    selectBinding,
    editBindingText,
    save,
    saveAsOverride,
    isOverride,
    setActiveActionSet,
    setMirrorMode,
    setSelectedInput,
    actionSets,
    loadCustomPaths,
    addCustomPath,
    removeCustomPath,
    loadDefaultController,
    setDefaultController,
  } from "$lib/bindings/store.svelte";

  async function addFolder() {
    const picked = await open({ directory: true, multiple: false, title: "Add a folder to scan for VR bindings" });
    if (typeof picked === "string") await addCustomPath(picked);
  }

  let view = $state<"visual" | "raw">("visual");
  const sets = $derived(actionSets());
  const canVisual = $derived(!!editor.bindingConfig);
  const override = $derived(isOverride());

  // game list filter + search
  let filter = $state<string>("all"); // "all" | "steam" | <custom path>
  let gameQuery = $state("");
  const folderName = (p: string) => p.split("/").filter(Boolean).pop() ?? p;
  const inCustomFolder = (path: string) =>
    editor.customPaths.some((p) => path.startsWith(p));
  const filteredGames = $derived(
    editor.games.filter((g) => {
      if (filter === "steam" && inCustomFolder(g.gamePath)) return false;
      if (filter !== "all" && filter !== "steam" && !g.gamePath.startsWith(filter))
        return false;
      const q = gameQuery.trim().toLowerCase();
      if (q && !g.name.toLowerCase().includes(q)) return false;
      return true;
    }),
  );
  function removeFolder(p: string) {
    if (filter === p) filter = "all";
    removeCustomPath(p);
  }

  // controller selector dropdown
  let ctlOpen = $state(false);
  let ctlQuery = $state("");
  const ctlList = $derived(
    (editor.game?.bindingFiles ?? []).filter(
      (b) =>
        !ctlQuery.trim() ||
        b.controllerType.toLowerCase().includes(ctlQuery.toLowerCase()),
    ),
  );

  onMount(() => {
    loadCustomPaths();
    loadDefaultController();
    const win = getCurrentWindow();
    let unfocus: (() => void) | undefined;
    // Auto-scan whenever the window is opened. We can't rely on a cross-window
    // event (a hidden window's webview may register its listener after the
    // one-shot emit fired), so we react to the window's own visibility/focus:
    // scan if we mount already visible, and on every focus-gain afterwards.
    (async () => {
      if ((await win.isVisible()) && !editor.scanning) scan();
      unfocus = await win.onFocusChanged(({ payload: focused }) => {
        if (focused && !editor.scanning) scan();
      });
    })();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && editor.selectedInput) setSelectedInput(null);
    };
    window.addEventListener("keydown", onKey);
    return () => {
      unfocus?.();
      window.removeEventListener("keydown", onKey);
    };
  });
</script>

<div class="win">
  <header class="topbar" data-tauri-drag-region>
    <span class="ttl" data-tauri-drag-region>Monad<b>eck</b> · Binding Editor</span>
    <div class="spacer" data-tauri-drag-region></div>
    <WindowControls closeAction="hide" />
  </header>

  <div class="cols">
    <aside class="games">
      <div class="games-head">
        <span>Games</span>
        <button class="rescan" onclick={scan} disabled={editor.scanning}>
          {editor.scanning ? "Scanning…" : "Rescan"}
        </button>
      </div>
      <div class="filters">
        <button class="chip" class:active={filter === "all"} onclick={() => (filter = "all")}>All</button>
        <button class="chip" class:active={filter === "steam"} onclick={() => (filter = "steam")}>Steam</button>
        {#each editor.customPaths as p (p)}
          <span class="chip folderchip" class:active={filter === p}>
            <button class="chip-main" title={p} onclick={() => (filter = p)}>{folderName(p)}</button>
            <button class="chip-x" title="Remove folder" onclick={() => removeFolder(p)}>✕</button>
          </span>
        {/each}
        <button class="chip add" title="Add folder" onclick={addFolder}>+</button>
      </div>
      <div class="search">
        <input placeholder="Search games…" bind:value={gameQuery} />
      </div>
      <div class="games-list">
        {#if editor.scanning && editor.games.length === 0}
          <p class="hint">Scanning Steam libraries…</p>
        {:else if filteredGames.length === 0}
          <p class="hint">
            {editor.games.length === 0
              ? "No VR games with SteamVR/xrizer bindings found."
              : "No games match."}
          </p>
        {:else}
          {#each filteredGames as g (g.actionsPath + "::" + g.source)}
            <GameTile
              game={g}
              active={editor.game?.actionsPath === g.actionsPath &&
                editor.game?.source === g.source}
              onpick={() => selectGame(g)}
            />
          {/each}
        {/if}
      </div>
    </aside>

    <main class="editor">
      {#if !editor.game}
        <div class="placeholder">Select a game to edit its xrizer controller bindings.</div>
      {:else}
        <div class="ed-head">
          <div class="g-meta">
            <div class="g-name">{editor.game.name}</div>
            <div class="g-src">{editor.game.source}</div>
          </div>
          <div class="ctl-select">
            <span class="ctl-cap">Controller</span>
            <button class="ctl-trigger" onclick={() => (ctlOpen = !ctlOpen)}>
              {editor.binding?.controllerType ?? "—"} <span class="caret">▾</span>
            </button>
            <button
              class="star"
              class:on={editor.binding?.controllerType === editor.defaultController}
              title="Set as default controller (auto-selected when you open a game)"
              onclick={() => editor.binding && setDefaultController(editor.binding.controllerType)}
            >★</button>
            {#if ctlOpen}
              <button class="scrim" aria-label="Close" onclick={() => (ctlOpen = false)}></button>
              <div class="ctl-menu">
                <input class="ctl-search" placeholder="Search controllers…" bind:value={ctlQuery} />
                <div class="ctl-list">
                  {#each ctlList as b (b.filePath)}
                    <button class="ctl-item" class:active={editor.binding?.filePath === b.filePath} onclick={() => { selectBinding(b); ctlOpen = false; ctlQuery = ""; }}>
                      <span>{b.controllerType}</span>
                      {#if b.controllerType === editor.defaultController}<span class="def">★</span>{/if}
                    </button>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        </div>

        {#if editor.binding}
          <div class="toolbar">
            <div class="tabs">
              <button class="tab" class:active={view === "visual"} disabled={!canVisual} onclick={() => (view = "visual")}>Visual</button>
              <button class="tab" class:active={view === "raw"} onclick={() => (view = "raw")}>Raw JSON</button>
            </div>
            {#if view === "visual" && canVisual}
              {#if sets.length > 1}
                <select class="setsel" value={editor.activeActionSet} onchange={(e) => setActiveActionSet(e.currentTarget.value)}>
                  {#each sets as s (s)}<option value={s}>{s}</option>{/each}
                </select>
              {/if}
              <label class="mirror"><Toggle label="Mirror" checked={editor.mirrorMode} onchange={(v) => setMirrorMode(v)} /> Mirror</label>
            {/if}
            <div class="spacer"></div>
            {#if editor.dirty}<span class="dirty">● unsaved</span>{/if}
            {#if override}
              <button class="save" onclick={save} disabled={editor.saving || !editor.dirty} title="Save the xrizer override in place">{editor.saving ? "Saving…" : "Save"}</button>
            {:else}
              <button class="save-alt" onclick={save} disabled={editor.saving} title="Overwrite the game's default binding (Steam may revert it on update)">Overwrite default</button>
              <button class="save" onclick={saveAsOverride} disabled={editor.saving} title="Write to <game>/xrizer/ so the game's default file stays untouched">{editor.saving ? "Saving…" : "Save as xrizer override"}</button>
            {/if}
          </div>

          {#if view === "visual" && canVisual}
            <div class="grid">
              <div class="side"><HandPanel hand="left" /></div>
              <ControllerDiagram hand="left" />
              <ControllerDiagram hand="right" />
              <div class="side"><HandPanel hand="right" /></div>
            </div>
          {:else}
            <textarea class="raw" spellcheck="false" value={editor.bindingJson} oninput={(e) => editBindingText(e.currentTarget.value)}></textarea>
            {#if !canVisual}<p class="warn">This binding file isn't valid JSON, so the visual editor is unavailable — fix it here.</p>{/if}
          {/if}
        {/if}
      {/if}

      {#if editor.error}
        <div class="err" role="alert">{editor.error}<button onclick={() => (editor.error = "")} aria-label="Dismiss">✕</button></div>
      {/if}
    </main>
  </div>
</div>

<style>
  .win {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: hsl(var(--background));
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
  .cols {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .games {
    width: 270px;
    flex: none;
    border-right: 1px solid hsl(var(--border) / 0.6);
    background: hsl(var(--surface) / 0.5);
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }
  .games-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 11px 12px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: hsl(var(--muted));
    flex: none;
  }
  .rescan {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 4px 10px;
    font-size: 11px;
  }
  .games-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hint {
    font-size: 12.5px;
    color: hsl(var(--muted));
    padding: 8px 6px;
  }
  .filters {
    flex: none;
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    padding: 0 10px 8px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--muted));
    border-radius: 99px;
    padding: 3px 10px;
    font-size: 11px;
    cursor: pointer;
  }
  .chip.active {
    border-color: hsl(var(--primary));
    color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.12);
  }
  .chip.add {
    padding: 3px 9px;
    font-weight: 700;
  }
  .folderchip {
    padding: 0;
    gap: 0;
    overflow: hidden;
  }
  .folderchip .chip-main {
    background: transparent;
    border: none;
    color: inherit;
    padding: 3px 4px 3px 10px;
    font-size: 11px;
    cursor: pointer;
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .folderchip .chip-x {
    background: transparent;
    border: none;
    color: hsl(var(--muted) / 0.7);
    padding: 3px 8px 3px 2px;
    cursor: pointer;
    font-size: 10px;
  }
  .folderchip .chip-x:hover {
    color: hsl(var(--danger));
  }
  .search {
    flex: none;
    padding: 0 10px 8px;
  }
  .search input {
    width: 100%;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 6px 10px;
    font-size: 12px;
  }
  .search input:focus {
    outline: none;
    border-color: hsl(var(--primary));
  }
  .editor {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    padding: 14px 16px;
    gap: 10px;
    min-height: 0;
  }
  .placeholder {
    margin: auto;
    color: hsl(var(--muted));
    font-size: 13px;
  }
  .ed-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    flex: none;
  }
  .g-name {
    font-size: 17px;
    font-weight: 600;
  }
  .g-src {
    font-size: 11.5px;
    color: hsl(var(--muted));
  }
  .ctl-select {
    position: relative;
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .ctl-cap {
    font-size: 11px;
    color: hsl(var(--muted));
  }
  .ctl-trigger {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 5px 11px;
    font-size: 12px;
    min-width: 150px;
    text-align: left;
    cursor: pointer;
  }
  .caret {
    float: right;
    color: hsl(var(--muted));
  }
  .star {
    background: transparent;
    border: 1px solid hsl(var(--border));
    border-radius: var(--radius-s);
    color: hsl(var(--muted) / 0.5);
    width: 28px;
    height: 28px;
    cursor: pointer;
    font-size: 14px;
  }
  .star.on {
    color: hsl(var(--warn));
    border-color: hsl(var(--warn) / 0.4);
  }
  .ctl-select .scrim {
    position: fixed;
    inset: 0;
    background: transparent;
    border: none;
    z-index: 40;
  }
  .ctl-menu {
    position: absolute;
    top: 34px;
    right: 0;
    z-index: 50;
    width: 240px;
    background: hsl(var(--surface));
    border: 1px solid hsl(var(--border));
    border-radius: var(--radius);
    box-shadow: 0 12px 30px hsl(0 0% 0% / 0.5);
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ctl-search {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 6px 9px;
    font-size: 12px;
  }
  .ctl-search:focus {
    outline: none;
    border-color: hsl(var(--primary));
  }
  .ctl-list {
    max-height: 260px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .ctl-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: transparent;
    border: none;
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 7px 9px;
    font-size: 12px;
    cursor: pointer;
    text-align: left;
  }
  .ctl-item:hover {
    background: hsl(var(--foreground) / 0.06);
  }
  .ctl-item.active {
    background: hsl(var(--primary) / 0.16);
    color: hsl(var(--primary));
  }
  .ctl-item .def {
    color: hsl(var(--warn));
    font-size: 11px;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: none;
  }
  .tabs {
    display: flex;
    gap: 2px;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    border-radius: var(--radius-s);
    padding: 2px;
  }
  .tab {
    background: transparent;
    border: none;
    color: hsl(var(--muted));
    border-radius: 4px;
    padding: 4px 12px;
    font-size: 12px;
  }
  .tab.active {
    background: hsl(var(--primary) / 0.16);
    color: hsl(var(--primary));
  }
  .tab:disabled {
    opacity: 0.4;
  }
  .setsel {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 4px 8px;
    font-size: 11px;
    font-family: ui-monospace, monospace;
  }
  .mirror {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: hsl(var(--muted));
  }
  .dirty {
    font-size: 11.5px;
    color: hsl(var(--warn));
  }
  .save {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
    border: none;
    border-radius: var(--radius-s);
    padding: 6px 16px;
    font-size: 12.5px;
    font-weight: 700;
  }
  .save:disabled {
    opacity: 0.5;
  }
  .save-alt {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--muted));
    border-radius: var(--radius-s);
    padding: 6px 12px;
    font-size: 12px;
  }
  .save-alt:hover {
    border-color: hsl(var(--danger) / 0.5);
    color: hsl(var(--foreground));
  }
  .grid {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 0.8fr 0.8fr 1fr;
    gap: 12px;
  }
  .side {
    min-height: 0;
    min-width: 0;
  }
  .raw {
    flex: 1;
    min-height: 0;
    resize: none;
    background: hsl(var(--background) / 0.7);
    border: 1px solid hsl(var(--border));
    border-radius: var(--radius-s);
    color: hsl(var(--foreground));
    padding: 12px;
    font-family: ui-monospace, monospace;
    font-size: 12px;
    line-height: 1.5;
    white-space: pre;
    overflow: auto;
  }
  .raw:focus {
    outline: none;
    border-color: hsl(var(--primary));
  }
  .warn {
    font-size: 11.5px;
    color: hsl(var(--warn));
    flex: none;
  }
  .err {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 12px;
    background: hsl(var(--danger) / 0.16);
    border: 1px solid hsl(var(--danger) / 0.4);
    border-radius: var(--radius-s);
    font-size: 12px;
  }
  .err button {
    background: transparent;
    border: none;
    color: hsl(var(--muted));
  }
</style>
