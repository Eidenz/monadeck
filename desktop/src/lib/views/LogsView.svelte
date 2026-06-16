<script lang="ts">
  import { onMount, tick } from "svelte";
  import { getLogs } from "$lib/api";

  let lines = $state<string[]>([]);
  let cursor = 0;
  let follow = $state(true);
  let box: HTMLDivElement;

  async function poll() {
    const chunk = await getLogs(cursor);
    if (chunk.lines.length) {
      lines = [...lines, ...chunk.lines].slice(-4000);
      cursor = chunk.cursor;
      if (follow) {
        await tick();
        box?.scrollTo({ top: box.scrollHeight });
      }
    } else {
      cursor = chunk.cursor;
    }
  }

  onMount(() => {
    poll();
    const t = setInterval(poll, 700);
    return () => clearInterval(t);
  });
</script>

<section class="logs">
  <div class="toolbar">
    <label class="follow">
      <input type="checkbox" bind:checked={follow} /> Follow
    </label>
    <button class="clear" onclick={() => (lines = [])}>Clear view</button>
  </div>
  <div class="box" bind:this={box}>
    {#if lines.length === 0}
      <p class="empty">No output yet. Logs appear here once the service is running.</p>
    {:else}
      {#each lines as line, i (i)}
        <div class="line">{line}</div>
      {/each}
    {/if}
  </div>
</section>

<style>
  .logs {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 14px;
    border-bottom: 1px solid hsl(var(--border) / 0.5);
    flex: none;
  }
  .follow {
    font-size: 12.5px;
    color: hsl(var(--muted));
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .clear {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 4px 10px;
    font-size: 12px;
  }
  .box {
    flex: 1;
    overflow: auto;
    padding: 10px 14px;
    font-family: "JetBrains Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 11.5px;
    line-height: 1.5;
    user-select: text;
  }
  .line {
    white-space: pre-wrap;
    word-break: break-word;
    color: hsl(var(--foreground) / 0.88);
  }
  .empty {
    color: hsl(var(--muted));
    font-family: inherit;
  }
</style>
