<script lang="ts">
  import { onMount, tick } from "svelte";
  import { getLogs } from "$lib/api";

  // monado emits one JSON object per line when XRT_JSON_LOG=1 (set on service
  // start): { level, file, func, message }. We parse those for color + level
  // filtering and fall back to showing any non-JSON line (third-party driver
  // output, the SteamVR wrapper, Vulkan loader, …) verbatim.
  type Level = "trace" | "debug" | "info" | "warn" | "error" | "raw";
  interface Entry {
    level: Level;
    text: string;
    where?: string;
  }

  let entries = $state<Entry[]>([]);
  let cursor = 0;
  let follow = $state(true);
  let minLevel = $state<"all" | "info" | "warn" | "error">("all");
  let box: HTMLDivElement;

  // Unparsed/raw lines rank as "info" so they stay visible unless you filter to
  // warnings/errors.
  const RANK: Record<Level, number> = {
    trace: 0,
    debug: 1,
    info: 2,
    raw: 2,
    warn: 3,
    error: 4,
  };
  const FILTER_RANK = { all: -1, info: 2, warn: 3, error: 4 } as const;
  const FILTERS = [
    { v: "all", lbl: "All" },
    { v: "info", lbl: "Info" },
    { v: "warn", lbl: "Warn" },
    { v: "error", lbl: "Error" },
  ] as const;

  function parse(line: string): Entry {
    if (line.startsWith("{") && line.endsWith("}")) {
      try {
        const o = JSON.parse(line);
        if (o && typeof o.message === "string" && typeof o.level === "string") {
          const lvl = String(o.level).toLowerCase();
          const level: Level =
            lvl === "trace" ||
            lvl === "debug" ||
            lvl === "info" ||
            lvl === "warn" ||
            lvl === "error"
              ? lvl
              : "raw";
          return {
            level,
            text: o.message,
            where: typeof o.func === "string" ? o.func : undefined,
          };
        }
      } catch {
        // not JSON after all — fall through to raw
      }
    }
    return { level: "raw", text: line };
  }

  const shown = $derived(
    entries.filter((e) => RANK[e.level] >= FILTER_RANK[minLevel]),
  );

  async function poll() {
    const chunk = await getLogs(cursor);
    cursor = chunk.cursor;
    if (chunk.lines.length) {
      entries = [...entries, ...chunk.lines.map(parse)].slice(-4000);
      if (follow) {
        await tick();
        box?.scrollTo({ top: box.scrollHeight });
      }
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
    <div class="levels">
      {#each FILTERS as f (f.v)}
        <button class:active={minLevel === f.v} onclick={() => (minLevel = f.v)}>
          {f.lbl}
        </button>
      {/each}
    </div>
    <button class="clear" onclick={() => (entries = [])}>Clear view</button>
  </div>
  <div class="box" bind:this={box}>
    {#if shown.length === 0}
      <p class="empty">
        {entries.length === 0
          ? "No output yet. Logs appear here once the service is running."
          : "No lines at this level."}
      </p>
    {:else}
      {#each shown as e, i (i)}
        <div class="line lvl-{e.level}" title={e.where ?? ""}>{e.text}</div>
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
  .levels {
    display: flex;
    gap: 4px;
  }
  .levels button {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--muted));
    border-radius: var(--radius-s);
    padding: 4px 10px;
    font-size: 12px;
  }
  .levels button.active {
    border-color: hsl(var(--primary));
    color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.12);
  }
  .clear {
    margin-left: auto;
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
  .lvl-error {
    color: hsl(var(--danger));
  }
  .lvl-warn {
    color: hsl(var(--warn));
  }
  .lvl-info {
    color: hsl(var(--foreground) / 0.9);
  }
  .lvl-debug,
  .lvl-trace {
    color: hsl(var(--muted));
  }
  .empty {
    color: hsl(var(--muted));
    font-family: inherit;
  }
</style>
