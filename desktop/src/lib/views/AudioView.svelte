<script lang="ts">
  // The desktop's default output and microphone while VR runs: switched when
  // VR starts (or when the device shows up), handed back when it stops.
  import { onMount } from "svelte";
  import { app, saveConfig } from "$lib/state.svelte";
  import { audioDevices } from "$lib/api";
  import Select from "$lib/components/Select.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import type { AudioDevice, AudioDevices, SelectOption } from "$lib/types";

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

  function choices(list: AudioDevice[], saved: AudioDevice | null, current: string | null): SelectOption[] {
    const out: SelectOption[] = [{ value: "", label: "Don't change" }];
    if (saved && !list.some((d) => d.name === saved.name)) {
      out.push({ value: saved.name, label: `${saved.description} (not plugged in)` });
    }
    for (const d of list) {
      out.push({ value: d.name, label: d.name === current ? `${d.description} (current default)` : d.description });
    }
    return out;
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
    {#if app.config?.backend !== "wivrn"}
      <button class="small" onclick={load} disabled={loading}>{loading ? "…" : "Refresh"}</button>
    {/if}
  </div>

  {#if devices && !devices.available}
    <span class="note bad">
      No answer from <code>pactl</code>: switching needs PipeWire's PulseAudio server (or
      PulseAudio) and its <code>pactl</code> tool.
    </span>
  {/if}

  {#if app.config?.backend === "wivrn"}
    <div class="toggle-row">
      <Toggle
        label="Use the headset's speakers and microphone"
        checked={app.config?.vr_audio_auto ?? true}
        onchange={(v) => {
          if (app.config) {
            app.config.vr_audio_auto = v;
            saveConfig();
          }
        }}
      />
      <span>Use the headset's speakers and microphone</span>
    </div>
    <span class="note">
      WiVRn adds them while the headset is connected, and they become the default each time it
      connects. The previous ones come back when VR stops.
    </span>
  {:else}
  {#each fields as f (f.field)}
    {@const list = f.list()}
    {@const saved = app.config?.[f.field] ?? null}
    <div class="field">
      <span class="lbl">{f.label}</span>
      <div class="pick">
        <Select
          wide
          label={f.label}
          value={saved?.name ?? ""}
          disabled={!app.config}
          options={choices(list, saved, f.current())}
          onchange={(v) => pick(f.field, list, v)}
        />
      </div>
    </div>
  {/each}

  <span class="note">
    Made the default as soon as VR starts and the device is there. The previous one comes back
    when VR stops, unless you picked another in the meantime.
  </span>
  {/if}
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
  .pick {
    max-width: 460px;
  }
  .toggle-row {
    display: flex;
    align-items: center;
    gap: 11px;
    font-size: 13px;
    color: hsl(var(--foreground));
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
