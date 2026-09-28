<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  let { title, sub = "", width = 560, onclose, children }: { title: string; sub?: string; width?: number; onclose: () => void; children: Snippet } = $props();

  function onkey(ev: KeyboardEvent) {
    if (ev.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="dim" role="presentation" onclick={onclose}></div>
<div class="card" role="dialog" aria-label={title} style:width="{width}px">
  <div class="top">
    <div>
      <h2>{title}</h2>
      {#if sub}<p>{sub}</p>{/if}
    </div>
    <button class="close" aria-label="Close" onclick={onclose}><Icon name="x" size={16} /></button>
  </div>
  <div class="body">
    {@render children()}
  </div>
</div>

<style>
  .dim {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.55);
    z-index: 60;
  }
  .card {
    position: fixed;
    top: 64px;
    left: 50%;
    transform: translateX(-50%);
    max-width: calc(100vw - 40px);
    max-height: calc(100vh - 100px);
    display: flex;
    flex-direction: column;
    background: hsl(var(--surface));
    border: 1px solid hsl(var(--foreground) / 0.08);
    border-radius: 18px;
    box-shadow: 0 18px 44px hsl(0 0% 0% / 0.55);
    z-index: 61;
    padding: 18px 18px 16px;
  }
  .top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 12px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }
  p {
    margin: 3px 0 0;
    font-size: 12.5px;
    color: hsl(var(--muted));
  }
  .close {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 9px;
    border: none;
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
    flex: none;
  }
  .close:hover {
    background: hsl(var(--border));
  }
  .body {
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-right: 4px;
  }
</style>
