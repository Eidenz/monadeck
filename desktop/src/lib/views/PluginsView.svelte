<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { app, saveConfig } from "$lib/state.svelte";
  import { launchPlugin, listInstalledApps } from "$lib/api";
  import type { ExecWhen, InstalledApp, Plugin } from "$lib/types";

  // --- installed-app picker -------------------------------------------------
  let picking = $state(false);
  let apps = $state<InstalledApp[]>([]);
  let query = $state("");
  let loading = $state(false);

  const filtered = $derived(
    query.trim()
      ? apps.filter((a) => a.name.toLowerCase().includes(query.toLowerCase()))
      : apps,
  );

  async function openPicker() {
    picking = true;
    query = "";
    if (apps.length === 0) {
      loading = true;
      try {
        apps = await listInstalledApps();
      } finally {
        loading = false;
      }
    }
  }

  async function addInstalled(a: InstalledApp) {
    picking = false;
    await addPlugin(a.name, a.path);
  }

  async function addExecutable() {
    const picked = await open({
      multiple: false,
      directory: false,
      title: "Pick an executable to launch alongside Monado",
    });
    if (typeof picked !== "string") return;
    await addPlugin(picked.split("/").pop() || "plugin", picked);
  }

  async function addPlugin(name: string, path: string) {
    if (!app.config) return;
    const plugin: Plugin = {
      name,
      path,
      args: [],
      when: "after-start",
      enabled: true,
    };
    app.config.plugins = [...app.config.plugins, plugin];
    await saveConfig();
  }

  // --- existing plugin editing ---------------------------------------------
  async function remove(i: number) {
    if (!app.config) return;
    app.config.plugins = app.config.plugins.filter((_, idx) => idx !== i);
    await saveConfig();
  }
  async function update(i: number, patch: Partial<Plugin>) {
    if (!app.config) return;
    app.config.plugins = app.config.plugins.map((p, idx) =>
      idx === i ? { ...p, ...patch } : p,
    );
    await saveConfig();
  }
  const argsToString = (args: string[]) => args.join(" ");
  const stringToArgs = (s: string) => (s.trim() ? s.trim().split(/\s+/) : []);

  const isDesktop = (path: string) => path.endsWith(".desktop");
  const sourceLabel = (path: string) =>
    isDesktop(path)
      ? `installed app · ${path.split("/").pop()?.replace(/\.desktop$/, "")}`
      : path;
</script>

<section class="plugins">
  <div class="head">
    <p>Launch apps alongside the service — pick an <b>installed app</b> or any <b>executable</b>.</p>
    <div class="add-btns">
      <button class="add" onclick={openPicker}>+ Installed app</button>
      <button class="add ghost" onclick={addExecutable}>+ Executable</button>
    </div>
  </div>

  {#if !app.config || app.config.plugins.length === 0}
    <p class="empty">No plugins yet.</p>
  {:else}
    <div class="list">
      {#each app.config.plugins as p, i (i)}
        <div class="row glass" class:off={!p.enabled}>
          <label class="en">
            <input
              type="checkbox"
              checked={p.enabled}
              onchange={(e) => update(i, { enabled: e.currentTarget.checked })}
            />
          </label>
          <div class="fields">
            <input
              class="name"
              value={p.name}
              onchange={(e) => update(i, { name: e.currentTarget.value })}
            />
            <div class="path" class:app={isDesktop(p.path)} title={p.path}>{sourceLabel(p.path)}</div>
            <div class="opts">
              <select
                value={p.when}
                onchange={(e) => update(i, { when: e.currentTarget.value as ExecWhen })}
              >
                <option value="after-start">after start</option>
                <option value="after-stop">after stop</option>
              </select>
              <input
                class="args"
                placeholder="extra args…"
                value={argsToString(p.args)}
                onchange={(e) => update(i, { args: stringToArgs(e.currentTarget.value) })}
              />
            </div>
          </div>
          <div class="row-acts">
            <button class="launch" title="Launch now" onclick={() => launchPlugin(i)}>▶</button>
            <button class="del" title="Remove" onclick={() => remove(i)}>✕</button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>

{#if picking}
  <button class="scrim" aria-label="Close picker" onclick={() => (picking = false)}></button>
  <div class="picker glass" role="dialog" aria-label="Choose an installed app">
    <input
      class="search"
      placeholder="Search installed apps…"
      bind:value={query}
    />
    <div class="results">
      {#if loading}
        <div class="msg">Scanning installed apps…</div>
      {:else if filtered.length === 0}
        <div class="msg">No matching apps.</div>
      {:else}
        {#each filtered as a (a.path)}
          <button class="result state-layer" onclick={() => addInstalled(a)}>
            <span class="r-name">{a.name}</span>
            <span class="r-id">{a.path.split("/").pop()}</span>
          </button>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .plugins {
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .head p {
    margin: 0;
    font-size: 12.5px;
    color: hsl(var(--muted));
  }
  .add-btns {
    display: flex;
    gap: 6px;
    flex: none;
  }
  .add {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
    border: none;
    border-radius: var(--radius-s);
    padding: 7px 12px;
    font-size: 12.5px;
    font-weight: 700;
  }
  .add.ghost {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
  }
  .row.off {
    opacity: 0.55;
  }
  .fields {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .name {
    background: transparent;
    border: none;
    border-bottom: 1px solid transparent;
    color: hsl(var(--foreground));
    font-size: 13.5px;
    font-weight: 600;
    padding: 2px 0;
  }
  .name:focus {
    outline: none;
    border-bottom-color: hsl(var(--primary));
  }
  .path {
    font-size: 11px;
    color: hsl(var(--muted));
    font-family: ui-monospace, monospace;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .path.app {
    color: hsl(var(--primary) / 0.9);
    font-family: inherit;
  }
  .opts {
    display: flex;
    gap: 8px;
  }
  select,
  .args {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 4px 8px;
    font-size: 12px;
  }
  .args {
    flex: 1;
    min-width: 0;
    font-family: ui-monospace, monospace;
  }
  .row-acts {
    display: flex;
    gap: 4px;
  }
  .launch,
  .del {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    width: 28px;
    height: 28px;
  }
  .launch:hover {
    border-color: hsl(var(--primary));
    color: hsl(var(--primary));
  }
  .del:hover {
    border-color: hsl(var(--danger));
    color: hsl(var(--danger));
  }
  .empty {
    font-size: 12.5px;
    color: hsl(var(--muted));
  }

  /* picker overlay */
  .scrim {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.45);
    border: none;
    z-index: 40;
  }
  .picker {
    position: fixed;
    z-index: 50;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(440px, 90vw);
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    padding: 12px;
    gap: 10px;
    background: hsl(var(--surface));
    box-shadow: 0 18px 50px hsl(0 0% 0% / 0.55);
  }
  .search {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 8px 11px;
    font-size: 13px;
  }
  .search:focus {
    outline: none;
    border-color: hsl(var(--primary));
  }
  .results {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .result {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    background: transparent;
    border: none;
    border-radius: var(--radius-s);
    padding: 8px 10px;
    text-align: left;
  }
  .r-name {
    font-size: 13px;
    color: hsl(var(--foreground));
  }
  .r-id {
    font-size: 10.5px;
    color: hsl(var(--muted));
    font-family: ui-monospace, monospace;
  }
  .msg {
    padding: 14px;
    text-align: center;
    color: hsl(var(--muted));
    font-size: 12.5px;
  }
</style>
