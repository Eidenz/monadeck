<script lang="ts">
  // Detached card above the deck (like ProtonBanner), shown when Monadeck, or
  // the Monado fork or xrizer that it installed itself, has a newer release.
  // "Later" puts off those releases only; the next one is offered again.
  // Self-built runtimes are never checked. Monadeck's own update is a download
  // (its release page): a package can't swap itself out while it runs.
  import { app, pendingUpdates, dismissUpdates, applyUpdates, downloadMonadeck } from "$lib/state.svelte";

  const updates = $derived(pendingUpdates());
  const runtimes = $derived(updates.filter((u) => u.kind !== "monadeck"));
  const own = $derived(updates.some((u) => u.kind === "monadeck"));
  const names = { monadeck: "Monadeck", monado: "Monado fork", xrizer: "xrizer" } as const;
  const title = $derived(
    updates.length === 1
      ? `${names[updates[0].kind]} update`
      : runtimes.length === updates.length
        ? "Runtime updates"
        : "Updates",
  );
</script>

<div class="toast" role="alert">
  <div class="title">{title}</div>
  <div class="desc">
    {#each updates as u (u.kind)}
      <div>{names[u.kind]} <b class="tag">{u.latest}</b> is out (you have <span class="tag">{u.installed}</span>).</div>
    {/each}
    {#if runtimes.length > 0 && app.service.running}
      <div class="later">{own ? "The runtime update takes" : "It takes"} effect the next time you start VR.</div>
    {/if}
  </div>
  {#if app.installResult && !app.installResult.ok}
    <div class="hint err">{app.installResult.msg}</div>
  {/if}
  <div class="acts">
    <button class="btn ghost" onclick={dismissUpdates} disabled={app.installing !== ""}>Later</button>
    <!-- With both kinds on offer, each button says which it acts on. -->
    {#if own}
      <button class={runtimes.length > 0 ? "btn ghost" : "btn primary"} onclick={downloadMonadeck}>
        {runtimes.length > 0 ? "Get Monadeck" : "Download"}
      </button>
    {/if}
    {#if runtimes.length > 0}
      <button class="btn primary" onclick={applyUpdates} disabled={app.installing !== ""}>
        {app.installing !== "" ? "Updating…" : own ? "Update runtimes" : "Update"}
      </button>
    {/if}
  </div>
</div>

<style>
  .toast {
    background: hsl(var(--surface) / 0.97);
    border: 1px solid hsl(var(--border) / 0.8);
    border-radius: var(--radius);
    padding: 12px 14px;
    box-shadow: 0 12px 30px hsl(0 0% 0% / 0.5);
    display: flex;
    flex-direction: column;
    gap: 7px;
  }
  .title {
    font-size: 13px;
    font-weight: 600;
    color: hsl(var(--foreground));
  }
  .desc {
    font-size: 12px;
    color: hsl(var(--muted));
    line-height: 1.45;
  }
  .desc b {
    color: hsl(var(--foreground));
    font-weight: 600;
  }
  .tag {
    white-space: nowrap;
  }
  .later {
    margin-top: 3px;
  }
  .hint.err {
    font-size: 11.5px;
    color: hsl(var(--danger));
  }
  .acts {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 3px;
  }
  .btn {
    border-radius: var(--radius-s);
    padding: 6px 14px;
    font-size: 12.5px;
    font-weight: 600;
    border: 1px solid transparent;
  }
  .ghost {
    background: hsl(var(--surface-2));
    border-color: hsl(var(--border));
    color: hsl(var(--foreground));
  }
  .ghost:hover {
    background: hsl(var(--surface-2) / 0.7);
  }
  .primary {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
  }
  .btn:disabled {
    opacity: 0.6;
  }
</style>
