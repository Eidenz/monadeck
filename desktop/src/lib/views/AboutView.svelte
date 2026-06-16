<script lang="ts">
  import { app } from "$lib/state.svelte";
</script>

<section class="view">
  <h2>About</h2>
  <div class="card">
    <div class="brand">Monad<b>eck</b> <span class="ver">v{app.version}</span></div>
    <p>
      A compact, SteamVR-style launcher for a custom Monado build — start/stop the
      XR service with your env vars, register xrizer, set capabilities, watch your
      devices via libmonado, and launch plugins by explicit path.
    </p>
    <div class="rows">
      <div class="kv"><span>OpenXR runtime</span><b>{app.runtime.openxr}</b></div>
      <div class="kv"><span>OpenVR runtime</span><b>{app.runtime.openvr}</b></div>
      <div class="kv"><span>Service</span><b>{app.service.connected ? "connected" : app.service.running ? "starting" : "stopped"}</b></div>
    </div>
  </div>
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
  .card {
    background: hsl(var(--surface) / 0.6);
    border: 1px solid hsl(var(--border) / 0.7);
    border-radius: var(--radius);
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .brand {
    font-size: 18px;
    color: hsl(var(--primary));
  }
  .brand b {
    color: hsl(var(--foreground));
  }
  .ver {
    font-size: 12px;
    color: hsl(var(--muted));
  }
  p {
    margin: 0;
    font-size: 12.5px;
    color: hsl(var(--muted));
    line-height: 1.55;
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .kv {
    display: flex;
    justify-content: space-between;
    font-size: 12.5px;
    border-top: 1px solid hsl(var(--border) / 0.5);
    padding-top: 6px;
  }
  .kv span {
    color: hsl(var(--muted));
  }
</style>
