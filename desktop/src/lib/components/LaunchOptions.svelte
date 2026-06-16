<script lang="ts">
  import { app } from "$lib/state.svelte";
  import { steamLaunchOptions } from "$lib/launchOptions";

  const opts = $derived(steamLaunchOptions(app.config));
  let copied = $state(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(opts);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Clipboard blocked — the field is selectable as a fallback.
    }
  }
</script>

<div class="lo glass">
  <div class="lo-head">
    <div class="lo-meta">
      <div class="lo-title">Steam launch options</div>
      <div class="lo-sub">
        Paste into a game's <b>Properties → Launch Options</b> so it picks up
        monado + xrizer through the Proton sandbox.
      </div>
    </div>
    <button class="copy" onclick={copy}>{copied ? "Copied ✓" : "Copy"}</button>
  </div>
  <code class="lo-str">{opts}</code>
</div>

<style>
  .lo {
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .lo-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .lo-title {
    font-size: 13px;
    font-weight: 600;
  }
  .lo-sub {
    font-size: 11.5px;
    color: hsl(var(--muted));
    margin-top: 2px;
  }
  .copy {
    flex: none;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 600;
  }
  .copy:hover {
    border-color: hsl(var(--primary));
    color: hsl(var(--primary));
  }
  .lo-str {
    display: block;
    background: hsl(var(--background) / 0.7);
    border: 1px solid hsl(var(--border) / 0.6);
    border-radius: var(--radius-s);
    padding: 9px 11px;
    font-family: ui-monospace, "JetBrains Mono", monospace;
    font-size: 11px;
    line-height: 1.5;
    color: hsl(var(--foreground) / 0.92);
    word-break: break-all;
    user-select: text;
  }
</style>
