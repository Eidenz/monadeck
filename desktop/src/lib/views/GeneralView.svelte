<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { app, saveConfig, applyCaps } from "$lib/state.svelte";

  async function browse(field: "monado_prefix" | "xrizer_path") {
    if (!app.config) return;
    const picked = await open({ directory: true, multiple: false });
    if (typeof picked === "string") {
      app.config[field] = picked;
      await saveConfig();
    }
  }

  const capLabel: Record<string, string> = {
    set: "Set ✓",
    needs_setcap: "Not set",
    no_binary: "No service binary",
    no_tooling: "getcap/setcap missing",
  };
</script>

<section class="view">
  <h2>General</h2>

  <div class="field">
    <span class="lbl">Monado build prefix</span>
    <div class="row">
      <input
        value={app.config?.monado_prefix ?? ""}
        placeholder="/usr"
        onchange={(e) => {
          if (app.config) {
            app.config.monado_prefix = e.currentTarget.value;
            saveConfig();
          }
        }}
      />
      <button onclick={() => browse("monado_prefix")}>Browse…</button>
    </div>
    <span class="note" class:bad={app.caps === "no_binary"}>
      {app.caps === "no_binary"
        ? "monado-service not found under this prefix."
        : `${app.config?.monado_prefix || "—"}/bin/monado-service`}
    </span>
  </div>

  <div class="field">
    <span class="lbl">xrizer runtime path</span>
    <div class="row">
      <input
        value={app.config?.xrizer_path ?? ""}
        placeholder="~/.local/share/xrizer/xrizer-nightly"
        onchange={(e) => {
          if (app.config) {
            app.config.xrizer_path = e.currentTarget.value || null;
            saveConfig();
          }
        }}
      />
      <button onclick={() => browse("xrizer_path")}>Browse…</button>
    </div>
  </div>

  <div class="field">
    <span class="lbl">OpenVR compatibility</span>
    <div class="seg">
      <button
        class:active={app.config?.ovr_runtime === "xrizer"}
        onclick={() => {
          if (app.config) {
            app.config.ovr_runtime = "xrizer";
            saveConfig();
          }
        }}>Register xrizer</button>
      <button
        class:active={app.config?.ovr_runtime === "none"}
        onclick={() => {
          if (app.config) {
            app.config.ovr_runtime = "none";
            saveConfig();
          }
        }}>Leave alone</button>
    </div>
    <span class="note">Starting backs up your existing runtime files and restores them on stop.</span>
  </div>

  <div class="field">
    <span class="lbl">Lighthouse driver</span>
    <div class="seg">
      {#each ["steamvr", "vive", "survive"] as d (d)}
        <button
          class:active={app.config?.lighthouse_driver === d}
          onclick={() => {
            if (app.config) {
              app.config.lighthouse_driver = d;
              saveConfig();
            }
          }}>{d}</button>
      {/each}
    </div>
    <span class="note">steamvr (default) drives the Bigscreen Beyond via monado's SteamVR wrapper. vive/survive are the FLOSS drivers for Vive/Index.</span>
  </div>

  <div class="field">
    <span class="lbl">Service capabilities (CAP_SYS_NICE)</span>
    <div class="row">
      <span class="pill" class:good={app.caps === "set"} class:warn={app.caps === "needs_setcap"}>
        {capLabel[app.caps] ?? app.caps}
      </span>
      {#if app.caps === "needs_setcap"}
        <button class="accent" onclick={applyCaps} disabled={app.busy}>
          {app.busy ? "Applying…" : "Set capabilities"}
        </button>
      {/if}
    </div>
  </div>
</section>

<style>
  .view {
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  h2 {
    margin: 0 0 2px;
    font-size: 17px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .lbl {
    font-size: 12.5px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  input {
    flex: 1;
    min-width: 0;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 7px 10px;
    font-size: 12.5px;
    font-family: ui-monospace, monospace;
  }
  input:focus {
    outline: none;
    border-color: hsl(var(--primary));
  }
  button {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 7px 12px;
    font-size: 12.5px;
    flex: none;
  }
  button.accent {
    background: hsl(var(--primary));
    border-color: transparent;
    color: hsl(var(--primary-fg));
    font-weight: 700;
  }
  .note {
    font-size: 11px;
    color: hsl(var(--muted));
    font-family: ui-monospace, monospace;
    word-break: break-all;
  }
  .note.bad {
    color: hsl(var(--danger));
  }
  .seg {
    display: flex;
    gap: 6px;
  }
  .seg button.active {
    border-color: hsl(var(--primary));
    color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.12);
  }
  .pill {
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
  .pill.warn {
    color: hsl(var(--warn));
    border-color: hsl(var(--warn) / 0.4);
  }
</style>
