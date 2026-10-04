<script lang="ts">
  // Lighthouse tracking, without SteamVR: the tracking driver, room setup
  // (floor, centre, forward), pairing controllers/trackers to their receivers,
  // and base stations over Bluetooth. Monado backend only.
  import { onMount } from "svelte";
  import {
    app,
    saveConfig,
    refreshFloorCal,
    runFloorCalibration,
    runSurviveCalibration,
  } from "$lib/state.svelte";
  import * as api from "$lib/api";
  import Toggle from "$lib/components/Toggle.svelte";
  import type {
    FoundStation,
    Receiver,
    SavedStation,
    StationPower,
    StationState,
  } from "$lib/types";

  const driver = $derived(app.config?.lighthouse_driver ?? "steamvr");
  const vrUp = $derived(app.service.connected);
  const native = $derived(!!app.floorCal?.native);

  function setDriver(d: string) {
    if (!app.config) return;
    app.config.lighthouse_driver = d;
    saveConfig();
  }

  // --- Room setup: live headset height while VR runs ---------------------------
  let height = $state<number | null>(null);

  // --- Pairing -----------------------------------------------------------------
  let receivers = $state<Receiver[]>([]);
  let loadingReceivers = $state(false);
  let pairing = $state<{ serial: string; left: number } | null>(null);
  let pairResult = $state<{ ok: boolean; msg: string } | null>(null);

  async function loadReceivers() {
    loadingReceivers = true;
    try {
      receivers = await api.pairingReceivers();
    } catch (e) {
      pairResult = { ok: false, msg: String(e) };
    } finally {
      loadingReceivers = false;
    }
  }

  // Devices that are on right now, keyed by serial (name as a fallback).
  async function liveDevices(): Promise<Map<string, string>> {
    const out = new Map<string, string>();
    if (!app.service.connected) return out;
    try {
      const s = await api.getSnapshot();
      for (const d of s.devices) {
        if (d.kind === "hmd" || d.kind === "basestation" || d.connected === false) continue;
        out.set(d.serial ?? `${d.name}#${d.index}`, d.name);
      }
    } catch {
      // VR went away: no live check, the light on the device tells.
    }
    return out;
  }

  async function pair(r: Receiver) {
    pairResult = null;
    const before = await liveDevices();
    let secs: number;
    try {
      secs = await api.pairingStart(r.serial);
    } catch (e) {
      pairResult = { ok: false, msg: String(e) };
      return;
    }
    pairing = { serial: r.serial, left: secs };
    // Watch for a device that wasn't on before (VR running), until the
    // receiver stops listening.
    const end = Date.now() + secs * 1000 + 2000;
    while (Date.now() < end) {
      await new Promise((res) => setTimeout(res, 1000));
      if (pairing) pairing.left = Math.max(0, Math.ceil((end - 2000 - Date.now()) / 1000));
      if (!app.service.connected) continue;
      const now = await liveDevices();
      const fresh = [...now.entries()].find(([k]) => !before.has(k));
      if (fresh) {
        pairing = null;
        pairResult = { ok: true, msg: `Paired: ${fresh[1]}.` };
        await loadReceivers();
        return;
      }
    }
    pairing = null;
    pairResult = app.service.connected
      ? {
          ok: false,
          msg: "Nothing paired. Put the device in pairing mode first, then press Pair again.",
        }
      : {
          ok: true,
          msg: "Pairing window closed. If the device's light turned solid, it's paired; start VR to use it.",
        };
    await loadReceivers();
  }

  function receiverState(r: Receiver): string {
    if (r.active === null) return "";
    return r.active ? "In use" : "Free";
  }

  // --- Base stations -----------------------------------------------------------
  const saved = $derived(app.config?.base_stations ?? []);
  let scanning = $state(false);
  let found = $state<FoundStation[]>([]);
  let bsError = $state("");
  let states = $state<Record<string, StationState>>({});
  let busy = $state<Record<string, string>>({}); // address → what it's doing

  const unsaved = $derived(found.filter((f) => !saved.some((s) => s.address === f.address)));

  async function scan() {
    scanning = true;
    bsError = "";
    try {
      found = await api.bsScan(6);
      if (found.length === 0) bsError = "No base stations found. Are they plugged in, and in range?";
    } catch (e) {
      bsError = String(e);
    } finally {
      scanning = false;
    }
  }

  function add(f: FoundStation) {
    if (!app.config) return;
    app.config.base_stations = [
      ...app.config.base_stations,
      { address: f.address, name: f.name, version: f.version, bsid: null },
    ];
    saveConfig();
  }

  function remove(s: SavedStation) {
    if (!app.config) return;
    app.config.base_stations = app.config.base_stations.filter((x) => x.address !== s.address);
    saveConfig();
  }

  function setBsid(s: SavedStation, value: string) {
    if (!app.config) return;
    const v = value.trim().toUpperCase();
    app.config.base_stations = app.config.base_stations.map((x) =>
      x.address === s.address ? { ...x, bsid: v || null } : x,
    );
    saveConfig();
  }

  // Run one station action, showing what it's doing and any error.
  async function act(s: SavedStation, label: string, f: () => Promise<void>) {
    busy[s.address] = label;
    bsError = "";
    try {
      await f();
    } catch (e) {
      bsError = `${s.name}: ${e}`;
    } finally {
      delete busy[s.address];
    }
  }

  const readState = (s: SavedStation) =>
    act(s, "Reading…", async () => {
      states[s.address] = await api.bsState(s.address);
    });

  const setPower = (s: SavedStation, p: StationPower) =>
    act(s, "Switching…", async () => {
      await api.bsSetPower(s.address, s.version, p, s.bsid);
      if (s.version === "v2") {
        states[s.address] = { ...(states[s.address] ?? { channel: null }), power: p === "on" ? "waking" : p };
      }
    });

  const setChannel = (s: SavedStation, ch: number) =>
    act(s, "Setting channel…", async () => {
      await api.bsSetChannel(s.address, ch);
      states[s.address] = { ...(states[s.address] ?? { power: null }), channel: ch };
    });

  const identify = (s: SavedStation) => act(s, "Blinking…", () => api.bsIdentify(s.address));

  async function switchAll(p: StationPower) {
    for (const s of saved) {
      if (s.version === "v1" && !s.bsid) continue;
      await setPower(s, s.version === "v1" && p === "standby" ? "sleep" : p);
    }
  }

  const powerLabel: Record<string, string> = {
    on: "On",
    sleep: "Asleep",
    standby: "Standby",
    waking: "Waking up",
  };

  onMount(() => {
    refreshFloorCal();
    loadReceivers();
    const t = setInterval(async () => {
      if (driver === "steamvr" && native && app.service.connected) {
        try {
          height = await api.headHeight();
        } catch {
          height = null;
        }
      } else {
        height = null;
      }
    }, 1000);
    return () => clearInterval(t);
  });
</script>

<section class="view">
  <h2>Lighthouse</h2>

  <div class="field">
    <span class="lbl">Tracking driver</span>
    <div class="seg">
      {#each ["steamvr", "vive", "survive"] as d (d)}
        <button class:active={driver === d} onclick={() => setDriver(d)}>{d}</button>
      {/each}
    </div>
    <span class="help">
      steamvr (default) loads SteamVR's tracking driver: it works for every Lighthouse headset.
      SteamVR has to be installed in Steam, but never runs.
      vive and survive are the open-source drivers for Vive/Index.
      {#if driver === "steamvr" && app.steamvr === false}
        <b class="warn-text">SteamVR isn't installed: install it from Steam.</b>
      {/if}
    </span>
  </div>

  {#if driver === "steamvr"}
    <div class="card">
      <div class="card-head">
        <span class="lbl">Room setup</span>
        <span
          class="pill"
          class:good={app.floorCal?.calibrated}
          class:warn={app.floorCal && !app.floorCal.calibrated}
        >
          {!app.floorCal
            ? "…"
            : app.floorCal.calibrated
              ? "Set ✓"
              : app.floorCal.has_universe
                ? "Not set"
                : "No base stations seen yet"}
        </span>
      </div>
      <span class="help">
        Sets your floor height, the center of your play space and which way is forward.
        Stand the headset upright on the floor where you want the center, lenses facing the way
        you'll face, and leave it still. Do it again after moving your base stations.
      </span>
      {#if native}
        <div class="row">
          <button
            class:accent={app.floorCal && !app.floorCal.calibrated}
            onclick={runFloorCalibration}
            disabled={app.calibratingFloor || !vrUp}
          >
            {app.calibratingFloor ? "Measuring…" : "Set floor and center"}
          </button>
          {#if !vrUp}
            <span class="muted">Start VR first.</span>
          {:else if height !== null}
            <span class="muted">Headset height: {(height * 100).toFixed(1)} cm above the floor</span>
          {/if}
        </div>
      {:else}
        <div class="row">
          <button
            onclick={runFloorCalibration}
            disabled={app.calibratingFloor || !app.floorCal?.available || app.service.running}
          >
            {app.calibratingFloor ? "Calibrating…" : "Calibrate with SteamVR"}
          </button>
          <span class="muted">
            {!app.floorCal?.available
              ? "SteamVR not found."
              : app.service.running
                ? "Stop VR first."
                : ""}
          </span>
        </div>
        <span class="help">
          This Monado can't report where the headset is, so this runs SteamVR's own calibration
          tool. Install the latest Monado fork from General to do it without SteamVR, with VR running.
        </span>
      {/if}
      {#if app.floorCalResult}
        <span class="result" class:bad={!app.floorCalResult.ok}>{app.floorCalResult.msg}</span>
      {/if}
    </div>
  {/if}

  {#if driver === "survive"}
    <div class="card">
      <div class="card-head">
        <span class="lbl">Libsurvive calibration</span>
        <span
          class="pill"
          class:good={app.surviveCal?.available && app.surviveCal.source_present}
          class:warn={app.surviveCal && !app.surviveCal.available}
        >
          {!app.surviveCal
            ? "…"
            : !app.surviveCal.available
              ? "survive-cli not found"
              : !app.surviveCal.source_present
                ? "No SteamVR data to import"
                : "Ready to import"}
        </span>
      </div>
      <div class="row">
        <button
          onclick={runSurviveCalibration}
          disabled={app.calibratingSurvive ||
            !app.surviveCal?.available ||
            !app.surviveCal?.source_present ||
            app.service.running}
        >
          {app.calibratingSurvive ? "Importing… (~1 min)" : "Import SteamVR calibration"}
        </button>
      </div>
      <span class="help">
        Seeds libsurvive from SteamVR's base-station solve, then runs it for ~1 minute to converge.
        Keep the headset still, on the floor, in view of your base stations. Needs
        <code>survive-cli</code> (ships with libsurvive) and a prior SteamVR room setup.{app.service
          .running
          ? " Stop the service first."
          : ""}
      </span>
      {#if app.surviveCalResult}
        <span class="result" class:bad={!app.surviveCalResult.ok}>{app.surviveCalResult.msg}</span>
      {/if}
    </div>
  {/if}

  <div class="card">
    <div class="card-head">
      <span class="lbl">Controllers and trackers</span>
      <button class="small" onclick={loadReceivers} disabled={loadingReceivers || !!pairing}>
        {loadingReceivers ? "…" : "Refresh"}
      </button>
    </div>
    <span class="help">
      Put the device in pairing mode, then press Pair on a free receiver: whatever it was paired
      with before is replaced. Works with VR running.
    </span>
    {#if receivers.length === 0}
      <span class="muted">{loadingReceivers ? "Looking for receivers…" : "No receivers plugged in."}</span>
    {/if}
    {#each receivers as r (r.serial)}
      <div class="item">
        <div class="item-text">
          <span class="item-name">{r.name}</span>
          <span class="item-sub">{r.serial}</span>
        </div>
        {#if receiverState(r)}
          <span class="pill" class:good={r.active === false}>{receiverState(r)}</span>
        {/if}
        <button
          class:accent={r.active === false}
          onclick={() => pair(r)}
          disabled={!!pairing}
        >
          {pairing?.serial === r.serial ? `Listening… ${pairing.left}s` : "Pair"}
        </button>
      </div>
    {/each}
    {#if pairResult}
      <span class="result" class:bad={!pairResult.ok}>{pairResult.msg}</span>
    {/if}
  </div>

  <div class="card">
    <div class="card-head">
      <span class="lbl">Base stations</span>
      <button class="small" onclick={scan} disabled={scanning}>
        {scanning ? "Scanning…" : "Scan"}
      </button>
    </div>
    <span class="help">
      Switched over Bluetooth, so the PC needs a Bluetooth adapter. Scan, add your base stations,
      then switch them here or along with VR.
    </span>

    {#each unsaved as f (f.address)}
      <div class="item">
        <div class="item-text">
          <span class="item-name">{f.name}</span>
          <span class="item-sub">
            {f.version === "v2" ? "2.0" : "1.0"} · {f.address}{f.rssi !== null ? ` · ${f.rssi} dBm` : ""}
          </span>
        </div>
        <button class="accent" onclick={() => add(f)}>Add</button>
      </div>
    {/each}

    {#each saved as s (s.address)}
      {@const st = states[s.address]}
      <div class="station">
        <div class="item">
          <div class="item-text">
            <span class="item-name">{s.name}</span>
            <span class="item-sub">
              {s.version === "v2" ? "2.0" : "1.0"} · {s.address}{st?.channel ? ` · channel ${st.channel}` : ""}
            </span>
          </div>
          {#if busy[s.address]}
            <span class="muted">{busy[s.address]}</span>
          {:else if st?.power}
            <span class="pill" class:good={st.power === "on"}>{powerLabel[st.power]}</span>
          {/if}
          <button class="small ghost" onclick={() => remove(s)} disabled={!!busy[s.address]}>Remove</button>
        </div>
        <div class="row wrap">
          <button onclick={() => setPower(s, "on")} disabled={!!busy[s.address]}>On</button>
          <button onclick={() => setPower(s, "sleep")} disabled={!!busy[s.address]}>Sleep</button>
          {#if s.version === "v2"}
            <button onclick={() => setPower(s, "standby")} disabled={!!busy[s.address]}>Standby</button>
            <button onclick={() => readState(s)} disabled={!!busy[s.address]}>Read state</button>
            <button onclick={() => identify(s)} disabled={!!busy[s.address]}>Blink</button>
            <select
              aria-label="Channel"
              disabled={!!busy[s.address]}
              value={st?.channel ?? ""}
              onchange={(e) => {
                const ch = Number((e.currentTarget as HTMLSelectElement).value);
                if (ch) setChannel(s, ch);
              }}
            >
              <option value="" disabled>Channel</option>
              {#each Array.from({ length: 16 }, (_, i) => i + 1) as ch (ch)}
                <option value={ch}>Channel {ch}</option>
              {/each}
            </select>
          {:else}
            <input
              placeholder="ID on its back (8 characters)"
              value={s.bsid ?? ""}
              maxlength="8"
              onchange={(e) => setBsid(s, (e.currentTarget as HTMLInputElement).value)}
            />
          {/if}
        </div>
      </div>
    {/each}

    {#if saved.length > 1}
      <div class="row">
        <button onclick={() => switchAll("on")}>All on</button>
        <button onclick={() => switchAll(app.config?.base_stations_off ?? "sleep")}>All off</button>
      </div>
    {/if}

    {#if saved.length > 0}
      <div class="toggle-row">
        <Toggle
          label="Switch base stations with VR"
          checked={app.config?.base_stations_auto ?? false}
          onchange={(v) => {
            if (app.config) {
              app.config.base_stations_auto = v;
              saveConfig();
            }
          }}
        />
        <span>Switch them on when VR starts and off when it stops</span>
      </div>
      <div class="field">
        <span class="lbl sub">Off means</span>
        <div class="seg">
          {#each [["sleep", "Sleep"], ["standby", "Standby"]] as [v, label] (v)}
            <button
              class:active={(app.config?.base_stations_off ?? "sleep") === v}
              onclick={() => {
                if (app.config) {
                  app.config.base_stations_off = v as StationPower;
                  saveConfig();
                }
              }}>{label}</button
            >
          {/each}
        </div>
        <span class="help">
          Sleep turns everything off, like SteamVR does. Standby only stops the lasers: the
          station wakes up faster, but keeps spinning (and humming). 1.0 stations always sleep.
        </span>
      </div>
    {/if}

    {#if bsError}
      <span class="result bad">{bsError}</span>
    {/if}
  </div>
</section>

<style>
  .view {
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  h2 {
    margin: 0 0 2px;
    font-size: 17px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    background: hsl(var(--surface) / 0.6);
    border: 1px solid hsl(var(--border) / 0.7);
    border-radius: var(--radius);
  }
  .card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .lbl {
    font-size: 12.5px;
    font-weight: 600;
  }
  .lbl.sub {
    font-size: 12px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .row.wrap {
    flex-wrap: wrap;
  }
  .help {
    font-size: 11.5px;
    color: hsl(var(--muted));
    line-height: 1.45;
  }
  .help b {
    font-weight: 600;
  }
  .warn-text {
    color: hsl(var(--warn));
  }
  .muted {
    font-size: 11.5px;
    color: hsl(var(--muted));
  }
  .result {
    font-size: 11.5px;
    color: hsl(var(--ok));
    line-height: 1.45;
  }
  .result.bad {
    color: hsl(var(--danger));
  }
  button {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 7px 12px;
    font-size: 12.5px;
    flex: none;
  }
  button.small {
    padding: 4px 10px;
    font-size: 11.5px;
  }
  button.ghost {
    background: transparent;
    color: hsl(var(--muted));
  }
  button.accent {
    background: hsl(var(--primary));
    border-color: transparent;
    color: hsl(var(--primary-fg));
    font-weight: 700;
  }
  button:disabled {
    opacity: 0.55;
  }
  select,
  input {
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 6px 8px;
    font-size: 12px;
  }
  input {
    flex: 1;
    min-width: 0;
    font-family: ui-monospace, monospace;
    text-transform: uppercase;
  }
  input::placeholder {
    text-transform: none;
  }
  input:focus,
  select:focus {
    outline: none;
    border-color: hsl(var(--primary));
  }
  .seg {
    display: flex;
    gap: 6px;
  }
  .seg button.active {
    border-color: hsl(var(--primary));
    color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.12);
  }
  .pill {
    font-size: 11.5px;
    font-weight: 600;
    padding: 3px 10px;
    border-radius: 99px;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--muted));
    flex: none;
  }
  .pill.good {
    color: hsl(var(--ok));
    border-color: hsl(var(--ok) / 0.4);
  }
  .pill.warn {
    color: hsl(var(--warn));
    border-color: hsl(var(--warn) / 0.4);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-top: 8px;
    border-top: 1px solid hsl(var(--border) / 0.5);
  }
  .station .row {
    margin-top: 6px;
  }
  .item-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .item-name {
    font-size: 12.5px;
    font-weight: 600;
  }
  .item-sub {
    font-size: 11px;
    color: hsl(var(--muted));
    font-family: ui-monospace, monospace;
  }
  .toggle-row {
    display: flex;
    align-items: center;
    gap: 11px;
    font-size: 13px;
    color: hsl(var(--foreground));
  }
</style>
