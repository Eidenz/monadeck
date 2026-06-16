// Central reactive app state (Svelte 5 runes) + refresh helpers. Components read
// `app.*`; the page drives a poll loop that calls the refresh functions.
import * as api from "./api";
import type {
  CapStatus,
  ClientInfo,
  DeviceInfo,
  MonadeckConfig,
  RuntimeStatus,
  ServiceStatus,
} from "./types";

export const app = $state({
  version: "",
  config: null as MonadeckConfig | null,
  service: { running: false, connected: false, exit_code: null } as ServiceStatus,
  runtime: { openxr: "none", openvr: "none" } as RuntimeStatus,
  caps: "no_binary" as CapStatus,
  devices: [] as DeviceInfo[],
  clients: [] as ClientInfo[],
  busy: false,
  error: "" as string,
});

export async function loadInitial() {
  app.version = await api.appVersion();
  app.config = await api.getConfig();
  // First-run convenience: autodetect the monado prefix and xrizer dir if unset.
  if (app.config) {
    let changed = false;
    if (!app.config.monado_prefix) {
      const guess = await api.autodetectPrefix();
      if (guess) {
        app.config.monado_prefix = guess;
        changed = true;
      }
    }
    if (!app.config.xrizer_path) {
      const xr = await api.autodetectXrizer();
      if (xr) {
        app.config.xrizer_path = xr;
        changed = true;
      }
    }
    if (changed) await api.setConfig($state.snapshot(app.config));
  }
  await refreshStatus();
}

export async function refreshStatus() {
  try {
    app.service = await api.serviceStatus();
    app.runtime = await api.runtimeStatus();
    app.caps = await api.capabilitiesStatus();
  } catch (e) {
    app.error = String(e);
  }
}

// Back off device polling while the service is up but returning nothing (e.g.
// no HMD): otherwise we'd call auto_connect() every poll and libmonado spams its
// connection-retry errors to stderr.
let deviceEmptyStreak = 0;
let deviceSkip = 0;

export async function refreshSnapshot() {
  if (!app.service.connected) {
    app.devices = [];
    app.clients = [];
    deviceEmptyStreak = 0;
    deviceSkip = 0;
    return;
  }
  if (deviceSkip > 0) {
    deviceSkip--;
    return;
  }
  try {
    const s = await api.getSnapshot();
    app.devices = s.devices;
    app.clients = s.clients;
    deviceEmptyStreak = s.devices.length === 0 ? deviceEmptyStreak + 1 : 0;
  } catch {
    app.devices = [];
    app.clients = [];
    deviceEmptyStreak++;
  }
  // After two empty/failed polls, drop to ~1 attempt every 4 cycles (~6s).
  if (deviceEmptyStreak >= 2) deviceSkip = 3;
}

export async function saveConfig() {
  if (!app.config) return;
  await api.setConfig($state.snapshot(app.config));
}

export async function start() {
  app.busy = true;
  app.error = "";
  try {
    await api.startService();
  } catch (e) {
    app.error = String(e);
  } finally {
    app.busy = false;
    await refreshStatus();
  }
}

export async function stop() {
  app.busy = true;
  try {
    await api.stopService();
  } catch (e) {
    app.error = String(e);
  } finally {
    app.busy = false;
    await refreshStatus();
  }
}

export async function applyCaps() {
  app.busy = true;
  try {
    await api.applyCapabilities();
  } catch (e) {
    app.error = String(e);
  } finally {
    app.busy = false;
    await refreshStatus();
  }
}
