<script lang="ts">
  import { onMount } from "svelte";
  import { app, saveConfig } from "$lib/state.svelte";
  import { amdGpu, hasNvidia, setAmdVrProfile } from "$lib/api";
  import Toggle from "$lib/components/Toggle.svelte";
  import type { AmdGpu } from "$lib/types";

  const PRESETS = [100, 120, 140, 170];

  function setScale(v: number) {
    if (!app.config) return;
    app.config.render_scale = Math.max(50, Math.min(300, Math.round(v) || 100));
    saveConfig();
  }
  const set = (
    k: "min_frame_period" | "compute_compositor" | "debug_gui" | "nvidia_mitigation",
    v: boolean,
  ) => {
    if (app.config) {
      app.config[k] = v;
      saveConfig();
    }
  };

  let gpu = $state<AmdGpu | null>(null);
  let nvidia = $state(false);
  let applying = $state(false);
  async function loadGpu() {
    gpu = await amdGpu();
    nvidia = await hasNvidia();
  }
  async function applyVr() {
    applying = true;
    try {
      await setAmdVrProfile();
      await loadGpu();
    } finally {
      applying = false;
    }
  }
  onMount(loadGpu);
</script>

<section class="view">
  <h2>Compositor</h2>

  <div class="field">
    <span class="lbl">Render scale</span>
    <div class="scale">
      <div class="seg">
        {#each PRESETS as p (p)}
          <button class:active={app.config?.render_scale === p} onclick={() => setScale(p)}>{p}%</button>
        {/each}
      </div>
      <input
        class="num"
        type="number"
        min="50"
        max="300"
        value={app.config?.render_scale ?? 140}
        onchange={(e) => setScale(parseInt(e.currentTarget.value, 10))}
      />
    </div>
    <span class="note">100% = native. Higher supersamples for a sharper image at a GPU cost — Envision defaults to 140.</span>
  </div>

  <div class="field">
    <span class="lbl">Options</span>
    <div class="toggle-row">
      <Toggle
        label="Unlock frame period"
        checked={app.config?.min_frame_period ?? true}
        onchange={(v) => set("min_frame_period", v)}
      />
      <span>Unlock min frame period <em>— usually a perf boost (U_PACING)</em></span>
    </div>
    <div class="toggle-row">
      <Toggle
        label="Compute compositor"
        checked={app.config?.compute_compositor ?? true}
        onchange={(v) => set("compute_compositor", v)}
      />
      <span>GPU compute compositor <em>(XRT_COMPOSITOR_COMPUTE)</em></span>
    </div>
    <div class="toggle-row">
      <Toggle
        label="Monado debug window"
        checked={app.config?.debug_gui ?? false}
        onchange={(v) => set("debug_gui", v)}
      />
      <span>Monado debug/preview window <em>(desktop mirror)</em></span>
    </div>
    {#if nvidia}
      <div class="toggle-row">
        <Toggle
          label="NVIDIA mitigations"
          checked={app.config?.nvidia_mitigation ?? true}
          onchange={(v) => set("nvidia_mitigation", v)}
        />
        <span>NVIDIA compositor mitigations <em>(present-wait + pacing fraction — NVIDIA GPU detected)</em></span>
      </div>
    {/if}
  </div>

  {#if gpu}
    <div class="field">
      <span class="lbl">AMD GPU power profile <span class="card">({gpu.card})</span></span>
      <div class="row">
        <span class="pill" class:good={gpu.vr_active}>
          {gpu.current_mode || "unknown"}{gpu.vr_active ? " ✓" : ""}
        </span>
        {#if !gpu.vr_active}
          <button class="accent" onclick={applyVr} disabled={applying}>
            {applying ? "Applying…" : "Set VR profile"}
          </button>
        {/if}
      </div>
      <span class="note">Pins GPU clocks for VR (less frame-time jitter). Needs a password (pkexec) and resets on reboot.</span>
    </div>
  {/if}
</section>

<style>
  .view {
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .lbl {
    font-size: 12.5px;
    font-weight: 600;
  }
  .card {
    color: hsl(var(--muted));
    font-weight: 400;
  }
  .scale {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .seg {
    display: flex;
    gap: 6px;
  }
  .seg button {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 6px 11px;
    font-size: 12.5px;
  }
  .seg button.active {
    border-color: hsl(var(--primary));
    color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.12);
  }
  .num {
    width: 72px;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 6px 8px;
    font-size: 12.5px;
  }
  .num:focus {
    outline: none;
    border-color: hsl(var(--primary));
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
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
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
  .accent {
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
  .note {
    font-size: 11px;
    color: hsl(var(--muted));
  }
</style>
