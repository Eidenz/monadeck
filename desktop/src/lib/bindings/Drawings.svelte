<script lang="ts">
  import Icon from "./Icon.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import type { HandId } from "./api";
  import { bind } from "./store.svelte";
  import { useLinks, register } from "./links.svelte";
  import { ART, inputLabel } from "./labels";

  const links = useLinks();
  const e = $derived(bind.editing!);
  const art = $derived(e.ctrl.art ? ART[e.ctrl.art] : null);
  const missing = $derived(
    e.manifest.actions
      .filter((a) => a.set === e.set && a.mandatory && ["boolean", "vector1", "vector2"].includes(a.kind))
      .filter((a) => !(e.view?.bound ?? []).includes(a.key))
      .map((a) => a.name),
  );
  const chordCount = $derived(e.view?.chords.length ?? 0);

  function on(hand: HandId, id: string) {
    const d = e.ctrl.inputs.find((x) => x.id === id);
    return d && (d.side === "both" || d.side === hand) ? d : null;
  }

  // A part held under the pointer a moment, its entry out of view: bring it in.
  let dwell: ReturnType<typeof setTimeout> | undefined;
  function enter(key: string) {
    links.enter(key);
    clearTimeout(dwell);
    dwell = setTimeout(() => {
      if (links.hover === key && !links.inView(key)) links.reveal(key);
    }, 300);
  }
  function leave(key: string) {
    clearTimeout(dwell);
    links.leave(key);
  }
  function tap(hand: HandId, id: string) {
    const key = `${hand}:${id}`;
    links.reveal(key);
    const d = on(hand, id);
    const used = (e.view?.sources ?? []).some((s) => s.hand === hand && s.input === id);
    if (d && d.modes.length && !used) e.popup = { kind: "addMode", hand, input: id };
  }
</script>

<div class="center">
  {#if art}
    <div class="arts">
      {#each ["left", "right"] as const as hand (hand)}
        <div class="art" style:aspect-ratio={art.aspect}>
          <img src={art.url} alt="" class:flipped={hand === "left"} draggable="false" />
          {#each e.ctrl.spots.filter((sp) => on(hand, sp.id)) as sp (sp.id)}
            {@const key = `${hand}:${sp.id}`}
            {@const x = hand === "left" ? 1 - sp.x : sp.x}
            <button
              class="spot"
              class:lit={links.hover === key}
              style:left="{x * 100}%"
              style:top="{sp.y * 100}%"
              style:width="max(16px, {sp.r * 200}%)"
              title={inputLabel(hand, on(hand, sp.id)!.label)}
              aria-label={inputLabel(hand, on(hand, sp.id)!.label)}
              use:register={[links.spots, key]}
              onmouseenter={() => enter(key)}
              onmouseleave={() => leave(key)}
              onclick={() => tap(hand, sp.id)}
            >
              {#if e.ctrl.art === "touch" && ["a", "b", "x", "y"].includes(sp.id)}
                <span class="letter">{sp.id.toUpperCase()}</span>
              {/if}
            </button>
          {/each}
        </div>
      {/each}
    </div>
  {/if}

  <div class="mirror-row">
    <Icon name="flip-horizontal" size={19} />
    <span class="m-text">
      <span>Mirror mode</span>
      <small>Changes copy to the other hand</small>
    </span>
    <Toggle label="Mirror mode" checked={e.mirror} onchange={(v) => (e.mirror = v)} />
  </div>

  <div class="buttons">
    {#if e.own}
      <button class="btn" title="Buttons held together" onclick={() => (e.popup = { kind: "chords" })}>
        <Icon name="link-simple" size={16} /> Chords{chordCount ? ` · ${chordCount}` : ""}
      </button>
    {:else}
      <button class="btn" title="Which vibration each hand plays" onclick={() => (e.popup = { kind: "haptics" })}><Icon name="vibrate" size={16} /> Haptics</button>
      <button class="btn" title="Where the game holds things in your hands" onclick={() => (e.popup = { kind: "poses" })}><Icon name="hand" size={16} /> Poses</button>
    {/if}
  </div>

  {#if e.own && e.set === "/actions/playspace"}
    <p class="hint"><Icon name="info" size={14} /> Hold to move the playspace, double press to snap it back</p>
  {:else if e.own && e.set === "/actions/mouse"}
    <p class="hint"><Icon name="info" size={14} /> While you point at a screen, and there first: a button used here does nothing else on that hand</p>
  {/if}
  {#if missing.length}
    <p class="hint warn"><Icon name="warning" size={14} /> Not on any input yet: {missing.join(", ")}</p>
  {/if}
</div>

<style>
  .center {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    overflow-y: auto;
    padding-top: 6px;
  }
  .arts {
    display: flex;
    justify-content: center;
    gap: 14px;
    width: 100%;
    height: min(300px, 42vh);
  }
  .art {
    position: relative;
    height: 100%;
    max-width: 48%;
  }
  .art img {
    width: 100%;
    height: 100%;
    display: block;
    opacity: 0.92;
    pointer-events: none;
  }
  .art img.flipped {
    transform: scaleX(-1);
  }
  .spot {
    position: absolute;
    aspect-ratio: 1;
    transform: translate(-50%, -50%);
    border-radius: 50%;
    border: 2px solid transparent;
    background: transparent;
    display: grid;
    place-items: center;
    padding: 0;
    transition:
      border-color 0.12s ease,
      background 0.12s ease,
      box-shadow 0.12s ease;
  }
  .spot.lit {
    border-color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.16);
    box-shadow: 0 0 0 6px hsl(var(--primary) / 0.1);
  }
  .letter {
    font-size: 10.5px;
    font-weight: 600;
    color: hsl(var(--muted));
  }
  .spot.lit .letter {
    color: hsl(var(--primary));
  }
  .mirror-row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: min(320px, 100%);
    padding: 9px 14px;
    border-radius: 12px;
    background: hsl(var(--surface));
    color: hsl(var(--muted));
  }
  .m-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    color: hsl(var(--foreground));
    font-size: 13.5px;
  }
  .m-text small {
    color: hsl(var(--muted));
    font-size: 11.5px;
  }
  .buttons {
    display: flex;
    gap: 10px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 36px;
    padding: 0 16px;
    border-radius: 10px;
    border: none;
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
    font-size: 13px;
  }
  .btn:hover {
    background: hsl(var(--border));
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: 12.5px;
    color: hsl(var(--muted));
    text-align: center;
    padding: 0 8px;
  }
  .hint.warn {
    color: hsl(var(--warn));
  }
</style>
