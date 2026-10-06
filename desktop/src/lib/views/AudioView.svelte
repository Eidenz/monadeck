<script lang="ts">
  // The desktop's default output and microphone while VR runs: switched when
  // VR starts (or when the device shows up), handed back when it stops.
  import { onMount } from "svelte";
  import { app, saveConfig } from "$lib/state.svelte";
  import { audioDevices } from "$lib/api";
  import type { AudioDevice, AudioDevices } from "$lib/types";

  type Field = "vr_audio_output" | "vr_audio_input";

  let devices = $state<AudioDevices | null>(null);
  let loading = $state(false);

  async function load() {
    loading = true;
    try {
      devices = await audioDevices();
    } catch {
      devices = null;
    } finally {
      loading = false;
    }
  }
  onMount(load);

  function pick(field: Field, list: AudioDevice[], name: string) {
    if (!app.config) return;
    // A saved device that isn't plugged in stays as it was.
    const saved = app.config[field];
    app.config[field] = name ? (list.find((d) => d.name === name) ?? (saved?.name === name ? saved : null)) : null;
    saveConfig();
  }

  const fields: { field: Field; label: string; list: () => AudioDevice[]; current: () => string | null }[] = [
    {
      field: "vr_audio_output",
      label: "Output while VR runs",
      list: () => devices?.outputs ?? [],
      current: () => devices?.default_output ?? null,
    },
    {
      field: "vr_audio_input",
      label: "Microphone while VR runs",
      list: () => devices?.inputs ?? [],
      current: () => devices?.default_input ?? null,
    },
  ];
</script>

<section class="view">
  <div class="head">
    <h2>Audio</h2>
    <button class="small" onclick={load} disabled={loading}>{loading ? "…" : "Refresh"}</button>
  </div>

  {#if devices && !devices.available}
    <span class="note bad">
      No answer from <code>pactl</code>: switching needs PipeWire's PulseAudio server (or
      PulseAudio) and its <code>pactl</code> tool.
    </span>
  {/if}

  {#each fields as f (f.field)}
    {@const list = f.list()}
    {@const saved = app.config?.[f.field] ?? null}
    {@const missing = saved && !list.some((d) => d.name === saved.name)}
    <div class="field">
      <span class="lbl">{f.label}</span>
      <select
        aria-label={f.label}
        value={saved?.name ?? ""}
        disabled={!app.config}
        onchange={(e) => pick(f.field, list, (e.currentTarget as HTMLSelectElement).value)}
      >
        <option value="">Don't change</option>
        {#if missing && saved}
          <option value={saved.name}>{saved.description} (not plugged in)</option>
        {/if}
        {#each list as d (d.name)}
          <option value={d.name}>{d.description}{d.name === f.current() ? " (current default)" : ""}</option>
        {/each}
      </select>
    </div>
  {/each}

  <span class="note">
    Made the default as soon as VR starts and the device is there. The previous one comes back
    when VR stops, unless you picked another in the meantime.
  </span>
</section>

<style>
  .view {
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
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
  .note {
    font-size: 11.5px;
    color: hsl(var(--muted));
    line-height: 1.45;
  }
  .note.bad {
    color: hsl(var(--danger));
  }
  select {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 7px 8px;
    font-size: 12.5px;
    max-width: 460px;
  }
  select:focus {
    outline: none;
    border-color: hsl(var(--primary));
  }
  button {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 4px 10px;
    font-size: 11.5px;
  }
  button:disabled {
    opacity: 0.55;
  }
</style>
