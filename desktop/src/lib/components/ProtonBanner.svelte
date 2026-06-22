<script lang="ts">
  // Detached card above the deck (like CapToast), shown when the Proton 11 / SLR4
  // OpenXR-import var isn't set yet. Harmless on Proton 10, so it doubles as a
  // "prepare ahead" nudge — most users are still on 10. The action is one click;
  // the install hint lives in Settings › Environment too.
  import { app, applyImportOpenxr } from "$lib/state.svelte";

  let { dismissed = $bindable(false) }: { dismissed?: boolean } = $props();
</script>

<div class="toast" role="alert">
  <div class="title">Proton OpenXR import not set</div>
  <div class="desc">
    Proton 11 / Steam Linux Runtime 4 need
    <code>PRESSURE_VESSEL_IMPORT_OPENXR_1_RUNTIMES</code> set for the whole session
    or OpenXR games won't launch in VR. Harmless on Proton 10 — set it now to be
    ready. Writes a config file that applies after a reboot.
  </div>
  {#if app.protonResult}
    <div class="hint" class:err={!app.protonResult.ok}>{app.protonResult.msg}</div>
  {/if}
  <div class="acts">
    <button class="btn ghost" onclick={() => (dismissed = true)}>Dismiss</button>
    <button
      class="btn primary"
      onclick={applyImportOpenxr}
      disabled={app.applyingProton}
    >
      {app.applyingProton ? "Writing…" : "Create config file"}
    </button>
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
  code {
    background: hsl(var(--background) / 0.7);
    padding: 0 4px;
    border-radius: 4px;
    font-size: 11px;
    word-break: break-all;
  }
  .hint {
    font-size: 11.5px;
    color: hsl(var(--ok));
  }
  .hint.err {
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
  .primary:disabled {
    opacity: 0.6;
  }
</style>
