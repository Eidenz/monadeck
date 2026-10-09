<script lang="ts">
  // First-run welcome, in its own window before the deck ever shows. One step
  // open at a time along a rail that grows out of the logo; each reuses the same
  // status + action as the deck's notices, so anything skipped here comes back
  // there as a reminder. Finishing (or skipping it all) hands over to the deck.
  import { onMount } from "svelte";
  import { slide } from "svelte/transition";
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    app,
    loadInitial,
    saveConfig,
    refreshStatus,
    refreshPreflight,
    refreshSteamvr,
    refreshImportOpenxr,
    installMonado,
    applyCaps,
    installUdevRules,
    applyImportOpenxr,
  } from "$lib/state.svelte";
  import { autodetectWivrn, finishWelcome } from "$lib/api";
  import WindowControls from "./WindowControls.svelte";
  import Toggle from "./Toggle.svelte";

  const isWivrn = $derived(app.config?.backend === "wivrn");
  // SteamVR's tracking driver only matters for Lighthouse headsets on Monado.
  // The floor is set from the deck once VR runs (it reads the headset's pose).
  const lighthouse = $derived(!isWivrn && app.config?.lighthouse_driver === "steamvr");

  let chosen = $state(false);
  let skipped = $state<Record<string, boolean>>({});

  type Step = { id: string; title: string; done: boolean; summary: string };
  const steps = $derived<Step[]>([
    { id: "backend", title: "Your headset", done: chosen, summary: isWivrn ? "WiVRn" : "Monado" },
    {
      id: "runtime",
      title: isWivrn ? "WiVRn server" : "Monado runtime",
      done: app.caps !== "no_binary",
      summary: "Found",
    },
    ...(isWivrn
      ? []
      : [{ id: "caps", title: "Service capabilities", done: app.caps === "set" || app.caps === "not_needed", summary: "Set" }]),
    { id: "prereq", title: "System prerequisites", done: !!app.preflight?.all_ok, summary: "Ready" },
    { id: "proton", title: "Proton OpenXR import", done: app.importOpenxr, summary: "Set" },
    ...(lighthouse
      ? [{ id: "steamvr", title: "SteamVR's tracking driver", done: app.steamvr === true, summary: "Found" }]
      : []),
    { id: "finish", title: "Ready to go", done: false, summary: "" },
  ]);

  // The open step: the first one left, then the next one each time a step is
  // done or skipped. Done and skipped steps fold up and can be reopened.
  let active = $state("backend");
  const passed = (s: Step) => s.done || !!skipped[s.id];
  function nextAfter(id: string) {
    const i = steps.findIndex((s) => s.id === id);
    return steps.slice(i + 1).find((s) => !passed(s))?.id ?? "finish";
  }
  function skip(id: string) {
    skipped[id] = true;
    active = nextAfter(id);
  }
  let wasDone: Record<string, boolean> = {};
  $effect(() => {
    const now = Object.fromEntries(steps.map((s) => [s.id, s.done]));
    if (now[active] && wasDone[active] === false) active = nextAfter(active);
    wasDone = now;
  });
  // Every step depends on the runtime, so that one comes first.
  const reachable = (id: string) => id === "backend" || chosen;

  async function setBackend(b: "monado" | "wivrn") {
    if (app.config && app.config.backend !== b) {
      app.config.backend = b;
      await saveConfig();
      await refreshStatus();
    }
    chosen = true;
    active = nextAfter("backend");
  }

  // Where's the runtime: look again (WiVRn may have just been installed), or
  // point at a build / binary by hand.
  async function recheckRuntime() {
    if (isWivrn && app.config && !app.config.wivrn_server_path) {
      const found = await autodetectWivrn().catch(() => null);
      if (found) {
        app.config.wivrn_server_path = found;
        await saveConfig();
      }
    }
    await refreshStatus();
  }
  async function locateRuntime() {
    if (!app.config) return;
    const picked = await open({ directory: !isWivrn, multiple: false });
    if (typeof picked !== "string") return;
    if (isWivrn) app.config.wivrn_server_path = picked;
    else app.config.monado_prefix = picked;
    await saveConfig();
    await refreshStatus();
  }

  const missing = $derived(app.preflight?.checks.filter((c) => !c.ok) ?? []);
  const killSteamvr = $derived(app.config?.kill_steamvr_on_start ?? true);
  const leftovers = $derived(steps.filter((s) => s.id !== "finish" && !s.done).length);

  let finishing = $state(false);
  async function finish() {
    finishing = true;
    try {
      await finishWelcome();
    } catch (e) {
      app.error = String(e);
      finishing = false;
    }
  }

  // --- Intro: the logo lands at the head of the step rail, which then grows
  // out of it, popping each step's node along the way.
  let logoEl = $state<HTMLElement>();
  let revealed = $state(false);
  // After the intro, steps added later (another runtime's) just appear.
  let settled = $state(false);
  $effect(() => {
    if (!revealed) return;
    const t = setTimeout(() => (settled = true), 3500);
    return () => clearTimeout(t);
  });
  onMount(() => {
    loadInitial().then(() => {
      wasDone = {};
      active = steps.find((s) => !s.done && reachable(s.id))?.id ?? "backend";
    });
    const poll = setInterval(() => {
      refreshStatus();
      refreshPreflight();
      refreshSteamvr();
      refreshImportOpenxr();
    }, 2000);
    let t: ReturnType<typeof setTimeout> | undefined;
    if (!logoEl || matchMedia("(prefers-reduced-motion: reduce)").matches) {
      revealed = true;
    } else {
      const r = logoEl.getBoundingClientRect();
      const dx = innerWidth / 2 - (r.left + r.width / 2);
      const dy = innerHeight / 2 - 30 - (r.top + r.height / 2);
      const away = `translate(${dx}px, ${dy}px)`;
      const glow = "0 0 90px 0 rgba(54, 205, 214, 0.4)";
      logoEl.animate(
        [
          { transform: `${away} scale(0.3)`, opacity: 0, boxShadow: "none" },
          { transform: `${away} scale(2.4)`, opacity: 1, boxShadow: glow, offset: 0.3 },
          { transform: `${away} scale(2.4)`, opacity: 1, boxShadow: glow, offset: 0.5 },
          { transform: "none", opacity: 1 },
        ],
        { duration: 1700, easing: "cubic-bezier(0.3, 0.7, 0.2, 1)", fill: "backwards" },
      );
      t = setTimeout(() => (revealed = true), 1250);
    }
    return () => {
      clearInterval(poll);
      clearTimeout(t);
    };
  });

  // Headset glyph (the deck's), wired for Monado or with Wi-Fi arcs for WiVRn.
  const VISOR =
    "M4 8.5h16a2 2 0 0 1 2 2v3.2a3 3 0 0 1-3 3h-2.4a2 2 0 0 1-1.7-.95L13.6 14.8a2 2 0 0 0-3.2 0l-1.3 1.95A2 2 0 0 1 7.4 17.7H5a3 3 0 0 1-3-3V10.5a2 2 0 0 1 2-2z";
  const CHOICES = [
    { id: "monado" as const, name: "Monado", sub: "Wired headsets: Lighthouse and others", extra: "M20.5 15.8c.6 2.6-.6 4.7-3.6 4.7H14" },
    {
      id: "wivrn" as const,
      name: "WiVRn",
      sub: "Standalone headsets over Wi-Fi: Quest, Pico…",
      extra: "M8.2 5.4a5.6 5.6 0 0 1 7.6 0 M10.2 7.2a2.6 2.6 0 0 1 3.6 0",
    },
  ];
</script>

<div class="welcome" class:revealed class:settled>
  <header class="top" data-tauri-drag-region>
    <button class="btn ghost skipall" onclick={finish} disabled={finishing}>Skip setup</button>
    <WindowControls />
  </header>

  <div class="scroll">
    <div class="col">
      <div class="hero">
        <div class="logo" bind:this={logoEl} aria-hidden="true">
          <svg viewBox="0 0 1024 1024" width="60" height="60">
            <defs>
              <linearGradient id="wl-bg" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stop-color="#222a31" />
                <stop offset="1" stop-color="#11171c" />
              </linearGradient>
              <radialGradient id="wl-glow" cx="0.3" cy="0.02" r="0.95">
                <stop offset="0" stop-color="#2b4d55" stop-opacity="0.8" />
                <stop offset="0.6" stop-color="#2b4d55" stop-opacity="0" />
              </radialGradient>
              <linearGradient id="wl-grad" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stop-color="#36cdd6" />
                <stop offset="1" stop-color="#30c47f" />
              </linearGradient>
            </defs>
            <rect width="1024" height="1024" rx="232" fill="url(#wl-bg)" />
            <rect width="1024" height="1024" rx="232" fill="url(#wl-glow)" />
            <g transform="translate(512,468) scale(33) translate(-12,-13.1)"><path d={VISOR} fill="url(#wl-grad)" /></g>
            <rect x="320" y="662" width="384" height="34" rx="17" fill="url(#wl-grad)" opacity="0.92" />
          </svg>
        </div>
        <div class="titles">
          <h1>Welcome to Monadeck</h1>
          <p>A few one-time steps to get VR going. Skip any of them: the deck reminds you later.</p>
        </div>
      </div>

      <ol class="steps">
        {#each steps as st, i (st.id)}
          {@const isOpen = st.id === active}
          <li class="step" class:open={isOpen} class:done={st.done} style="--i:{i}">
            <span class="node" aria-hidden="true">
              {#if st.done && !isOpen}
                <svg viewBox="0 0 24 24" width="14" height="14"><path d="M20 6 9 17l-5-5" /></svg>
              {:else}{i + 1}{/if}
            </span>
            <div class="sbody">
              <button class="shead" disabled={isOpen || !reachable(st.id)} onclick={() => (active = st.id)}>
                <span class="stitle">{st.title}</span>
                {#if !isOpen && st.done}
                  <span class="chip" class:good={st.id !== "backend"}>{st.summary}</span>
                {:else if !isOpen && skipped[st.id]}
                  <span class="chip">Skipped</span>
                {/if}
              </button>

              {#if isOpen}
                <div class="card" transition:slide={{ duration: 220 }}>
                  {#if st.id === "backend"}
                    <p class="desc">Which headset do you use? You can switch later in Settings.</p>
                    <div class="choices">
                      {#each CHOICES as c}
                        <button
                          class="choice"
                          class:selected={chosen && app.config?.backend === c.id}
                          disabled={app.service.running}
                          onclick={() => setBackend(c.id)}
                        >
                          <svg class="glyph" viewBox="0 0 24 24" width="44" height="44" aria-hidden="true">
                            <path d={VISOR} />
                            <path class="extra" d={c.extra} />
                          </svg>
                          <span class="ctext"><b>{c.name}</b><span>{c.sub}</span></span>
                        </button>
                      {/each}
                    </div>
                  {:else if st.id === "runtime"}
                    {#if st.done}
                      <p class="desc">
                        {isWivrn ? "wivrn-server" : "monado-service"} found{#if isWivrn ? app.config?.wivrn_server_path : app.config?.monado_prefix}
                          in <code>{isWivrn ? app.config?.wivrn_server_path : app.config?.monado_prefix}</code>{/if}.
                      </p>
                    {:else if isWivrn}
                      <p class="desc">
                        Install WiVRn from your distribution (package <code>wivrn</code>). Monadeck finds it on its own.
                      </p>
                      <div class="row">
                        <button class="btn primary" onclick={recheckRuntime}>Check again</button>
                        <button class="btn ghost" onclick={locateRuntime}>Locate wivrn-server…</button>
                      </div>
                    {:else}
                      <p class="desc">
                        Monadeck can install its own Monado build, with the drivers it relies on. Or point it at one you
                        built.
                      </p>
                      <div class="row">
                        <button class="btn primary" onclick={installMonado} disabled={app.installing !== ""}>
                          {app.installing === "monado" ? "Installing…" : "Install Monado"}
                        </button>
                        <button class="btn ghost" onclick={locateRuntime} disabled={app.installing !== ""}>Use my own build…</button>
                      </div>
                      {#if app.installResult?.kind === "monado"}
                        <p class="note" class:bad={!app.installResult.ok}>{app.installResult.msg}</p>
                      {/if}
                    {/if}
                  {:else if st.id === "caps"}
                    <p class="desc">
                      Lets Monado's compositor run at real-time priority, for steady frames. Asks for your password; redo it
                      after updating Monado.
                    </p>
                    <div class="row">
                      {#if st.done}
                        <span class="chip good">Set</span>
                      {:else if app.caps === "needs_setcap"}
                        <button class="btn primary" onclick={applyCaps} disabled={app.busy}>{app.busy ? "Setting…" : "Set"}</button>
                        <button class="btn ghost" onclick={() => skip(st.id)}>Skip</button>
                      {:else}
                        <span class="note">{app.caps === "no_binary" ? "Comes after the runtime." : "Needs getcap and setcap (package libcap)."}</span>
                        <button class="btn ghost" onclick={() => skip(st.id)}>Skip</button>
                      {/if}
                    </div>
                  {:else if st.id === "prereq"}
                    {#if st.done}
                      <p class="desc">Everything's in place.</p>
                    {:else}
                      <p class="desc">Outside Monadeck's control. The commands are best-effort hints{app.preflight?.distro ? ` for ${app.preflight.distro}` : ""}.</p>
                      <div class="checks">
                        {#each missing as c (c.id)}
                          <div class="check">
                            <div class="ctitle">
                              <span class="dot" class:imp={c.severity === "important"}></span>{c.label}
                              <span class="sev">{c.severity === "important" ? "Missing" : "Optional"}</span>
                            </div>
                            <div class="cdetail">{c.detail}</div>
                            {#if c.action === "install_udev_rules"}
                              <div class="row">
                                <button class="btn primary sm" onclick={installUdevRules} disabled={app.installingUdev}>
                                  {app.installingUdev ? "Installing…" : "Install the rules"}
                                </button>
                                {#if c.fix}<span class="note">or with your package manager:</span>{/if}
                              </div>
                            {/if}
                            {#if c.fix}<code class="fix">{c.fix}</code>{/if}
                          </div>
                        {/each}
                      </div>
                      {#if app.udevResult && !app.udevResult.ok}<p class="note bad">{app.udevResult.msg}</p>{/if}
                      <div class="row">
                        <button class="btn primary" onclick={refreshPreflight}>Check again</button>
                        <button class="btn ghost" onclick={() => skip(st.id)}>Skip</button>
                      </div>
                    {/if}
                  {:else if st.id === "proton"}
                    <p class="desc">
                      Lets games on Proton 11 find the VR runtime, and changes nothing on older Proton. Applies after a
                      reboot or a new login.
                    </p>
                    <div class="row">
                      {#if st.done}
                        <span class="chip good">Set</span>
                      {:else}
                        <button class="btn primary" onclick={applyImportOpenxr} disabled={app.applyingProton}>
                          {app.applyingProton ? "Writing…" : "Create the file"}
                        </button>
                        <button class="btn ghost" onclick={() => skip(st.id)}>Skip</button>
                      {/if}
                    </div>
                    {#if app.protonResult && !app.protonResult.ok}<p class="note bad">{app.protonResult.msg}</p>{/if}
                  {:else if st.id === "steamvr"}
                    {#if st.done}
                      <p class="desc">SteamVR is installed. It never runs: Monado only loads its Lighthouse tracking driver.</p>
                    {:else}
                      <p class="desc">Lighthouse headsets track through SteamVR's driver. Install SteamVR from Steam; it never needs to run.</p>
                      <div class="row">
                        <button class="btn primary" onclick={refreshSteamvr}>Check again</button>
                        <button class="btn ghost" onclick={() => skip(st.id)}>Skip</button>
                      </div>
                    {/if}
                  {:else}
                    <p class="desc">
                      {leftovers === 0
                        ? "All set. Start VR from the deck."
                        : "Start VR from the deck. What's left shows up there as a reminder."}
                      {#if lighthouse && app.floorCal && !app.floorCal.calibrated}
                        The first time VR runs, it also asks you to set your floor.
                      {/if}
                    </p>
                    <div class="pref">
                      <div>
                        <div class="ptitle">Stop SteamVR on start</div>
                        <div class="psub">
                          {isWivrn
                            ? "A running SteamVR holds the OpenVR runtime your games need."
                            : "SteamVR fights Monado for the headset."} Leave on unless you need SteamVR running.
                        </div>
                      </div>
                      <Toggle
                        label="Stop SteamVR on start"
                        checked={killSteamvr}
                        onchange={(v) => {
                          if (app.config) {
                            app.config.kill_steamvr_on_start = v;
                            saveConfig();
                          }
                        }}
                      />
                    </div>
                    <button class="btn primary big" onclick={finish} disabled={finishing}>Open Monadeck</button>
                  {/if}
                </div>
              {/if}
            </div>
          </li>
        {/each}
      </ol>

      {#if app.error}
        <div class="err" role="alert">
          <span>{app.error}</span>
          <button aria-label="Dismiss" onclick={() => (app.error = "")}>✕</button>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .welcome {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background:
      radial-gradient(70% 50% at 50% -10%, hsl(185 45% 22% / 0.45), transparent 70%),
      hsl(var(--background));
    color: hsl(var(--foreground));
  }
  .top {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    height: 44px;
    padding: 6px 8px 6px 12px;
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
  }
  .col {
    width: min(600px, 100% - 48px);
    margin: 0 auto;
    padding: max(8px, 3vh) 0 40px;
  }

  /* The rail: logo, then one node per step, joined by a line down the first column. */
  .hero,
  .step {
    position: relative;
    display: grid;
    grid-template-columns: 60px minmax(0, 1fr);
    column-gap: 18px;
  }
  .hero {
    align-items: center;
    padding-bottom: 28px;
  }
  .hero::after,
  .step::before {
    content: "";
    position: absolute;
    left: 29px;
    width: 2px;
    background: hsl(var(--border));
    transform-origin: top;
  }
  .hero::after {
    top: 68px;
    bottom: 0;
  }
  .step::before {
    top: 0;
    bottom: 0;
  }
  .step:last-child::before {
    bottom: auto;
    height: 15px;
  }
  .logo {
    position: relative;
    z-index: 2;
    width: 60px;
    height: 60px;
    border-radius: 14px;
  }
  .logo svg {
    display: block;
  }
  h1 {
    margin: 0;
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .titles p {
    margin: 4px 0 0;
    font-size: 13px;
    line-height: 1.45;
    color: hsl(var(--muted));
  }

  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .step {
    padding-bottom: 16px;
  }
  .node {
    position: relative;
    z-index: 1;
    justify-self: center;
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: hsl(var(--background));
    box-shadow: inset 0 0 0 1.5px hsl(var(--border));
    color: hsl(var(--muted));
    font-size: 12.5px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    transition: background 0.2s ease, color 0.2s ease, box-shadow 0.2s ease;
  }
  .node svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 3;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .step.done .node {
    background: hsl(var(--ok) / 0.16);
    box-shadow: none;
    color: hsl(var(--ok));
  }
  .step.open .node {
    background: hsl(var(--primary));
    box-shadow: 0 0 0 5px hsl(var(--primary) / 0.18);
    color: hsl(var(--primary-fg));
  }
  .shead {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 30px;
    padding: 0;
    background: none;
    border: none;
    text-align: left;
    color: inherit;
    font: inherit;
  }
  .shead:disabled {
    cursor: default;
  }
  .stitle {
    font-size: 14.5px;
    font-weight: 600;
    color: hsl(var(--foreground) / 0.8);
  }
  .step.open .stitle,
  .shead:not(:disabled):hover .stitle {
    color: hsl(var(--foreground));
  }
  .step:not(.done):not(.open) .stitle {
    color: hsl(var(--muted));
  }
  .chip {
    padding: 2px 9px;
    border-radius: 99px;
    background: hsl(var(--surface-2));
    color: hsl(var(--muted));
    font-size: 11px;
    font-weight: 600;
  }
  .chip.good {
    background: hsl(var(--ok) / 0.14);
    color: hsl(var(--ok));
  }
  .card {
    margin-top: 10px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    background: hsl(var(--surface) / 0.9);
    border: 1px solid hsl(var(--border) / 0.7);
    border-radius: var(--radius-l);
  }
  .desc {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: hsl(var(--foreground) / 0.78);
  }
  .note {
    margin: 0;
    font-size: 12px;
    color: hsl(var(--muted));
  }
  .note.bad {
    color: hsl(var(--danger));
  }
  code {
    padding: 1px 5px;
    border-radius: 4px;
    background: hsl(var(--background) / 0.8);
    font-family: ui-monospace, monospace;
    font-size: 12px;
    user-select: text;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .btn {
    padding: 7px 14px;
    border-radius: var(--radius-s);
    border: 1px solid transparent;
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
  }
  .btn.sm {
    padding: 5px 11px;
  }
  .btn.big {
    padding: 9px 18px;
    font-size: 13px;
  }
  .btn.primary {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
  }
  .btn.primary:hover:not(:disabled) {
    background: hsl(var(--primary) / 0.88);
  }
  .btn.ghost {
    background: transparent;
    color: hsl(var(--muted));
  }
  .btn.ghost:hover:not(:disabled) {
    background: hsl(var(--foreground) / 0.06);
    color: hsl(var(--foreground));
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .choices {
    align-self: stretch;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    text-align: left;
    background: hsl(var(--surface-2) / 0.6);
    border: 1px solid hsl(var(--border) / 0.7);
    border-radius: var(--radius);
    color: inherit;
    font: inherit;
    transition: border-color 0.15s ease, background 0.15s ease;
  }
  .choice:hover:not(:disabled) {
    background: hsl(var(--surface-2));
    border-color: hsl(var(--border));
  }
  .choice.selected {
    border-color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.12);
  }
  .glyph {
    flex: none;
  }
  .glyph path {
    fill: hsl(185 62% 52%);
  }
  .glyph .extra {
    fill: none;
    stroke: hsl(160 60% 46%);
    stroke-width: 1.7;
    stroke-linecap: round;
  }
  .ctext {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ctext b {
    font-size: 14px;
  }
  .ctext span {
    font-size: 11.5px;
    line-height: 1.35;
    color: hsl(var(--muted));
  }

  .checks {
    align-self: stretch;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .check {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    background: hsl(var(--background) / 0.5);
    border: 1px solid hsl(var(--border) / 0.6);
    border-radius: var(--radius);
  }
  .ctitle {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
  }
  .sev {
    margin-left: auto;
    font-size: 11px;
    font-weight: 600;
    color: hsl(var(--muted));
  }
  .dot {
    width: 7px;
    height: 7px;
    flex: none;
    border-radius: 99px;
    background: hsl(var(--muted));
  }
  .dot.imp {
    background: hsl(var(--warn));
  }
  .cdetail {
    font-size: 12px;
    line-height: 1.45;
    color: hsl(var(--muted));
  }
  .fix {
    display: block;
    padding: 6px 8px;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .pref {
    align-self: stretch;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 12px;
    background: hsl(var(--surface-2) / 0.6);
    border: 1px solid hsl(var(--border) / 0.7);
    border-radius: var(--radius);
  }
  .pref > div {
    flex: 1;
  }
  .ptitle {
    font-size: 13px;
    font-weight: 600;
  }
  .psub {
    margin-top: 2px;
    font-size: 11.5px;
    line-height: 1.4;
    color: hsl(var(--muted));
  }
  .err {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-top: 6px;
    padding: 8px 12px;
    background: hsl(var(--danger) / 0.16);
    border: 1px solid hsl(var(--danger) / 0.4);
    border-radius: var(--radius);
    font-size: 12px;
  }
  .err button {
    background: transparent;
    border: none;
    color: hsl(var(--muted));
  }

  /* Intro: hidden until the logo lands, then the rail grows step by step. */
  .welcome:not(.revealed) :is(.titles, .steps, .skipall) {
    opacity: 0;
  }
  .welcome:not(.revealed) .hero::after {
    transform: scaleY(0);
  }
  .revealed .titles {
    animation: rise 0.5s ease both;
  }
  .revealed .skipall {
    animation: rise 0.5s ease 0.6s both;
  }
  .revealed .hero::after {
    animation: grow 0.25s ease-in 0.25s both;
  }
  .revealed .step::before {
    animation: grow 0.24s linear calc(0.5s + var(--i) * 0.22s) both;
  }
  .revealed .node {
    animation: pop 0.38s cubic-bezier(0.3, 1.6, 0.5, 1) calc(0.45s + var(--i) * 0.22s) both;
  }
  .revealed .sbody {
    animation: rise 0.45s ease calc(0.55s + var(--i) * 0.22s) both;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  @keyframes grow {
    from {
      transform: scaleY(0);
    }
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.3);
    }
  }
  .settled :is(.titles, .skipall, .node, .sbody),
  .settled .hero::after,
  .settled .step::before {
    animation: none;
  }
  @media (prefers-reduced-motion: reduce) {
    .revealed :is(.titles, .skipall, .node, .sbody),
    .revealed .hero::after,
    .revealed .step::before {
      animation: none;
    }
  }
</style>
