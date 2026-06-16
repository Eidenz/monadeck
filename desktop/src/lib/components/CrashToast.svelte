<script lang="ts">
  // Floats above the deck when the service stopped without us asking (crash or
  // failed startup). "View logs" opens the settings window on the Logs section.
  import { app } from "$lib/state.svelte";
  import { openSettings } from "$lib/windows";

  const codeText = $derived(
    app.crash?.code != null ? ` (exit code ${app.crash.code})` : "",
  );
</script>

<div class="toast" role="alert">
  <div class="title">Monado service stopped unexpectedly{codeText}</div>
  <div class="desc">
    It exited without you pressing Stop — likely a crash or a failed startup
    (e.g. HMD/driver). The logs have the details.
  </div>
  <div class="acts">
    <button class="btn ghost" onclick={() => (app.crash = null)}>Dismiss</button>
    <button class="btn primary" onclick={() => openSettings("logs")}>View logs</button>
  </div>
</div>

<style>
  .toast {
    background: hsl(var(--surface) / 0.97);
    border: 1px solid hsl(var(--danger) / 0.5);
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
  .primary {
    background: hsl(var(--danger) / 0.9);
    color: white;
  }
</style>
