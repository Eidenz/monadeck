<script lang="ts">
  import {
    buildInputPath,
    getControllerInputFromPath,
    getInputsForHand,
  } from "./data/controllers";
  import {
    getAvailableModesForInput,
    getInputSubTypesForMode,
    type SourceEntry,
    type SourceMode,
  } from "./types";
  import {
    editor,
    activeProfile,
    activeBindingSources,
    sourcesForInput,
    inputActions,
    localizeAction,
    setSelectedInput,
    setHoveredInput,
    addSource,
    updateSource,
    removeSource,
    addHaptic,
    removeHaptic,
    outputActions,
    activeHaptics,
  } from "./store.svelte";

  let { hand }: { hand: "left" | "right" } = $props();

  const onThisHand = $derived(
    !!editor.selectedInput?.includes(`/hand/${hand}/`),
  );
  const profile = $derived(activeProfile());
  const prefix = $derived(`/user/hand/${hand}/`);
  const handSources = $derived(
    activeBindingSources().filter((s) => s.path.startsWith(prefix)),
  );
  const handInputs = $derived(getInputsForHand(profile, hand));

  // selected-input editor state
  const inputDef = $derived(
    editor.selectedInput
      ? getControllerInputFromPath(profile, editor.selectedInput)
      : null,
  );
  const selSources = $derived(
    editor.selectedInput ? sourcesForInput(editor.selectedInput) : [],
  );
  const actions = $derived(inputActions());

  function bind() {
    if (!editor.activeActionSet || !editor.selectedInput) return;
    addSource(editor.activeActionSet, {
      path: editor.selectedInput,
      mode: inputDef?.defaultModes[0] ?? "button",
      inputs: {},
    });
  }
  function changeMode(i: number, src: SourceEntry, mode: SourceMode) {
    if (editor.activeActionSet)
      updateSource(editor.activeActionSet, i, { ...src, mode, inputs: {} });
  }
  function changeSub(i: number, src: SourceEntry, sub: string, action: string) {
    const inputs = { ...src.inputs };
    if (action === "") delete inputs[sub];
    else inputs[sub] = { output: action };
    if (editor.activeActionSet)
      updateSource(editor.activeActionSet, i, { ...src, inputs });
  }

  // --- per-source parameters (deadzone, thresholds, …) ----------------------
  const KNOWN_PARAMS: Record<string, { label: string; placeholder: string }[]> = {
    trigger: [
      { label: "click_activate_threshold", placeholder: "0.5" },
      { label: "click_deactivate_threshold", placeholder: "0.45" },
    ],
    joystick: [{ label: "deadzone_pct", placeholder: "20" }],
    dpad: [
      { label: "deadzone_pct", placeholder: "50" },
      { label: "overlap_pct", placeholder: "0" },
      { label: "sub_mode", placeholder: "click" },
      { label: "sticky", placeholder: "false" },
    ],
    trackpad: [{ label: "deadzone_pct", placeholder: "20" }],
    force_sensor: [{ label: "threshold", placeholder: "0.5" }],
  };
  let paramsOpen = $state<Record<number, boolean>>({});
  let newParam = $state<Record<number, string>>({});

  function changeParam(i: number, src: SourceEntry, key: string, value: string) {
    const params: Record<string, unknown> = { ...(src.parameters ?? {}) };
    if (value === "") delete params[key];
    else {
      const n = parseFloat(value);
      params[key] = isNaN(n) ? value : n;
    }
    if (editor.activeActionSet)
      updateSource(editor.activeActionSet, i, { ...src, parameters: params });
  }
  function customParams(src: SourceEntry): string[] {
    const known = (KNOWN_PARAMS[src.mode] ?? []).map((p) => p.label);
    return Object.keys(src.parameters ?? {}).filter((k) => !known.includes(k));
  }

  // --- haptics (per hand: output/haptic → a vibration action) ---------------
  const hapticPath = $derived(`/user/hand/${hand}/${profile.hapticPath}`);
  const outputs = $derived(outputActions());
  const currentHaptic = $derived(
    activeHaptics().find((h) => h.path === hapticPath)?.output ?? "",
  );
  function setHaptic(action: string) {
    if (!editor.activeActionSet) return;
    const idx = activeHaptics().findIndex((h) => h.path === hapticPath);
    if (idx !== -1) removeHaptic(editor.activeActionSet, idx);
    if (action !== "") addHaptic(editor.activeActionSet, { output: action, path: hapticPath });
  }
</script>

<div class="panel">
  {#if onThisHand && editor.selectedInput}
    <!-- Input editor -->
    {@const label = inputDef?.label ?? editor.selectedInput.split("/").pop()}
    {@const inputType = inputDef?.type ?? "button"}
    <div class="ed-head">
      <button class="back" title="Back (Esc)" onclick={() => setSelectedInput(null)}>←</button>
      <div class="ed-title">
        <div class="lbl">{label}</div>
        {#if editor.mirrorMode}<div class="mirror">Mirror — both hands</div>{/if}
      </div>
      <button class="bind" onclick={bind}>+ Bind</button>
    </div>

    {#if selSources.length === 0}
      <div class="empty">No bindings — click <b>Bind</b> to add one.</div>
    {:else}
      <div class="sources">
        {#each selSources as { source, globalIndex } (globalIndex)}
          <div class="src">
            <div class="src-head">
              <span class="src-n">#{globalIndex + 1}</span>
              <button class="del" title="Remove" onclick={() => editor.activeActionSet && removeSource(editor.activeActionSet, globalIndex)}>✕</button>
            </div>
            <div class="row">
              <span class="k">Mode</span>
              <select value={source.mode} onchange={(e) => changeMode(globalIndex, source, e.currentTarget.value as SourceMode)}>
                {#each getAvailableModesForInput(inputType) as m (m)}<option value={m}>{m}</option>{/each}
                {#if !getAvailableModesForInput(inputType).includes(source.mode)}<option value={source.mode}>{source.mode}</option>{/if}
              </select>
            </div>
            {#each getInputSubTypesForMode(source.mode) as sub (sub)}
              {@const cur = source.inputs[sub]?.output ?? ""}
              <div class="row">
                <span class="k mono">{sub}</span>
                <select value={cur} onchange={(e) => changeSub(globalIndex, source, sub, e.currentTarget.value)}>
                  <option value="">— none —</option>
                  {#each actions as a (a)}<option value={a}>{localizeAction(a)}</option>{/each}
                  {#if cur && !actions.includes(cur)}<option value={cur}>{localizeAction(cur)}</option>{/if}
                </select>
              </div>
            {/each}

            <button class="param-toggle" onclick={() => (paramsOpen[globalIndex] = !paramsOpen[globalIndex])}>
              {paramsOpen[globalIndex] ? "▾" : "▸"} Parameters
            </button>
            {#if paramsOpen[globalIndex]}
              <div class="params">
                {#each KNOWN_PARAMS[source.mode] ?? [] as p (p.label)}
                  <div class="prow">
                    <span class="pk" title={p.label}>{p.label}</span>
                    <input value={String(source.parameters?.[p.label] ?? "")} placeholder={p.placeholder} onchange={(e) => changeParam(globalIndex, source, p.label, e.currentTarget.value)} />
                  </div>
                {/each}
                {#each customParams(source) as k (k)}
                  <div class="prow">
                    <span class="pk" title={k}>{k}</span>
                    <input value={String(source.parameters?.[k] ?? "")} onchange={(e) => changeParam(globalIndex, source, k, e.currentTarget.value)} />
                    <button class="pdel" title="Remove" onclick={() => changeParam(globalIndex, source, k, "")}>✕</button>
                  </div>
                {/each}
                <div class="prow">
                  <input class="pk-new" placeholder="custom_param" value={newParam[globalIndex] ?? ""} oninput={(e) => (newParam[globalIndex] = e.currentTarget.value)} />
                  <button class="padd" title="Add" onclick={() => { const k = (newParam[globalIndex] ?? "").trim(); if (k) { changeParam(globalIndex, source, k, "0"); newParam[globalIndex] = ""; } }}>+</button>
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <!-- Binding list -->
    {#each handInputs as input (input.id)}
      {@const path = buildInputPath(hand, input.pathSuffix)}
      {@const srcs = handSources.filter((s) => s.path === path)}
      <button
        class="item"
        class:bound={srcs.length > 0}
        onclick={() => setSelectedInput(path)}
        onmouseenter={() => setHoveredInput(path)}
        onmouseleave={() => setHoveredInput(null)}
      >
        <div class="item-lbl">{input.label}</div>
        {#if srcs.length > 0}
          {#each srcs as s, i (i)}
            <div class="item-sub">
              <span class="mode">{s.mode}</span> →
              {#each Object.entries(s.inputs) as [sub, b], j (sub)}{j > 0 ? ", " : ""}<span class="act">{localizeAction(b.output)}</span> <span class="subt">({sub})</span>{/each}
            </div>
          {/each}
        {:else}
          <div class="item-none">no binding</div>
        {/if}
      </button>
    {/each}

    {#if outputs.length > 0}
      <div class="haptic">
        <span class="haptic-lbl">Haptic feedback</span>
        <select value={currentHaptic} onchange={(e) => setHaptic(e.currentTarget.value)}>
          <option value="">— none —</option>
          {#each outputs as a (a)}<option value={a}>{localizeAction(a)}</option>{/each}
        </select>
      </div>
    {/if}
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
    min-height: 0;
    height: 100%;
    padding-right: 2px;
  }
  /* binding list */
  .item {
    text-align: left;
    background: transparent;
    border: 1px solid hsl(var(--border) / 0.25);
    border-radius: var(--radius-s);
    padding: 8px 10px;
    cursor: pointer;
  }
  .item:hover {
    border-color: hsl(var(--primary) / 0.4);
    background: hsl(var(--primary) / 0.06);
  }
  .item.bound {
    border-color: hsl(var(--border) / 0.6);
  }
  .item-lbl {
    font-size: 12px;
    font-weight: 500;
    color: hsl(var(--foreground) / 0.85);
    margin-bottom: 2px;
  }
  .item.bound .item-lbl {
    color: hsl(var(--primary) / 0.85);
  }
  .item-sub {
    font-size: 10.5px;
    color: hsl(var(--muted));
    font-family: ui-monospace, monospace;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item-sub .act {
    color: hsl(var(--foreground) / 0.8);
  }
  .item-sub .subt,
  .item-sub .mode {
    color: hsl(var(--muted) / 0.7);
  }
  .item-none {
    font-size: 10.5px;
    color: hsl(var(--muted) / 0.6);
    font-style: italic;
  }
  .haptic {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 10px;
    border: 1px solid hsl(var(--border) / 0.4);
    border-radius: var(--radius-s);
    margin-top: 2px;
  }
  .haptic-lbl {
    font-size: 12px;
    font-weight: 500;
    color: hsl(var(--foreground) / 0.7);
  }
  /* input editor */
  .ed-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .back {
    background: transparent;
    border: none;
    color: hsl(var(--muted));
    font-size: 15px;
    cursor: pointer;
  }
  .ed-title {
    flex: 1;
    min-width: 0;
  }
  .lbl {
    font-size: 12.5px;
    font-weight: 600;
  }
  .mirror {
    font-size: 9px;
    color: hsl(var(--primary) / 0.6);
  }
  .bind {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
    border: none;
    border-radius: var(--radius-s);
    padding: 4px 10px;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
  }
  .empty {
    font-size: 11px;
    color: hsl(var(--muted));
    text-align: center;
    padding: 14px;
    border: 1px dashed hsl(var(--border) / 0.5);
    border-radius: var(--radius-s);
  }
  .sources {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .src {
    background: hsl(var(--surface-2) / 0.5);
    border: 1px solid hsl(var(--border) / 0.5);
    border-radius: var(--radius-s);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .src-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .src-n {
    font-size: 10.5px;
    color: hsl(var(--muted));
  }
  .del {
    background: transparent;
    border: none;
    color: hsl(var(--muted));
    cursor: pointer;
  }
  .del:hover {
    color: hsl(var(--danger));
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .k {
    font-size: 10px;
    color: hsl(var(--muted));
    width: 44px;
    flex: none;
  }
  .k.mono {
    font-family: ui-monospace, monospace;
  }
  select {
    flex: 1;
    min-width: 0;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 4px 6px;
    font-size: 11px;
  }
  .param-toggle {
    background: transparent;
    border: none;
    color: hsl(var(--muted));
    font-size: 10px;
    text-align: left;
    cursor: pointer;
    padding: 2px 0;
  }
  .param-toggle:hover {
    color: hsl(var(--foreground));
  }
  .params {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-left: 6px;
    border-left: 1px solid hsl(var(--border) / 0.5);
  }
  .prow {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .pk {
    font-size: 9px;
    color: hsl(var(--muted));
    width: 120px;
    flex: none;
    font-family: ui-monospace, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .prow input {
    flex: 1;
    min-width: 0;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 3px 6px;
    font-size: 10px;
    font-family: ui-monospace, monospace;
  }
  .pdel,
  .padd {
    background: transparent;
    border: none;
    color: hsl(var(--muted));
    cursor: pointer;
    flex: none;
  }
  .pdel:hover {
    color: hsl(var(--danger));
  }
</style>
