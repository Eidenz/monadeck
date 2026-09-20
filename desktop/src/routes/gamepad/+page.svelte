<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import WindowControls from "$lib/components/WindowControls.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import {
    gp,
    load,
    select,
    newProfile,
    duplicate,
    save,
    remove,
    revert,
    addRule,
    updateRule,
    removeRule,
    isDirty,
    isReadOnly,
  } from "$lib/gamepad/store.svelte";
  import {
    INPUTS,
    INPUT_GROUPS,
    BUTTONS,
    BUTTON_LABELS,
    AXES,
    AXIS_LABELS,
    MOUSE_BUTTONS,
    TARGET_KINDS,
    targetKind,
    defaultTarget,
    usesSpeed,
    usesThreshold,
    inputInfo,
    type Hand,
    type Input,
    type Rule,
    type TargetKind,
  } from "$lib/gamepad/types";
  import { KEY_OPTIONS, keyName, BROWSER_CODE_TO_EVDEV } from "$lib/gamepad/keys";

  const dirty = $derived(isDirty());
  const readOnly = $derived(isReadOnly());
  const rulesOf = (hand: Hand) =>
    (gp.draft?.rules ?? []).map((rule, i) => ({ rule, i })).filter((x) => x.rule.hand === hand);

  // "Press a key…" capture for a key target.
  let capturing = $state<number | null>(null);
  let confirmDelete = $state(false);

  function setKind(i: number, kind: TargetKind) {
    updateRule(i, { target: defaultTarget(kind) });
  }
  function setInput(i: number, input: Input) {
    const patch: Partial<Rule> = { input };
    // A digital input can't invert.
    if (!inputInfo(input).analog) patch.invert = false;
    updateRule(i, patch);
  }
  function num(v: string, fallback: number): number {
    const n = Number(v);
    return Number.isFinite(n) ? n : fallback;
  }

  onMount(() => {
    const win = getCurrentWindow();
    let unfocus: (() => void) | undefined;
    (async () => {
      if (await win.isVisible()) load();
      unfocus = await win.onFocusChanged(({ payload: focused }) => {
        // Files may have changed under us (hand edits); keep the list fresh,
        // but never clobber an unsaved draft.
        if (focused && !isDirty()) load();
      });
    })();
    const onKey = (e: KeyboardEvent) => {
      if (capturing === null) return;
      e.preventDefault();
      if (e.code === "Escape") {
        capturing = null;
        return;
      }
      const code = BROWSER_CODE_TO_EVDEV[e.code];
      if (code !== undefined) {
        updateRule(capturing, { target: { key: code } });
        capturing = null;
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      unfocus?.();
      window.removeEventListener("keydown", onKey, true);
    };
  });
</script>

<div class="win">
  <header class="topbar" data-tauri-drag-region>
    <span class="ttl" data-tauri-drag-region>Monad<b>eck</b> · Gamepad remaps</span>
    <div class="spacer" data-tauri-drag-region></div>
    <WindowControls closeAction="hide" />
  </header>

  <div class="cols">
    <aside class="side">
      <div class="side-head">
        <span>Profiles</span>
        <button class="small" onclick={() => load()} disabled={gp.loading}>{gp.loading ? "…" : "Reload"}</button>
      </div>
      <div class="list">
        {#each gp.files as f, i (f.file || "stock")}
          <button class="item state-layer" class:active={gp.selected === i} onclick={() => select(i)}>
            <span class="name">{f.profile.name}</span>
            <span class="sub">
              {#if f.stock}built-in · read-only{:else if f.profile.game}auto for “{f.profile.game}”{:else}{f.file}{/if}
            </span>
          </button>
        {/each}
        {#if gp.isNew && gp.draft}
          <div class="item active">
            <span class="name">{gp.draft.name}</span>
            <span class="sub">unsaved</span>
          </div>
        {/if}
      </div>
      <div class="side-foot">
        <button class="accent" onclick={newProfile}>New profile</button>
        <p class="path" title={gp.dir}>{gp.dir}</p>
        <p class="hint">
          Gaming mode uses these. Pick one from the watch's sliders button; a profile with a game name switches in when that game launches.
        </p>
      </div>
    </aside>

    <main class="editor">
      {#if !gp.draft}
        <p class="placeholder">{gp.loading ? "Loading…" : "Pick a profile, or create one."}</p>
      {:else}
        <div class="ed-head">
          <div class="field">
            <span class="lbl">Name</span>
            <input value={gp.draft.name} disabled={readOnly} oninput={(e) => gp.draft && (gp.draft.name = e.currentTarget.value)} />
          </div>
          <div class="field grow">
            <span class="lbl">Auto-select for game <span class="dim">(part of its library name, optional)</span></span>
            <input
              value={gp.draft.game ?? ""}
              placeholder="e.g. Elden Ring"
              disabled={readOnly}
              oninput={(e) => gp.draft && (gp.draft.game = e.currentTarget.value || null)}
            />
          </div>
          <div class="actions">
            {#if readOnly}
              <button class="accent" onclick={duplicate}>Duplicate to edit</button>
            {:else}
              <button onclick={duplicate} disabled={gp.isNew && !gp.draft.rules.length}>Duplicate</button>
              <button class="accent" onclick={save} disabled={!dirty || gp.saving}>{gp.saving ? "Saving…" : gp.isNew ? "Create" : "Save"}</button>
              {#if dirty}
                <button onclick={revert}>Revert</button>
              {/if}
              {#if !gp.isNew}
                {#if confirmDelete}
                  <button class="danger" onclick={() => { confirmDelete = false; remove(); }}>Really delete</button>
                  <button onclick={() => (confirmDelete = false)}>Keep</button>
                {:else}
                  <button class="ghost" onclick={() => (confirmDelete = true)}>Delete</button>
                {/if}
              {/if}
            {/if}
          </div>
        </div>
        {#if gp.error}<p class="msg bad">{gp.error}</p>{:else if gp.status}<p class="msg">{gp.status}</p>{/if}
        {#if readOnly}
          <p class="msg dim">The stock Xbox layout can't be changed. Duplicate it and edit the copy.</p>
        {/if}

        <div class="hands">
          {#each [["left", "Left controller"], ["right", "Right controller"]] as [hand, title] (hand)}
            <section class="hand">
              <h3>{title}</h3>
              <div class="rules">
                {#each rulesOf(hand as Hand) as { rule, i } (i)}
                  {@const kind = targetKind(rule.target)}
                  {@const info = inputInfo(rule.input)}
                  <div class="rule">
                    <div class="rule-main">
                      <select class="input" value={rule.input} disabled={readOnly} onchange={(e) => setInput(i, e.currentTarget.value as Input)}>
                        {#each INPUT_GROUPS as g (g)}
                          <optgroup label={g}>
                            {#each INPUTS.filter((x) => x.group === g) as x (x.id)}<option value={x.id}>{x.label}</option>{/each}
                          </optgroup>
                        {/each}
                      </select>
                      <span class="arrow">→</span>
                      <select class="kind" value={kind} disabled={readOnly} onchange={(e) => setKind(i, e.currentTarget.value as TargetKind)}>
                        <optgroup label="Gamepad">
                          {#each TARGET_KINDS.filter((k) => k.group === "Gamepad") as k (k.id)}<option value={k.id}>{k.label}</option>{/each}
                        </optgroup>
                        <optgroup label="Keyboard & mouse">
                          {#each TARGET_KINDS.filter((k) => k.group !== "Gamepad") as k (k.id)}<option value={k.id}>{k.label}</option>{/each}
                        </optgroup>
                      </select>
                      {#if typeof rule.target !== "string"}
                        {#if "button" in rule.target}
                          <select class="val" value={rule.target.button} disabled={readOnly} onchange={(e) => updateRule(i, { target: { button: e.currentTarget.value as (typeof BUTTONS)[number] } })}>
                            {#each BUTTONS as b (b)}<option value={b}>{BUTTON_LABELS[b]}</option>{/each}
                          </select>
                        {:else if "axis" in rule.target}
                          <select class="val" value={rule.target.axis} disabled={readOnly} onchange={(e) => updateRule(i, { target: { axis: e.currentTarget.value as (typeof AXES)[number] } })}>
                            {#each AXES as a (a)}<option value={a}>{AXIS_LABELS[a]}</option>{/each}
                          </select>
                        {:else if "key" in rule.target}
                          {@const code = rule.target.key}
                          <select class="val" value={String(code)} disabled={readOnly} onchange={(e) => updateRule(i, { target: { key: Number(e.currentTarget.value) } })}>
                            {#each KEY_OPTIONS as k (k.code)}<option value={String(k.code)}>{k.label}</option>{/each}
                            {#if !KEY_OPTIONS.some((k) => k.code === code)}<option value={String(code)}>{keyName(code)}</option>{/if}
                          </select>
                          <button class="small" class:capturing={capturing === i} disabled={readOnly} onclick={() => (capturing = capturing === i ? null : i)}>
                            {capturing === i ? "Press a key… (Esc cancels)" : "Press a key"}
                          </button>
                        {:else}
                          <select class="val" value={String(rule.target.mouse_button)} disabled={readOnly} onchange={(e) => updateRule(i, { target: { mouse_button: Number(e.currentTarget.value) } })}>
                            {#each MOUSE_BUTTONS as m (m.code)}<option value={String(m.code)}>{m.label}</option>{/each}
                          </select>
                        {/if}
                      {/if}
                      <button class="del" title="Remove" disabled={readOnly} onclick={() => removeRule(i)}>✕</button>
                    </div>
                    <div class="rule-opts">
                      {#if usesThreshold(rule.input)}
                        <label class="opt" title={info.analog ? "Pressed past this much of the input's range" : info.group === "Trackpad" ? "Trackpad force that counts as a press" : "Stick deflection that counts as a press"}>
                          <span>Threshold</span>
                          <input type="number" min="0.05" max="1" step="0.05" value={rule.threshold} disabled={readOnly} onchange={(e) => updateRule(i, { threshold: Math.min(1, Math.max(0.05, num(e.currentTarget.value, 0.5))) })} />
                        </label>
                      {/if}
                      {#if info.analog}
                        <label class="opt" title="Flip the input's sign (inverted look, mirrored axis)">
                          <span>Invert</span>
                          <Toggle checked={rule.invert} disabled={readOnly} label="Invert" onchange={(v) => updateRule(i, { invert: v })} />
                        </label>
                      {/if}
                      {#if usesSpeed(rule.target)}
                        <label class="opt" title={kind.startsWith("wheel") ? "Scroll notches per second at full deflection" : "Pixels per second at full deflection"}>
                          <span>Speed</span>
                          <input type="number" min="1" step={kind.startsWith("wheel") ? 1 : 50} value={rule.speed} disabled={readOnly} onchange={(e) => updateRule(i, { speed: Math.max(1, num(e.currentTarget.value, 900)) })} />
                          <span class="unit">{kind.startsWith("wheel") ? "notches/s" : "px/s"}</span>
                        </label>
                      {/if}
                      {#if !info.analog && (kind === "axis")}
                        <span class="note">button → full axis while held</span>
                      {/if}
                      {#if !info.analog && usesSpeed(rule.target)}
                        <span class="note">button → constant motion while held</span>
                      {/if}
                    </div>
                  </div>
                {/each}
                {#if !rulesOf(hand as Hand).length}
                  <p class="hint">Nothing mapped on this hand.</p>
                {/if}
              </div>
              {#if !readOnly}
                <button class="add" onclick={() => addRule(hand as Hand)}>+ Add mapping</button>
              {/if}
            </section>
          {/each}
        </div>
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
  .side {
    width: 250px;
    flex: none;
    border-right: 1px solid hsl(var(--border) / 0.6);
    background: hsl(var(--surface) / 0.5);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .side-head {
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
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .item {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    color: hsl(var(--foreground));
    padding: 8px 10px;
    border-radius: var(--radius-s);
  }
  .item.active {
    background: hsl(var(--primary) / 0.12);
    border-color: hsl(var(--primary) / 0.5);
  }
  .item .name {
    font-size: 13px;
    font-weight: 600;
  }
  .item .sub {
    font-size: 11px;
    color: hsl(var(--muted));
  }
  .side-foot {
    flex: none;
    padding: 10px 12px 12px;
    border-top: 1px solid hsl(var(--border) / 0.6);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .path {
    margin: 0;
    font-size: 10.5px;
    color: hsl(var(--muted));
    font-family: ui-monospace, monospace;
    word-break: break-all;
  }
  .hint {
    margin: 0;
    font-size: 11.5px;
    color: hsl(var(--muted));
  }
  .editor {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    padding: 14px 16px;
    gap: 10px;
    min-height: 0;
    overflow-y: auto;
  }
  .placeholder {
    margin: auto;
    color: hsl(var(--muted));
    font-size: 13px;
  }
  .ed-head {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    flex-wrap: wrap;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 200px;
  }
  .field.grow {
    flex: 1;
  }
  .lbl {
    font-size: 12px;
    font-weight: 600;
  }
  .dim {
    color: hsl(var(--muted));
    font-weight: 400;
  }
  input,
  select {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 6px 9px;
    font-size: 12.5px;
    font-family: inherit;
    min-width: 0;
  }
  input:focus,
  select:focus {
    outline: none;
    border-color: hsl(var(--primary));
  }
  input:disabled,
  select:disabled,
  button:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .actions {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-left: auto;
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
  button.danger {
    background: hsl(var(--danger));
    border-color: transparent;
    color: white;
    font-weight: 700;
  }
  button.ghost {
    background: transparent;
    color: hsl(var(--muted));
  }
  button.small {
    padding: 4px 10px;
    font-size: 11px;
  }
  button.capturing {
    border-color: hsl(var(--primary));
    color: hsl(var(--primary));
  }
  .msg {
    margin: 0;
    font-size: 12px;
    color: hsl(var(--ok));
  }
  .msg.bad {
    color: hsl(var(--danger));
  }
  .msg.dim {
    color: hsl(var(--muted));
  }
  .hands {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    align-items: start;
  }
  @media (max-width: 1000px) {
    .hands {
      grid-template-columns: 1fr;
    }
  }
  .hand {
    background: hsl(var(--surface) / 0.6);
    border: 1px solid hsl(var(--border) / 0.7);
    border-radius: var(--radius);
    padding: 10px 12px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  h3 {
    margin: 0;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: hsl(var(--muted));
  }
  .rules {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .rule {
    background: hsl(var(--surface-2) / 0.6);
    border: 1px solid hsl(var(--border) / 0.5);
    border-radius: var(--radius-s);
    padding: 7px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .rule-main {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .rule-main select {
    padding: 4px 6px;
    font-size: 11.5px;
  }
  .rule-main .input {
    flex: 1 1 150px;
  }
  .rule-main .kind {
    flex: 1 1 120px;
  }
  .rule-main .val {
    flex: 1 1 130px;
  }
  .arrow {
    color: hsl(var(--muted));
    flex: none;
  }
  .del {
    padding: 3px 8px;
    font-size: 11px;
    color: hsl(var(--muted));
    background: transparent;
    margin-left: auto;
  }
  .del:hover:not(:disabled) {
    color: hsl(var(--danger));
  }
  .rule-opts {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    min-height: 0;
  }
  .rule-opts:empty {
    display: none;
  }
  .opt {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: hsl(var(--muted));
  }
  .opt input[type="number"] {
    width: 78px;
    padding: 3px 6px;
    font-size: 11.5px;
  }
  .unit {
    font-size: 10.5px;
  }
  .note {
    font-size: 10.5px;
    color: hsl(var(--muted));
    font-style: italic;
  }
  .add {
    align-self: flex-start;
    font-size: 12px;
    padding: 5px 10px;
  }
</style>
