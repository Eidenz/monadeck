<script lang="ts">
  // First-run onboarding. Consolidates the individual deck notices (runtime,
  // capabilities, prerequisites, Proton import, floor calibration) into one
  // checklist so a newcomer isn't buried in stacked popups. Each row reuses the
  // exact same status + action the standalone notices use, so completing one here
  // is identical to fixing it from its toast. "Skip" / "Done" both just mark the
  // config seen — anything still outstanding then falls back to its popup.
  import {
    app,
    saveConfig,
    installMonado,
    applyCaps,
    applyImportOpenxr,
    runFloorCalibration,
  } from "$lib/state.svelte";
  import { openSettings } from "$lib/windows";
  import TitleBar from "./TitleBar.svelte";

  const runtimeDone = $derived(app.caps !== "no_binary");
  const capsDone = $derived(app.caps === "set");
  const preflightDone = $derived(!!app.preflight?.all_ok);
  const protonDone = $derived(app.importOpenxr);
  // Floor calibration only applies to the SteamVR Lighthouse driver.
  const showFloor = $derived(app.config?.lighthouse_driver === "steamvr");
  const floorDone = $derived(!!app.floorCal?.calibrated);

  const steps = $derived([
    runtimeDone,
    capsDone,
    preflightDone,
    protonDone,
    ...(showFloor ? [floorDone] : []),
  ]);
  const doneCount = $derived(steps.filter(Boolean).length);
  const allDone = $derived(doneCount === steps.length);

  function finish() {
    if (app.config) {
      app.config.setup_seen = true;
      saveConfig();
    }
  }
</script>

<div class="welcome">
  <TitleBar />
  <div class="body">
    <div class="head">
      <h1>Welcome to Monadeck</h1>
      <p>
        A few one-time setup steps. Do as many as you like now, you can skip and
        finish them later from the notices and Settings.
      </p>
      <div class="progress">{doneCount} / {steps.length} done</div>
    </div>

    <div class="list">
      <!-- Monado runtime -->
      <div class="item" class:done={runtimeDone}>
        <div class="mark">{runtimeDone ? "✓" : ""}</div>
        <div class="text">
          <div class="t">Monado runtime</div>
          <div class="d">
            {runtimeDone
              ? "monado-service found."
              : "Install the built-in fork, or point the prefix at your build in Settings."}
          </div>
        </div>
        <div class="act">
          {#if runtimeDone}
            <span class="ok">Ready</span>
          {:else}
            <button
              class="accent"
              onclick={installMonado}
              disabled={app.installing !== ""}
            >
              {app.installing === "monado" ? "Installing…" : "Install"}
            </button>
          {/if}
        </div>
      </div>

      <!-- Service capabilities -->
      <div class="item" class:done={capsDone}>
        <div class="mark">{capsDone ? "✓" : ""}</div>
        <div class="text">
          <div class="t">Service capabilities</div>
          <div class="d">CAP_SYS_NICE on monado-service (re-apply after each rebuild).</div>
        </div>
        <div class="act">
          {#if capsDone}
            <span class="ok">Set</span>
          {:else if app.caps === "needs_setcap"}
            <button class="accent" onclick={applyCaps} disabled={app.busy}>
              {app.busy ? "…" : "Set"}
            </button>
          {:else if app.caps === "no_binary"}
            <span class="muted">After runtime</span>
          {:else}
            <span class="muted">Needs getcap/setcap</span>
          {/if}
        </div>
      </div>

      <!-- VR prerequisites (preflight) -->
      <div class="item" class:done={preflightDone}>
        <div class="mark">{preflightDone ? "✓" : ""}</div>
        <div class="text">
          <div class="t">VR prerequisites</div>
          <div class="d">
            {preflightDone
              ? "udev rules + pkexec present."
              : "Some system prerequisites are missing (udev rules / pkexec)."}
          </div>
        </div>
        <div class="act">
          {#if preflightDone}
            <span class="ok">Ready</span>
          {:else}
            <button onclick={() => openSettings("environment")}>View fixes</button>
          {/if}
        </div>
      </div>

      <!-- Proton 11 / SLR4 OpenXR import -->
      <div class="item" class:done={protonDone}>
        <div class="mark">{protonDone ? "✓" : ""}</div>
        <div class="text">
          <div class="t">Proton OpenXR import</div>
          <div class="d">Needed for Proton 11 / SLR4 (harmless on Proton 10). Applies after a reboot.</div>
        </div>
        <div class="act">
          {#if protonDone}
            <span class="ok">Set</span>
          {:else}
            <button class="accent" onclick={applyImportOpenxr} disabled={app.applyingProton}>
              {app.applyingProton ? "…" : "Create file"}
            </button>
          {/if}
        </div>
      </div>

      <!-- Floor calibration (SteamVR Lighthouse only) -->
      {#if showFloor}
        <div class="item" class:done={floorDone}>
          <div class="mark">{floorDone ? "✓" : ""}</div>
          <div class="text">
            <div class="t">Floor calibration</div>
            <div class="d">
              {#if floorDone}
                Play space is set.
              {:else if app.service.running}
                Stop the service first, then calibrate (headset on the floor, centered).
              {:else}
                Set floor + forward for the SteamVR Lighthouse driver (headset on the floor, centered).
              {/if}
            </div>
          </div>
          <div class="act">
            {#if floorDone}
              <span class="ok">Calibrated</span>
            {:else if !app.floorCal?.available}
              <span class="muted">SteamVR not found</span>
            {:else}
              <button
                class="accent"
                onclick={runFloorCalibration}
                disabled={app.calibratingFloor || app.service.running}
              >
                {app.calibratingFloor ? "…" : "Calibrate"}
              </button>
            {/if}
          </div>
        </div>
      {/if}
    </div>

    <div class="foot">
      <button class="ghost" onclick={finish}>Skip for now</button>
      <button class="accent" onclick={finish}>{allDone ? "Done" : "Continue"}</button>
    </div>
  </div>
</div>

<style>
  .welcome {
    background:
      radial-gradient(120% 90% at 30% -10%, hsl(195 35% 18% / 0.6), transparent 60%),
      linear-gradient(180deg, hsl(var(--surface)), hsl(var(--background)));
  }
  .body {
    padding: 4px 16px 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .head h1 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: hsl(var(--foreground));
  }
  .head p {
    margin: 6px 0 0;
    font-size: 12px;
    line-height: 1.45;
    color: hsl(var(--muted));
  }
  .progress {
    margin-top: 8px;
    font-size: 11px;
    font-weight: 600;
    color: hsl(var(--primary));
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    background: hsl(var(--surface) / 0.6);
    border: 1px solid hsl(var(--border) / 0.7);
    border-radius: var(--radius);
  }
  .item.done {
    opacity: 0.7;
  }
  .mark {
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 99px;
    display: grid;
    place-items: center;
    font-size: 12px;
    font-weight: 700;
    border: 1px solid hsl(var(--border));
    color: hsl(var(--primary-fg));
  }
  .item.done .mark {
    background: hsl(var(--ok));
    border-color: transparent;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .t {
    font-size: 12.5px;
    font-weight: 600;
    color: hsl(var(--foreground));
  }
  .d {
    font-size: 11px;
    color: hsl(var(--muted));
    line-height: 1.4;
    margin-top: 2px;
  }
  .act {
    flex: none;
  }
  .act .ok {
    font-size: 11.5px;
    font-weight: 600;
    color: hsl(var(--ok));
  }
  .act .muted {
    font-size: 11px;
    color: hsl(var(--muted));
  }
  button {
    border-radius: var(--radius-s);
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 600;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
  }
  button:disabled {
    opacity: 0.6;
  }
  button.accent {
    background: hsl(var(--primary));
    border-color: transparent;
    color: hsl(var(--primary-fg));
  }
  button.ghost {
    background: transparent;
    color: hsl(var(--muted));
  }
  .foot {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    margin-top: 2px;
  }
</style>
