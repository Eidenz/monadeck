// Central reactive app state (Svelte 5 runes) + refresh helpers. Components read
// `app.*`; the page drives a poll loop that calls the refresh functions.
import * as api from "./api";
import type {
  CapStatus,
  ClientInfo,
  DeviceInfo,
  MonadeckConfig,
  PreflightReport,
  RuntimeStatus,
  ServiceStatus,
  UevrStatus,
} from "./types";

export const app = $state({
  version: "",
  config: null as MonadeckConfig | null,
  service: { running: false, connected: false, exit_code: null } as ServiceStatus,
  runtime: { openxr: "none", openvr: "none" } as RuntimeStatus,
  caps: "no_binary" as CapStatus,
  // Runtime prerequisite report (udev rules, pkexec). Null until first checked;
  // rarely changes, so it's fetched on load rather than polled.
  preflight: null as PreflightReport | null,
  devices: [] as DeviceInfo[],
  clients: [] as ClientInfo[],
  busy: false,
  // Which built-in runtime is currently downloading/installing ("" = none).
  installing: "" as "" | "monado" | "xrizer",
  // Last install outcome, tagged by runtime so the UI shows it in the right
  // section (a Monado result must not render under xrizer).
  installResult: null as null | {
    kind: "monado" | "xrizer";
    ok: boolean;
    msg: string;
  },
  error: "" as string,
  // Set when the service stops without us asking (crash) — drives the toast.
  crash: null as { code: number | null } | null,
  // UEVR ("VR Mod") tooling status + the chihuahua install action's progress.
  uevr: { protontricks: false, chihuahua: null } as UevrStatus,
  installingChihuahua: false,
  chihuahuaResult: null as null | { ok: boolean; msg: string },
});

// Crash detection: a running→stopped transition we didn't initiate.
let wasRunning = false;
let intendedStop = false;

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
  await refreshPreflight();
  await refreshUevr();
}

// Re-check UEVR tooling (protontricks + chihuahua). Cheap; called on load and
// after installing the injector.
export async function refreshUevr() {
  try {
    app.uevr = await api.uevrStatus();
  } catch (e) {
    app.error = String(e);
  }
}

// Download the chihuahua injector ahead of time (or re-download with `force`).
export async function installChihuahua(force = false) {
  app.installingChihuahua = true;
  app.chihuahuaResult = null;
  try {
    const path = await api.installChihuahua(force);
    app.chihuahuaResult = { ok: true, msg: `Ready: ${path}` };
    await refreshUevr();
  } catch (e) {
    app.chihuahuaResult = { ok: false, msg: String(e) };
  } finally {
    app.installingChihuahua = false;
  }
}

// Re-run the runtime prerequisite checks (udev rules, pkexec). Cheap; called on
// load and after the user may have installed something.
export async function refreshPreflight() {
  try {
    app.preflight = await api.preflightCheck();
  } catch (e) {
    app.error = String(e);
  }
}

// Re-pull config from the backend so changes made in the settings window (e.g.
// always-on-top, prefix) reach the deck window, which has its own state.
export async function refreshConfig() {
  app.config = await api.getConfig();
}

export async function refreshStatus() {
  try {
    app.service = await api.serviceStatus();
    app.runtime = await api.runtimeStatus();
    app.caps = await api.capabilitiesStatus();
    // The service went from running to stopped — if we didn't ask for it, it
    // crashed (or failed to bring up a system); surface a toast.
    if (wasRunning && !app.service.running) {
      if (!intendedStop) app.crash = { code: app.service.exit_code };
      intendedStop = false;
    }
    wasRunning = app.service.running;
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
  app.crash = null;
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
  intendedStop = true; // a clean stop, not a crash
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

// Download + install a built-in runtime, then pull the updated config/status so
// the new prefix takes effect (Start enables, the no-runtime banner clears).
export async function installMonado() {
  app.installing = "monado";
  app.installResult = null;
  try {
    const r = await api.installBuiltinMonado();
    app.installResult = { kind: "monado", ok: true, msg: `Installed Monado ${r.tag}` };
    await refreshConfig();
    await refreshStatus();
  } catch (e) {
    app.installResult = { kind: "monado", ok: false, msg: String(e) };
  } finally {
    app.installing = "";
  }
}

export async function installXrizer() {
  app.installing = "xrizer";
  app.installResult = null;
  try {
    const r = await api.installBuiltinXrizer();
    app.installResult = { kind: "xrizer", ok: true, msg: `Installed xrizer ${r.tag}` };
    await refreshConfig();
  } catch (e) {
    app.installResult = { kind: "xrizer", ok: false, msg: String(e) };
  } finally {
    app.installing = "";
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
