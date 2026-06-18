<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    app,
    saveConfig,
    applyCaps,
    installMonado,
    installXrizer,
    installChihuahua,
  } from "$lib/state.svelte";
  import Toggle from "$lib/components/Toggle.svelte";

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
    <div class="install">
      <button
        class:accent={app.caps === "no_binary"}
        onclick={installMonado}
        disabled={app.installing !== ""}
      >
        {app.installing === "monado"
          ? "Downloading & installing…"
          : "Install built-in Monado (latest)"}
      </button>
      <span class="install-hint">Downloads our prebuilt fork and points the prefix at it — no compiling.</span>
    </div>
    {#if app.installResult?.kind === "monado"}
      <span class="install-ok" class:bad={!app.installResult.ok}>{app.installResult.msg}</span>
    {/if}
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
    <div class="install">
      <button
        onclick={installXrizer}
        disabled={app.installing !== ""}
      >
        {app.installing === "xrizer"
          ? "Downloading & installing…"
          : "Install built-in xrizer (latest)"}
      </button>
      <span class="install-hint">Downloads the latest xrizer release and registers it as the OpenVR runtime.</span>
    </div>
    {#if app.installResult?.kind === "xrizer"}
      <span class="install-ok" class:bad={!app.installResult.ok}>{app.installResult.msg}</span>
    {/if}
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
    <span class="lbl">Behavior</span>
    <div class="toggle-row">
      <Toggle
        label="Minimize to tray on close"
        checked={app.config?.minimize_to_tray ?? true}
        onchange={(v) => {
          if (app.config) {
            app.config.minimize_to_tray = v;
            saveConfig();
          }
        }}
      />
      <span>Minimize to tray on close <em>(uncheck to quit instead)</em></span>
    </div>
    <div class="toggle-row">
      <Toggle
        label="Start the service when Monadeck launches"
        checked={app.config?.auto_start ?? false}
        onchange={(v) => {
          if (app.config) {
            app.config.auto_start = v;
            saveConfig();
          }
        }}
      />
      <span>Start the service when Monadeck launches</span>
    </div>
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

  <div class="field">
    <span class="lbl">VR Mods (UEVR)</span>
    <div class="row">
      <span
        class="pill"
        class:good={app.uevr.protontricks}
        class:warn={!app.uevr.protontricks}
      >
        protontricks {app.uevr.protontricks ? "✓" : "missing"}
      </span>
      <span
        class="pill"
        class:good={!!app.uevr.chihuahua}
        class:warn={!app.uevr.chihuahua}
      >
        chihuahua {app.uevr.chihuahua ? "✓" : "not installed"}
      </span>
    </div>
    {#if !app.uevr.protontricks}
      <span class="note bad"
        >Install <code>protontricks</code> to launch flat Unreal Engine games in VR.
        The in-headset VR-Mod toggle stays hidden until it's present.</span
      >
    {/if}
    {#if app.uevr.chihuahua}
      <span class="note">{app.uevr.chihuahua}</span>
    {/if}
    <div class="install">
      <button
        onclick={() => installChihuahua(!!app.uevr.chihuahua)}
        disabled={app.installingChihuahua}
      >
        {app.installingChihuahua
          ? "Downloading…"
          : app.uevr.chihuahua
            ? "Re-download chihuahua"
            : "Install chihuahua"}
      </button>
      <span class="install-hint"
        >The headless UEVR injector. Downloaded automatically on the first VR-Mod
        launch — fetch it here ahead of time.</span
      >
    </div>
    {#if app.chihuahuaResult}
      <span class="install-ok" class:bad={!app.chihuahuaResult.ok}
        >{app.chihuahuaResult.msg}</span
      >
    {/if}
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
  .install {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 4px;
    flex-wrap: wrap;
  }
  .install-hint {
    font-size: 11px;
    color: hsl(var(--muted));
  }
  .install-ok {
    font-size: 11.5px;
    color: hsl(var(--ok));
    margin-top: 2px;
  }
  .install-ok.bad {
    color: hsl(var(--danger));
  }
  button:disabled {
    opacity: 0.55;
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
  .toggle-row {
    display: flex;
    align-items: center;
    gap: 11px;
    font-size: 13px;
    color: hsl(var(--foreground));
  }
  .toggle-row em {
    color: hsl(var(--muted));
    font-style: normal;
  }
</style>
