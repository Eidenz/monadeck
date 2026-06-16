<script lang="ts">
  import { app, saveConfig } from "$lib/state.svelte";
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
      <button class="small" onclick={addPair}>+ Add</button>
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
</section>

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
</style>
