<script lang="ts">
  import Icon from "./Icon.svelte";
  import SourceCard from "./SourceCard.svelte";
  import type { HandId, InputDef } from "./api";
  import { bind } from "./store.svelte";
  import { useLinks, register } from "./links.svelte";
  import { actionName, inputLabel, prettyName } from "./labels";

  let { hand }: { hand: HandId } = $props();
  const links = useLinks();
  const e = $derived(bind.editing!);
  const sources = $derived((e.view?.sources ?? []).filter((s) => s.hand === hand));
  const defs = $derived(e.ctrl.inputs.filter((d) => d.side === "both" || d.side === hand));
  // Inputs the controller doesn't list (a pinch, a finger) close the list, flagged.
  const others = $derived([...new Set(sources.filter((s) => !e.ctrl.inputs.some((d) => d.id === s.input)).map((s) => s.input))]);

  function labelOf(h: HandId | null, id: string): string {
    const l = e.ctrl.inputs.find((d) => d.id === id)?.label ?? prettyName(id);
    return h && h !== hand ? inputLabel(h, l) : l;
  }

  function chordsOf(id: string) {
    return (e.view?.chords ?? [])
      .filter((c) => c.inputs.some((i) => i.hand === hand && i.input === id))
      .map((c) => {
        const others = c.inputs.filter((i) => !(i.hand === hand && i.input === id)).map((i) => labelOf(i.hand, i.input));
        const what = actionName(e.manifest, c.action);
        return { index: c.index, text: others.length ? `${what}  ·  with ${others.join(" + ")}` : what };
      });
  }
</script>

{#snippet section(id: string, label: string, def: InputDef | null)}
  {@const key = `${hand}:${id}`}
  {@const bindable = !!def && def.modes.length > 0}
  <div class="section" role="group" onmouseenter={() => links.enter(key)} onmouseleave={() => links.leave(key)}>
    <div class="in-head" class:lit={links.hover === key} use:register={[links.anchors, key]}>
      <span class="in-name" class:dim={!bindable}>{label}</span>
      {#if bindable}
        <button class="plus" title={`Use the ${inputLabel(hand, label).toLowerCase()} for something`} onclick={() => (e.popup = { kind: "addMode", hand, input: id })}>
          <Icon name="plus" size={16} />
        </button>
      {:else}
        <span class="note">{def?.note || (e.own ? "Monadeck can't read this input" : "xrizer ignores this input")}</span>
      {/if}
    </div>
    {#each sources.filter((s) => s.input === id) as s (s.index)}
      <SourceCard {s} {def} />
    {/each}
    {#each chordsOf(id) as c (c.index)}
      {#if e.own}
        <button class="chord" title="Buttons held together" onclick={() => (e.popup = { kind: "chords" })}>
          <span class="chord-tag">Chord</span>
          <Icon name="link-simple" size={15} />
          <span class="chord-text">{c.text}</span>
          <Icon name="caret-right" size={13} />
        </button>
      {:else}
        <div class="chord ignored" title="Buttons held together: xrizer doesn't read chords, so this does nothing">
          <span class="chord-tag">Chord</span>
          <span class="chord-text">{c.text}</span>
          <span class="ignored-flag">xrizer ignores this</span>
        </div>
      {/if}
    {/each}
  </div>
{/snippet}

<div class="col">
  <div class="col-title" class:right={hand === "right"}>{hand === "left" ? "Left" : "Right"} controller</div>
  <div class="list" data-list>
    {#each defs as d (d.id)}
      {@render section(d.id, d.label, d)}
    {/each}
    {#each others as id (id)}
      {@render section(id, prettyName(id), null)}
    {/each}
  </div>
</div>

<style>
  .col {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .col-title {
    font-size: 12.5px;
    color: hsl(var(--muted));
    padding: 0 2px 6px;
  }
  .col-title.right {
    text-align: right;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-right: 6px;
    padding-bottom: 12px;
  }
  .section {
    padding-bottom: 8px;
  }
  .in-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    height: 40px;
    border-bottom: 1px solid hsl(var(--foreground) / 0.12);
    transition: border-color 0.12s ease;
  }
  .in-head.lit {
    border-bottom: 2px solid hsl(var(--primary));
  }
  .in-name {
    font-size: 16px;
    font-weight: 500;
    transition: color 0.12s ease;
  }
  .in-head.lit .in-name {
    color: hsl(var(--primary));
  }
  .in-name.dim {
    color: hsl(var(--muted));
  }
  .plus {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 9px;
    border: none;
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
    transition:
      background 0.12s ease,
      color 0.12s ease;
  }
  .plus:hover {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
  }
  .note {
    font-size: 11.5px;
    color: hsl(var(--muted));
    background: hsl(var(--foreground) / 0.06);
    border-radius: 99px;
    padding: 2px 10px;
    white-space: nowrap;
  }
  .chord {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    margin-top: 6px;
    height: 36px;
    padding: 0 10px 0 12px;
    border-radius: 12px;
    border: 1px solid hsl(var(--foreground) / 0.05);
    background: hsl(var(--surface));
    color: hsl(var(--primary));
    text-align: left;
  }
  .chord:hover {
    border-color: hsl(var(--primary) / 0.45);
  }
  .chord.ignored {
    color: hsl(var(--muted));
  }
  .chord.ignored:hover {
    border-color: hsl(var(--foreground) / 0.05);
  }
  .chord.ignored .chord-text {
    color: hsl(var(--muted));
  }
  .ignored-flag {
    font-size: 11px;
    color: hsl(var(--warn));
    background: hsl(var(--warn) / 0.16);
    border-radius: 99px;
    padding: 1px 8px;
    white-space: nowrap;
  }
  .chord-tag {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: hsl(var(--muted));
  }
  .chord-text {
    flex: 1;
    min-width: 0;
    color: hsl(var(--foreground));
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
