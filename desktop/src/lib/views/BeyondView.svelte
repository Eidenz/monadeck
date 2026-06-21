<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    eyetrackingStatus,
    eyetrackingStart,
    eyetrackingStop,
    installBsbcams,
    installBsbcamsRule,
    setBsbcamsPath,
  } from "$lib/api";
  import type { EyeStatus } from "$lib/types";

  let status = $state<EyeStatus | null>(null);
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let poll: ReturnType<typeof setInterval> | undefined;

  async function refresh() {
    try {
      status = await eyetrackingStatus();
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    refresh();
    poll = setInterval(refresh, 1500);
  });
  onDestroy(() => clearInterval(poll));

  async function run(name: string, fn: () => Promise<unknown>) {
    busy = name;
    error = null;
    try {
      await fn();
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
      await refresh();
    }
  }

  const toggle = () =>
    run("toggle", () => (status?.running ? eyetrackingStop() : eyetrackingStart()));
  const download = () => run("download", () => installBsbcams());
  const grant = () => run("grant", () => installBsbcamsRule());

  async function choose() {
    const picked = await open({
      multiple: false,
      title: "Select the go-bsb-cams binary",
    });
    if (typeof picked === "string") await run("choose", () => setBsbcamsPath(picked));
  }

  const hasBinary = $derived(!!status?.binary);
  const streamUrl = $derived(`http://localhost:${status?.port ?? 8080}/stream`);
</script>

<section class="view">
  <h2>Beyond eye tracking</h2>
  <p class="lead">
    Runs the Beyond's eye-camera server (go-bsb-cams) so you don't need a terminal
    open for it. Point Babballonia / VRCFT at the stream below.
  </p>

  {#if status && !status.present}
    <div class="card warn">No Bigscreen Beyond detected. Plug it in to manage eye tracking.</div>
  {/if}

  <div class="card run">
    <div class="run-info">
      <span class="dot" class:on={status?.running}></span>
      <div>
        <div class="run-title">{status?.running ? "Eye tracking is running" : "Eye tracking is stopped"}</div>
        <div class="run-sub">Serving <code>{streamUrl}</code></div>
      </div>
    </div>
    <button
      class="btn primary"
      class:stop={status?.running}
      disabled={!hasBinary || busy === "toggle"}
      onclick={toggle}
    >
      {busy === "toggle" ? "…" : status?.running ? "Stop" : "Start"}
    </button>
  </div>

  <div class="card">
    <div class="row">
      <div>
        <div class="row-title">Camera access</div>
        <div class="row-sub">
          {#if status?.rule_installed}
            Granted. The cameras are readable without sudo.
          {:else}
            A udev rule is needed so the cameras can be read without sudo.
          {/if}
        </div>
      </div>
      {#if status?.rule_installed}
        <span class="ok-tag">Ready</span>
      {:else}
        <button class="btn" disabled={busy === "grant"} onclick={grant}>
          {busy === "grant" ? "…" : "Grant access"}
        </button>
      {/if}
    </div>
  </div>

  <div class="card">
    <div class="row">
      <div class="grow">
        <div class="row-title">go-bsb-cams binary</div>
        <div class="row-sub mono">
          {status?.binary ?? "Not found — download it or choose your own build."}
        </div>
      </div>
      <div class="actions">
        <button class="btn" disabled={busy === "download"} onclick={download}>
          {busy === "download" ? "…" : hasBinary ? "Update" : "Download"}
        </button>
        <button class="btn ghost" disabled={busy === "choose"} onclick={choose}>Choose…</button>
      </div>
    </div>
  </div>

  {#if error}
    <div class="card err">{error}</div>
  {/if}
</section>

<style>
  .view {
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h2 {
    margin: 0;
    font-size: 17px;
  }
  .lead {
    margin: 0;
    font-size: 12.5px;
    color: hsl(var(--muted));
    line-height: 1.55;
  }
  .card {
    background: hsl(var(--surface) / 0.6);
    border: 1px solid hsl(var(--border) / 0.7);
    border-radius: var(--radius);
    padding: 14px;
  }
  .card.warn {
    border-color: hsl(var(--warn) / 0.6);
    color: hsl(var(--warn));
    font-size: 12.5px;
  }
  .card.err {
    border-color: hsl(var(--danger) / 0.6);
    color: hsl(var(--danger));
    font-size: 12px;
    white-space: pre-wrap;
  }
  .run {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .run-info {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: hsl(var(--muted) / 0.6);
    flex: none;
  }
  .dot.on {
    background: hsl(var(--primary));
    box-shadow: 0 0 8px hsl(var(--primary) / 0.7);
  }
  .run-title {
    font-size: 14px;
    font-weight: 600;
  }
  .run-sub,
  .row-sub {
    font-size: 12px;
    color: hsl(var(--muted));
    margin-top: 2px;
  }
  code {
    font-family: ui-monospace, monospace;
    color: hsl(var(--foreground));
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .grow {
    min-width: 0;
    flex: 1;
  }
  .row-title {
    font-size: 13.5px;
    font-weight: 600;
  }
  .mono {
    font-family: ui-monospace, monospace;
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    gap: 8px;
    flex: none;
  }
  .btn {
    border: 1px solid hsl(var(--border));
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 8px 14px;
    font-size: 13px;
    cursor: pointer;
  }
  .btn:hover:not(:disabled) {
    border-color: hsl(var(--primary));
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .btn.ghost {
    background: transparent;
  }
  .btn.primary {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
    border-color: transparent;
    min-width: 92px;
    font-weight: 600;
  }
  .btn.primary.stop {
    background: hsl(var(--danger));
    color: white;
  }
  .ok-tag {
    font-size: 12px;
    font-weight: 600;
    color: hsl(var(--primary));
  }
</style>
