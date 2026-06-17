<script lang="ts">
  import { onMount } from "svelte";
  import { gameCover, type DetectedGame } from "./api";

  let {
    game,
    active,
    onpick,
  }: { game: DetectedGame; active: boolean; onpick: () => void } = $props();

  let cover = $state<string | null>(null);
  onMount(async () => {
    if (game.appId) {
      try {
        cover = await gameCover(game.appId, game.gamePath);
      } catch {
        cover = null;
      }
    }
  });
</script>

<button class="tile state-layer" class:active onclick={onpick}>
  <div class="art">
    {#if cover}
      <img src={cover} alt="" />
    {:else}
      <span class="ph">{game.name.slice(0, 1).toUpperCase()}</span>
    {/if}
  </div>
  <div class="meta">
    <div class="name" title={game.name}>{game.name}</div>
    <div class="src">{game.source} · {game.bindingFiles.length} binding{game.bindingFiles.length === 1 ? "" : "s"}</div>
  </div>
</button>

<style>
  .tile {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-s);
    padding: 6px;
  }
  .tile.active {
    background: hsl(var(--primary) / 0.14);
    border-color: hsl(var(--primary) / 0.4);
  }
  .art {
    flex: none;
    width: 34px;
    height: 50px;
    border-radius: 4px;
    overflow: hidden;
    background: hsl(var(--surface-2));
    display: grid;
    place-items: center;
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .ph {
    font-size: 18px;
    font-weight: 700;
    color: hsl(var(--muted));
  }
  .meta {
    min-width: 0;
  }
  .name {
    font-size: 13px;
    color: hsl(var(--foreground));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .src {
    font-size: 10.5px;
    color: hsl(var(--muted));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
