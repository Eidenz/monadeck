<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app } from "$lib/state.svelte";

  // Mirrors the README's Credits section: keep the two in step.
  const credits: { name: string; url: string; by?: string; what: string }[] = [
    { name: "Monado", url: "https://gitlab.freedesktop.org/monado/monado", what: "the OpenXR runtime all of this drives" },
    { name: "xrizer", url: "https://github.com/Supreeeme/xrizer", by: "Supreeeme", what: "OpenVR on OpenXR" },
    { name: "WiVRn", url: "https://github.com/WiVRn/WiVRn", by: "Guillaume Meunier and Patrick Nicolas", what: "standalone-headset streaming, driven through its D-Bus interface" },
    { name: "WayVR", url: "https://github.com/wlx-team/wayvr", by: "galister and the wlx team", what: "the desktop overlay, watch and playspace drag this borrows from" },
    { name: "libmonado-rs", url: "https://github.com/wayvr-org/libmonado-rs", by: "the WayVR organisation", what: "the libmonado bindings" },
    { name: "UEVR", url: "https://github.com/praydog/UEVR", by: "praydog", what: "VR for flat Unreal Engine games" },
    { name: "chihuahua", url: "https://github.com/keton/chihuahua", by: "keton", what: "the UEVR injector" },
    { name: "protontricks", url: "https://github.com/Matoking/protontricks", by: "Matoking", what: "runs the injector inside a game's Proton prefix" },
    { name: "go-bsb-cams", url: "https://github.com/Red-M/go-bsb-cams", by: "Red-M", what: "Bigscreen Beyond eye cameras" },
    { name: "Envision", url: "https://gitlab.com/gabmus/envision", by: "GabMus", what: "for the ground it covered first" },
    { name: "Table Mountain 2", url: "https://polyhaven.com/a/table_mountain_2", by: "Greg Zaal, Poly Haven", what: "the default background (CC0)" },
    { name: "Phosphor", url: "https://phosphoricons.com", what: "the icons" },
  ];

  // The settings window is a webview: a plain link would navigate it away.
  function open(url: string) {
    openUrl(url).catch(() => {});
  }
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

  <div class="card">
    <div class="sect">Made by</div>
    <div class="author">
      <button class="link strong" onclick={() => open("https://github.com/Eidenz")}>Eidenz</button>
      <span class="dot">·</span>
      <button class="link" onclick={() => open("https://github.com/Eidenz/monadeck")}>github.com/Eidenz/monadeck</button>
      <span class="dot">·</span>
      <span class="lic">MIT licence</span>
    </div>
  </div>

  <div class="card">
    <div class="sect">Credits</div>
    <p>Monadeck stands on other people's work. Thank you to:</p>
    <ul class="credits">
      {#each credits as c (c.name)}
        <li>
          <button class="link strong" onclick={() => open(c.url)} title={c.url}>{c.name}</button>
          {#if c.by}<span class="by">by {c.by}</span>{/if}
          <span class="what">{c.what}</span>
        </li>
      {/each}
    </ul>
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
  .sect {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: hsl(var(--muted));
  }
  .author {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 6px;
    font-size: 13px;
  }
  .dot,
  .lic {
    color: hsl(var(--muted));
    font-size: 12.5px;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    font-size: 12.5px;
    color: hsl(var(--primary));
    cursor: pointer;
    text-align: left;
  }
  .link:hover {
    text-decoration: underline;
  }
  .link.strong {
    font-weight: 600;
    font-size: 13px;
  }
  .credits {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .credits li {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 3px 7px;
    padding: 7px 0;
    border-top: 1px solid hsl(var(--border) / 0.5);
  }
  .by {
    font-size: 12.5px;
    color: hsl(var(--foreground));
  }
  .what {
    font-size: 12px;
    color: hsl(var(--muted));
    flex-basis: 100%;
  }
</style>
