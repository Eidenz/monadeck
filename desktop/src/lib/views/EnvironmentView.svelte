<script lang="ts">
  import { onMount } from "svelte";
  import {
    app,
    saveConfig,
    refreshPreflight,
    refreshImportOpenxr,
    applyImportOpenxr,
  } from "$lib/state.svelte";
  import { KNOWN_ENV_VARS, type KnownVar } from "$lib/knownEnvVars";
  import LaunchOptions from "$lib/components/LaunchOptions.svelte";

  // Editable rows in local state so an in-progress *empty* row can exist (the
  // config Record can't key on ""). Initialized from config once it's loaded;
  // only non-empty keys are written back.
  let pairs = $state<[string, string][]>([]);
  let initialized = false;
  $effect(() => {
    if (!initialized && app.config) {
      pairs = Object.entries(app.config.environment);
      initialized = true;
    }
  });

  // --- known-var picker ---
  let showKnown = $state(false);
  function addKnown(kv: KnownVar) {
    showKnown = false;
    // Replace an existing row with the same key, else append.
    const i = pairs.findIndex(([k]) => k === kv.name);
    if (i >= 0) pairs[i] = [kv.name, kv.value];
    else pairs = [...pairs, [kv.name, kv.value]];
    persist();
  }

  // --- Proton 11 / SLR import-openxr config file ---
  // Shared app state (so the deck's ProtonBanner popup and this card agree).
  onMount(() => {
    refreshImportOpenxr();
  });

  function persist() {
    if (!app.config) return;
    const env: Record<string, string> = {};
    for (const [k, v] of pairs) if (k.trim()) env[k.trim()] = v;
    app.config.environment = env;
    saveConfig();
  }
  function setKey(i: number, key: string) {
    pairs[i] = [key, pairs[i][1]];
    persist();
  }
  function setVal(i: number, val: string) {
    pairs[i] = [pairs[i][0], val];
    persist();
  }
  function addPair() {
    pairs = [...pairs, ["", ""]];
  }
  function removePair(i: number) {
    pairs = pairs.filter((_, idx) => idx !== i);
    persist();
  }
</script>

<section class="view">
  <h2>Environment</h2>

  <div class="field">
    <div class="env-head">
      <span class="lbl">Variables passed to <code>monado-service</code></span>
      <div class="head-btns">
        <button class="small" onclick={() => (showKnown = true)}>+ Known var</button>
        <button class="small" onclick={addPair}>+ Add</button>
      </div>
    </div>
    {#each pairs as [k, v], i (i)}
      <div class="row">
        <input class="key" value={k} placeholder="KEY" onchange={(e) => setKey(i, e.currentTarget.value)} />
        <input class="val" value={v} placeholder="value" onchange={(e) => setVal(i, e.currentTarget.value)} />
        <button class="del" onclick={() => removePair(i)}>✕</button>
      </div>
    {/each}
    {#if pairs.length === 0}
      <span class="note">No custom env vars. Add e.g. <code>MONADO_SCREENSHOT_DIR</code> or your pacing vars.</span>
    {/if}
  </div>

  <LaunchOptions />

  <div class="proton glass">
    <div class="proton-head">
      <div class="proton-meta">
        <div class="proton-title">Proton 11 / Steam Linux Runtime 4</div>
        <div class="proton-sub">
          Newer Proton needs <code>PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES</code>
          set session-wide, not just per game.
        </div>
      </div>
      {#if app.importOpenxr}
        <span class="pill good">set ✓</span>
      {:else}
        <button class="accent" onclick={applyImportOpenxr} disabled={app.applyingProton}>
          {app.applyingProton ? "…" : "Create config file"}
        </button>
      {/if}
    </div>
    {#if !app.importOpenxr}
      <div class="proton-note">
        Writes <code>~/.config/environment.d/…conf</code> — takes effect after a reboot.
      </div>
    {/if}
    {#if app.protonResult}
      <div class="proton-note" class:bad={!app.protonResult.ok}>{app.protonResult.msg}</div>
    {/if}
  </div>

  <div class="readiness glass">
    <div class="rd-head">
      <div class="rd-meta">
        <div class="rd-title">System readiness</div>
        <div class="rd-sub">
          Prerequisites outside Monadeck's control{app.preflight?.distro
            ? ` · ${app.preflight.distro}`
            : ""}. Install commands are best-effort hints.
        </div>
      </div>
      <button class="small" onclick={refreshPreflight}>Re-check</button>
    </div>

    {#each app.preflight?.checks ?? [] as c (c.id)}
      <div class="check">
        <div class="check-row">
          <div class="check-text">
            <div class="check-title">{c.label}</div>
            <div class="check-detail">{c.detail}</div>
          </div>
          <span
            class="pill"
            class:good={c.ok}
            class:warn={!c.ok && c.severity === "important"}
          >
            {c.ok ? "ok ✓" : c.severity === "important" ? "missing" : "optional"}
          </span>
        </div>
        {#if !c.ok && c.fix}
          <code class="fix">{c.fix}</code>
        {/if}
      </div>
    {/each}

    {#if app.preflight && app.preflight.all_ok}
      <div class="rd-allok">Everything's in place. ✓</div>
    {/if}
  </div>
</section>

{#if showKnown}
  <button class="scrim" aria-label="Close" onclick={() => (showKnown = false)}></button>
  <div class="picker glass" role="dialog" aria-label="Known monado env vars">
    <div class="picker-list">
      {#each KNOWN_ENV_VARS as kv (kv.name)}
        <button class="kv state-layer" onclick={() => addKnown(kv)}>
          <span class="kv-name">{kv.name}</span>
          <span class="kv-desc">{kv.desc}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .view {
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .env-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .lbl {
    font-size: 12.5px;
    font-weight: 600;
  }
  code {
    background: hsl(var(--background) / 0.6);
    padding: 0 4px;
    border-radius: 4px;
    font-size: 11.5px;
    font-family: ui-monospace, monospace;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  input {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 7px 10px;
    font-size: 12.5px;
    font-family: ui-monospace, monospace;
    min-width: 0;
  }
  .key {
    flex: 0 0 220px;
  }
  .val {
    flex: 1;
  }
  input:focus {
    outline: none;
    border-color: hsl(var(--primary));
  }
  .small {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 4px 10px;
    font-size: 12px;
  }
  .del {
    flex: none;
    width: 34px;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
  }
  .del:hover {
    border-color: hsl(var(--danger));
    color: hsl(var(--danger));
  }
  .note {
    font-size: 11.5px;
    color: hsl(var(--muted));
  }
  .head-btns {
    display: flex;
    gap: 6px;
  }

  .proton {
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .proton-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .proton-title {
    font-size: 13px;
    font-weight: 600;
  }
  .proton-sub,
  .proton-note {
    font-size: 11.5px;
    color: hsl(var(--muted));
    line-height: 1.45;
    margin-top: 2px;
  }
  .proton-note.bad {
    color: hsl(var(--danger));
  }
  .pill {
    flex: none;
    font-size: 12px;
    font-weight: 600;
    padding: 5px 12px;
    border-radius: 99px;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--muted));
  }
  .pill.good {
    color: hsl(var(--ok));
    border-color: hsl(var(--ok) / 0.4);
  }
  .accent {
    flex: none;
    background: hsl(var(--primary));
    border: none;
    color: hsl(var(--primary-fg));
    border-radius: var(--radius-s);
    padding: 7px 12px;
    font-size: 12.5px;
    font-weight: 700;
  }
  .accent:disabled {
    opacity: 0.6;
  }

  .readiness {
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .rd-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .rd-title {
    font-size: 13px;
    font-weight: 600;
  }
  .rd-sub {
    font-size: 11.5px;
    color: hsl(var(--muted));
    line-height: 1.45;
    margin-top: 2px;
  }
  .check {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 10px;
    border-top: 1px solid hsl(var(--border) / 0.5);
  }
  .check-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .check-title {
    font-size: 12.5px;
    font-weight: 600;
  }
  .check-detail {
    font-size: 11.5px;
    color: hsl(var(--muted));
    line-height: 1.45;
    margin-top: 2px;
  }
  .fix {
    display: block;
    background: hsl(var(--background) / 0.7);
    border: 1px solid hsl(var(--border));
    border-radius: var(--radius-s);
    padding: 7px 10px;
    font-size: 11.5px;
    font-family: ui-monospace, monospace;
    color: hsl(var(--foreground));
    user-select: all;
    word-break: break-all;
  }
  .pill.warn {
    color: hsl(var(--warn));
    border-color: hsl(var(--warn) / 0.4);
  }
  .rd-allok {
    font-size: 11.5px;
    color: hsl(var(--ok));
  }

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
    width: min(480px, 92vw);
    max-height: 72vh;
    padding: 8px;
    /* opaque so the list is easy to read over whatever's behind it */
    background: hsl(var(--surface));
    box-shadow: 0 18px 50px hsl(0 0% 0% / 0.55);
  }
  .picker-list {
    overflow-y: auto;
    max-height: calc(72vh - 16px);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .kv {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    background: transparent;
    border: none;
    border-radius: var(--radius-s);
    padding: 9px 11px;
    text-align: left;
  }
  .kv-name {
    font-size: 12.5px;
    font-family: ui-monospace, monospace;
    color: hsl(var(--primary));
  }
  .kv-desc {
    font-size: 11.5px;
    color: hsl(var(--muted));
    line-height: 1.4;
  }
</style>
