<script lang="ts">
  // Detached card that floats above the deck (SteamVR's "Update Permissions"
  // banner). Shown whenever the service binary lacks CAP_SYS_NICE=eip — e.g.
  // right after you rebuild your fork. `dismissed` is bindable so the deck can
  // collapse the window when it's hidden.
  import { app, applyCaps } from "$lib/state.svelte";

  let { dismissed = $bindable(false) }: { dismissed?: boolean } = $props();

  async function apply() {
    await applyCaps();
  }
</script>

<div class="toast" role="alert">
  <div class="title">Capabilities not set on the XR service</div>
  <div class="desc">
    Monado needs <code>CAP_SYS_NICE=eip</code> for proper scheduling. Required
    after each rebuild of your fork.
  </div>
  <div class="acts">
    <button class="btn ghost" onclick={() => (dismissed = true)}>Dismiss</button>
    <button class="btn primary" onclick={apply} disabled={app.busy}>
      {app.busy ? "Applying…" : "Set capabilities"}
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
