<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { InputDef, Source } from "./api";
  import { bind, apply } from "./store.svelte";
  import { actionName, modeTitle, prettyName } from "./labels";

  let { s, def }: { s: Source; def: InputDef | null } = $props();
  const e = $derived(bind.editing!);
  const mode = $derived(s.known ? bind.modes.find((m) => m.id === s.mode) : undefined);
  const editable = $derived(!!mode && !!def && def.modes.length > 0);
  // The mode's slots this input has, then anything else the file binds.
  const rows = $derived.by(() => {
    const out: { key: string; label: string; editable: boolean }[] = [];
    for (const sl of mode?.slots ?? []) {
      if (sl.key === "touch" && def && !def.touch) continue;
      out.push({ key: sl.key, label: sl.label, editable });
    }
    for (const [k] of s.outputs) if (!out.some((r) => r.key === k)) out.push({ key: k, label: prettyName(k), editable: false });
    return out;
  });
  const flag = $derived(
    !s.known
      ? { text: e.own ? "Monadeck ignores this" : "xrizer can't read this", tone: "danger" }
      : !def
        ? { text: e.own ? "Monadeck ignores this" : "xrizer ignores this", tone: "warn" }
        : null,
  );
  const output = (key: string) => s.outputs.find(([k]) => k === key)?.[1] ?? null;
</script>

<div class="card">
  <div class="head">
    <span class="title">{modeTitle(s.mode, bind.modes)}</span>
    {#if flag}<span class="flag {flag.tone}">{flag.text}</span>{/if}
    <span class="spacer"></span>
    {#if editable && (def?.modes.length ?? 0) > 1}
      <button class="mini" title="Use it another way" onclick={() => (e.popup = { kind: "changeMode", index: s.index })}><Icon name="pencil-simple" size={15} /></button>
    {/if}
    <button class="mini danger" title="Remove" onclick={() => apply({ op: "remove", index: s.index })}><Icon name="trash" size={15} /></button>
  </div>
  {#each rows as r (r.key)}
    {@const a = output(r.key)}
    <button class="row" disabled={!r.editable} onclick={() => (e.popup = { kind: "action", index: s.index, slot: r.key })}>
      <span class="slot">{r.label}</span>
      <span class="what" class:none={!a}>{a ? actionName(e.manifest, a) : "None"}</span>
      {#if r.editable}<span class="caret"><Icon name="caret-right" size={13} /></span>{/if}
    </button>
  {/each}
</div>

<style>
  .card {
    background: hsl(var(--surface));
    border: 1px solid hsl(var(--foreground) / 0.05);
    border-radius: 12px;
    padding: 5px 6px 6px 12px;
    margin-top: 6px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
  }
  .title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: hsl(var(--muted));
  }
  .flag {
    font-size: 11px;
    border-radius: 99px;
    padding: 1px 8px;
  }
  .flag.danger {
    color: hsl(var(--danger));
    background: hsl(var(--danger) / 0.16);
  }
  .flag.warn {
    color: hsl(var(--warn));
    background: hsl(var(--warn) / 0.16);
  }
  .spacer {
    flex: 1;
  }
  .mini {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    border: none;
    background: transparent;
    color: hsl(var(--muted) / 0.85);
  }
  .mini:hover {
    background: hsl(var(--foreground) / 0.08);
    color: hsl(var(--foreground));
  }
  .mini.danger:hover {
    background: hsl(var(--danger) / 0.14);
    color: hsl(var(--danger));
  }
  .row {
    width: 100%;
    display: grid;
    grid-template-columns: 104px 1fr 16px;
    align-items: center;
    gap: 6px;
    height: 29px;
    padding: 0 6px 0 4px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: hsl(var(--foreground));
    text-align: left;
    font-size: 13px;
  }
  .row:hover:not(:disabled) {
    background: hsl(var(--foreground) / 0.06);
  }
  .row:disabled {
    cursor: default;
    color: hsl(var(--muted));
  }
  .slot {
    color: hsl(var(--muted));
  }
  .what {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .what.none {
    color: hsl(var(--muted) / 0.6);
  }
  .caret {
    color: hsl(var(--muted) / 0.6);
  }
  .row:hover .caret {
    color: hsl(var(--foreground));
  }
</style>
