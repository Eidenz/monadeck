<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import InputColumn from "./InputColumn.svelte";
  import Drawings from "./Drawings.svelte";
  import Pickers from "./Pickers.svelte";
  import { bind, apply, dirty, leave, reset, save, setJson, editJson, setSet, undo } from "./store.svelte";
  import { Links, provideLinks } from "./links.svelte";

  const links = new Links();
  provideLinks(links);

  const e = $derived(bind.editing!);
  const isDirty = $derived(dirty());
  const sets = $derived(e.manifest.sets.filter((s) => !s.hidden || (e.view?.counts[s.key] ?? 0) > 0));
  const unreadable = $derived(e.view?.unreadable ?? []);
  const subtitle = $derived(e.personal ? "Your personal binding" : e.own ? "Monadeck's default controls" : "The game's own binding");
  const many = $derived((e.own ? bind.ownControllers.length : (e.game?.controllers.length ?? 0)) > 1);

  // Required actions of a set nothing drives yet (the tab's amber mark).
  function missing(set: string): string[] {
    const bound = new Set(e.view?.bound ?? []);
    return e.manifest.actions.filter((a) => a.set === set && a.mandatory && ["boolean", "vector1", "vector2"].includes(a.kind) && !bound.has(a.key)).map((a) => a.name);
  }

  // --- the link line between an entry and its part of the drawing
  let body = $state<HTMLElement>();
  let line = $state<{ x1: number; y1: number; x2: number; x3: number; y3: number } | null>(null);
  function measure() {
    const key = links.hover;
    const a = key ? links.anchors.get(key) : undefined;
    const s = key ? links.spots.get(key) : undefined;
    if (!key || !a || !s || !body || e.popup || !links.inView(key)) {
      line = null;
      return;
    }
    const b = body.getBoundingClientRect();
    const ar = a.getBoundingClientRect();
    const sr = s.getBoundingClientRect();
    const left = key.startsWith("left:");
    const x1 = (left ? ar.right + 6 : ar.left - 6) - b.left;
    line = { x1, y1: ar.top + ar.height / 2 - b.top, x2: x1 + (left ? 18 : -18), x3: sr.left + sr.width / 2 - b.left, y3: sr.top + sr.height / 2 - b.top };
  }
  $effect(() => {
    // Re-measure when the hovered input, the set or a popup changes.
    void links.hover;
    void e.popup;
    void e.view;
    requestAnimationFrame(measure);
  });
  onMount(() => {
    const onScroll = () => requestAnimationFrame(measure);
    body?.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    return () => {
      body?.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
    };
  });

  // Reset asks twice.
  let armed = $state(false);
  let armTimer: ReturnType<typeof setTimeout> | undefined;
  function resetClick() {
    if (armed) {
      armed = false;
      reset();
      return;
    }
    armed = true;
    clearTimeout(armTimer);
    armTimer = setTimeout(() => (armed = false), 3000);
  }

  function onkey(ev: KeyboardEvent) {
    if ((ev.ctrlKey || ev.metaKey) && ev.key === "s") {
      ev.preventDefault();
      if (isDirty && !e.view?.blocker) save();
    }
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="editor">
  <header class="head">
    <button class="back" onclick={leave}><Icon name="arrow-left" size={16} /> Back</button>
    <div class="who">
      <span class="name">{e.name}</span>
      <span class="sub" class:mine={e.personal}>{subtitle}</span>
    </div>
    <span class="grow"></span>
    <button class="json" class:on={e.json} title="Edit the binding's JSON" onclick={() => setJson(!e.json)}><Icon name="brackets-curly" size={16} /> JSON</button>
    <button class="ctrl" disabled={!many} onclick={() => (e.popup = { kind: "controller" })}>
      <Icon name="game-controller" size={17} />
      {e.ctrl.name}
      {#if many}<Icon name="caret-down" size={14} />{/if}
    </button>
  </header>

  {#if unreadable.length}
    {@const kinds = [...new Set(unreadable.map(([, m]) => m))].sort()}
    <div class="banner">
      <Icon name="warning" size={22} />
      <div class="b-text">
        <b>xrizer can't read this binding</b>
        <span>
          {unreadable.length} input{unreadable.length === 1 ? " uses" : "s use"} a mode it doesn't know ({kinds.join(", ")}), so it ignores the whole file and the game gets no controls from it.
        </span>
      </div>
      <button class="primary" title="Drop those inputs, then save to make it readable" onclick={() => apply({ op: "dropUnreadable" })}><Icon name="wrench" size={16} /> Remove them</button>
    </div>
  {/if}

  <nav class="tabs">
    {#each sets as s (s.key)}
      {@const m = missing(s.key)}
      {@const n = e.view?.counts[s.key] ?? 0}
      <button
        class="tab"
        class:on={s.key === e.set}
        title={m.length ? `${s.name}: not on any input yet — ${m.join(", ")}` : `${s.name}: ${n} input${n === 1 ? "" : "s"} in use`}
        onclick={() => setSet(s.key)}
      >
        <span class="tab-name">{s.name}</span>
        {#if m.length}<span class="dot"></span>{/if}
        <span class="count">{n}</span>
      </button>
    {:else}
      <span class="no-sets">This game lists no actions</span>
    {/each}
  </nav>

  {#if e.json}
    <div class="json-view">
      <textarea spellcheck="false" value={e.jsonText} oninput={(ev) => editJson(ev.currentTarget.value)}></textarea>
      {#if e.jsonError}<p class="json-err"><Icon name="warning" size={14} /> Not applied: {e.jsonError}</p>{/if}
    </div>
  {:else}
    <div class="body" bind:this={body}>
      <InputColumn hand="left" />
      <Drawings />
      <InputColumn hand="right" />
      {#if line}
        <svg class="link" aria-hidden="true">
          <polyline points="{line.x1},{line.y1} {line.x2},{line.y1} {line.x3},{line.y3}" />
          <circle cx={line.x1} cy={line.y1} r="3" />
        </svg>
      {/if}
    </div>
  {/if}

  <footer class="foot">
    {#if bind.error}
      <span class="status err"><Icon name="warning" size={15} /> {bind.error}</span>
    {:else if e.view?.blocker}
      <span class="status warn"><Icon name="warning" size={15} /> {e.view.blocker}</span>
    {:else if isDirty}
      <span class="status warn"><Icon name="circle" size={13} /> Unsaved changes</span>
    {:else if e.personal}
      <span class="status ok"><Icon name="check" size={15} /> {e.own ? "Saved · in use in the headset" : `Saved · ${e.name} uses it from its next start`}</span>
    {:else}
      <span class="status"><Icon name="info" size={15} /> {e.own ? "Monadeck's default controls · save a change to make them yours" : "The game's own binding · save a change to make it yours"}</span>
    {/if}
    <span class="grow"></span>
    {#if e.personal && !e.busy}
      <button class="danger-btn" class:armed title={e.own ? "Go back to Monadeck's default controls (yours is kept as a .bak file)" : "Go back to the game's own binding (yours is kept as a .bak file)"} onclick={resetClick}>
        <Icon name="arrow-counter-clockwise" size={16} />
        {armed ? "Click again to reset" : "Reset to default"}
      </button>
    {/if}
    {#if isDirty}
      <button class="neutral" onclick={undo}><Icon name="arrow-counter-clockwise" size={16} /> Undo changes</button>
    {/if}
    <button
      class="primary"
      disabled={!isDirty || e.busy || !!e.view?.blocker}
      title={e.view?.blocker ?? (e.own ? "Save your binding: the headset uses it right away" : "Save as your personal binding (the game's own file stays as it is)")}
      onclick={() => save()}
    >
      <Icon name="floppy-disk" size={16} />
      {e.busy ? "Saving…" : "Save"}
    </button>
  </footer>

  <Pickers />
</div>

<style>
  .editor {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 12px 18px 0;
    gap: 12px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 16px;
    flex: none;
  }
  .back,
  .json,
  .ctrl {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    height: 36px;
    padding: 0 14px;
    border-radius: 10px;
    border: none;
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
    font-size: 13px;
  }
  .back:hover,
  .json:hover,
  .ctrl:hover:not(:disabled) {
    background: hsl(var(--border));
  }
  .ctrl:disabled {
    cursor: default;
  }
  .json {
    color: hsl(var(--muted));
  }
  .json.on {
    color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.14);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-size: 19px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: 12.5px;
    color: hsl(var(--muted));
  }
  .sub.mine {
    color: hsl(var(--primary));
  }
  .grow {
    flex: 1;
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 14px;
    border-radius: 13px;
    background: hsl(var(--danger) / 0.12);
    border: 1px solid hsl(var(--danger) / 0.45);
    color: hsl(var(--danger));
    flex: none;
  }
  .b-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    font-size: 12.5px;
    color: hsl(var(--muted));
  }
  .b-text b {
    color: hsl(var(--foreground));
    font-size: 14px;
    font-weight: 600;
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 4px;
    border-radius: 13px;
    background: hsl(var(--background));
    border: 1px solid hsl(var(--border) / 0.5);
    flex: none;
    overflow-x: auto;
  }
  .tab {
    flex: 1;
    min-width: 110px;
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 36px;
    padding: 0 40px 0 14px;
    border-radius: 10px;
    border: none;
    background: transparent;
    color: hsl(var(--foreground) / 0.85);
    font-size: 13.5px;
  }
  .tab:hover {
    background: hsl(var(--foreground) / 0.06);
  }
  .tab.on {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
    font-weight: 600;
  }
  .tab-name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .count {
    position: absolute;
    right: 8px;
    min-width: 24px;
    padding: 1px 7px;
    border-radius: 99px;
    font-size: 11.5px;
    background: hsl(var(--foreground) / 0.08);
  }
  .tab.on .count {
    background: hsl(0 0% 0% / 0.16);
  }
  .dot {
    position: absolute;
    right: 38px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: hsl(var(--warn));
  }
  .no-sets {
    padding: 8px 12px;
    color: hsl(var(--muted));
    font-size: 13px;
  }
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(250px, 1fr) minmax(300px, 420px) minmax(250px, 1fr);
    gap: 20px;
  }
  .link {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    overflow: visible;
  }
  .link polyline {
    fill: none;
    stroke: hsl(var(--primary) / 0.9);
    stroke-width: 1.6;
  }
  .link circle {
    fill: hsl(var(--primary));
  }
  .json-view {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  textarea {
    flex: 1;
    min-height: 0;
    resize: none;
    background: hsl(var(--background) / 0.7);
    border: 1px solid hsl(var(--border));
    border-radius: 12px;
    color: hsl(var(--foreground));
    padding: 12px 14px;
    font-family: ui-monospace, monospace;
    font-size: 12px;
    line-height: 1.5;
    white-space: pre;
    user-select: text;
  }
  textarea:focus {
    outline: none;
    border-color: hsl(var(--primary));
  }
  .json-err {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: 12.5px;
    color: hsl(var(--warn));
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 58px;
    border-top: 1px solid hsl(var(--foreground) / 0.06);
    flex: none;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 13px;
    color: hsl(var(--muted));
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .status.ok {
    color: hsl(var(--primary));
  }
  .status.warn {
    color: hsl(var(--warn));
  }
  .status.err {
    color: hsl(var(--danger));
  }
  .foot button,
  .banner button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 36px;
    padding: 0 16px;
    border-radius: 10px;
    font-size: 13px;
    border: 1px solid transparent;
    flex: none;
  }
  .primary {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
    font-weight: 600;
    min-width: 110px;
    justify-content: center;
  }
  .primary:hover:not(:disabled) {
    filter: brightness(1.08);
  }
  .primary:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .neutral {
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
  }
  .neutral:hover {
    background: hsl(var(--border));
  }
  .danger-btn {
    background: transparent;
    border-color: hsl(var(--danger) / 0.55) !important;
    color: hsl(var(--danger));
  }
  .danger-btn:hover {
    background: hsl(var(--danger) / 0.12);
  }
  .danger-btn.armed {
    background: hsl(var(--danger));
    color: white;
  }
</style>
