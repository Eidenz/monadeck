<script lang="ts" module>
  // Covers are data URLs from Rust; fetched once each, when a tile scrolls in.
  const covers = new Map<string, Promise<string | null>>();
  function cover(id: string): Promise<string | null> {
    let p = covers.get(id);
    if (!p) {
      p = import("./api").then((api) => api.gameCover(id)).catch(() => null);
      covers.set(id, p);
    }
    return p;
  }
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { Game } from "./api";

  let { game, opening, onpick }: { game: Game; opening: boolean; onpick: () => void } = $props();
  let src = $state<string | null>(null);
  let el: HTMLElement;

  $effect(() => {
    const id = game.coverId;
    if (!id) return;
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        io.disconnect();
        cover(id).then((u) => (src = u));
      }
    });
    io.observe(el);
    return () => io.disconnect();
  });

  // A name-seeded tint for games without art.
  const TINTS = ["210 30% 22%", "275 25% 21%", "170 30% 17%", "30 32% 19%", "335 27% 20%", "130 20% 18%"];
  const tint = $derived(TINTS[[...game.name].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7) % TINTS.length]);
</script>

<button class="tile" bind:this={el} onclick={onpick} title={game.name}>
  <span class="art" style:--tint={tint}>
    {#if src}
      <img {src} alt="" />
    {:else}
      <span class="ph"><Icon name="game-controller" size={26} /><span>{game.name}</span></span>
    {/if}
    {#if game.personal.length}
      <span class="badge"><Icon name="sparkle" size={11} /> Customized</span>
    {/if}
    {#if opening}
      <span class="busy"><span class="spinner"></span></span>
    {/if}
  </span>
  <span class="name">{game.name}</span>
</button>

<style>
  .tile {
    display: flex;
    flex-direction: column;
    gap: 7px;
    background: transparent;
    border: none;
    padding: 0;
    color: hsl(var(--muted));
    text-align: left;
    min-width: 0;
  }
  .art {
    position: relative;
    display: block;
    aspect-ratio: 2 / 3;
    border-radius: 12px;
    overflow: hidden;
    background: linear-gradient(180deg, hsl(var(--tint)), hsl(var(--tint) / 0.55));
    border: 1px solid hsl(var(--foreground) / 0.06);
    transition:
      transform 0.14s ease,
      box-shadow 0.14s ease,
      border-color 0.14s ease;
  }
  .tile:hover .art {
    transform: translateY(-2px);
    box-shadow: 0 10px 22px hsl(0 0% 0% / 0.45);
    border-color: hsl(var(--foreground) / 0.25);
  }
  .tile:hover {
    color: hsl(var(--foreground));
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .ph {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 12px;
    text-align: center;
    color: hsl(var(--foreground) / 0.85);
    font-size: 13px;
  }
  .ph :global(.icon) {
    opacity: 0.4;
  }
  .badge {
    position: absolute;
    left: 8px;
    bottom: 8px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: hsl(var(--primary));
    background: hsl(0 0% 0% / 0.7);
    border: 1px solid hsl(var(--primary) / 0.4);
    border-radius: 99px;
    padding: 2px 8px;
  }
  .busy {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    background: hsl(0 0% 0% / 0.55);
  }
  .spinner {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 2px solid hsl(0 0% 100% / 0.25);
    border-top-color: white;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .name {
    font-size: 12.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    padding: 0 2px;
  }
</style>
