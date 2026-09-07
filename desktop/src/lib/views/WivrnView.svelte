<script lang="ts">
  // WiVRn backend page: headset pairing/status and the server's own settings.
  // Everything here talks to the running server over D-Bus, so most of it is
  // greyed out until the service is started from the deck.
  import { onMount } from "svelte";
  import {
    app,
    wivrnEnablePairing,
    wivrnDisablePairing,
    wivrnDisconnect,
    wivrnRevokeKey,
    wivrnRenameKey,
  } from "$lib/state.svelte";
  import { wivrnGetConfig, wivrnSetConfig } from "$lib/api";
  import Toggle from "$lib/components/Toggle.svelte";

  const status = $derived(app.service.wivrn);
  const up = $derived(status !== null);
  // Pairing (and config edits) are refused by the server while a session runs.
  const sessionUp = $derived(!!status?.session_running);

  // --- Paired headsets ------------------------------------------------------
  let renaming = $state<string | null>(null);
  let renameText = $state("");
  let confirmRevoke = $state<string | null>(null);

  function ago(secs: number): string {
    if (!secs) return "never";
    const d = Date.now() / 1000 - secs;
    if (d < 120) return `${Math.round(d)} s ago`;
    if (d < 7200) return `${Math.round(d / 60)} min ago`;
    if (d < 172800) return `${Math.round(d / 3600)} h ago`;
    return `${Math.round(d / 86400)} d ago`;
  }

  async function commitRename(key: string) {
    const name = renameText.trim();
    renaming = null;
    if (name) await wivrnRenameKey(key, name);
  }

  // --- Server configuration ---------------------------------------------------
  // Structured pickers for the settings people actually touch, plus the raw
  // JSON for everything else (see WiVRn's docs/configuration.md). The server
  // applies encoder changes on the next headset connection.
  const ENCODERS = ["auto", "x264", "nvenc", "vaapi", "vulkan"];
  const CODECS = ["auto", "h264", "h265", "av1"];

  let cfg = $state<Record<string, unknown> | null>(null);
  let raw = $state("");
  let rawDirty = $state(false);
  let cfgError = $state("");
  let cfgSaved = $state(false);
  let loadedOnce = $state(false);

  function encoderOf(c: Record<string, unknown>): { encoder: string; codec: string } {
    const e = c["encoder"];
    const one = Array.isArray(e) ? e[0] : e;
    if (typeof one === "string") return { encoder: one, codec: "auto" };
    if (one && typeof one === "object") {
      const o = one as Record<string, unknown>;
      return {
        encoder: typeof o["encoder"] === "string" ? (o["encoder"] as string) : "auto",
        codec: typeof o["codec"] === "string" ? (o["codec"] as string) : "auto",
      };
    }
    return { encoder: "auto", codec: "auto" };
  }
  const enc = $derived(cfg ? encoderOf(cfg) : { encoder: "auto", codec: "auto" });
  const tcpOnly = $derived(!!cfg?.["tcp-only"]);
  const bitDepth = $derived(Number(cfg?.["bit-depth"] ?? 8));

  async function load() {
    if (!up) return;
    try {
      const text = await wivrnGetConfig();
      const parsed = text.trim() && text.trim() !== "null" ? JSON.parse(text) : {};
      cfg = parsed && typeof parsed === "object" ? parsed : {};
      raw = JSON.stringify(cfg, null, 2);
      rawDirty = false;
      cfgError = "";
      loadedOnce = true;
    } catch (e) {
      cfgError = String(e);
    }
  }

  async function save(next: Record<string, unknown>) {
    cfgError = "";
    cfgSaved = false;
    try {
      await wivrnSetConfig(JSON.stringify(next));
      cfg = next;
      raw = JSON.stringify(next, null, 2);
      rawDirty = false;
      cfgSaved = true;
      setTimeout(() => (cfgSaved = false), 2500);
    } catch (e) {
      cfgError = String(e);
    }
  }

  function setEncoder(encoder: string, codec: string) {
    if (!cfg) return;
    const next = { ...cfg };
    if (encoder === "auto" && codec === "auto") {
      delete next["encoder"];
    } else {
      const o: Record<string, string> = {};
      if (encoder !== "auto") o["encoder"] = encoder;
      if (codec !== "auto") o["codec"] = codec;
      next["encoder"] = o;
    }
    save(next);
  }
  function setBool(key: string, v: boolean) {
    if (!cfg) return;
    const next = { ...cfg };
    if (v) next[key] = true;
    else delete next[key];
    save(next);
  }
  function setBitDepth(v: number) {
    if (!cfg) return;
    const next = { ...cfg };
    if (v === 8) delete next["bit-depth"];
    else next["bit-depth"] = v;
    save(next);
  }
  function saveRaw() {
    try {
      const parsed = JSON.parse(raw);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
        throw new Error("configuration must be a JSON object");
      save(parsed);
    } catch (e) {
      cfgError = String(e);
    }
  }

  async function copySteam() {
    if (!status?.steam_command) return;
    try {
      await navigator.clipboard.writeText(status.steam_command);
    } catch {
      /* clipboard unavailable — the text is selectable anyway */
    }
  }

  onMount(() => {
    load();
  });
  // (Re)load once the server comes up.
  $effect(() => {
    if (up && !loadedOnce) load();
    if (!up) loadedOnce = false;
  });
</script>

<section class="view">
  <h2>WiVRn</h2>

  {#if !up}
    <div class="notice">
      {app.service.available
        ? "Start the service from the deck to pair headsets and edit the server's settings."
        : "wivrn-server was not found. Install WiVRn (your distro package or the flatpak) and set its path in General."}
    </div>
  {/if}

  <div class="field">
    <span class="lbl">Headset</span>
    <div class="row">
      <span
        class="pill"
        class:good={status?.session_running}
        class:warn={status?.headset_connected && !status?.session_running}
      >
        {!up
          ? "Server stopped"
          : status?.session_running
            ? `Streaming · ${status.system_name || "headset"}`
            : status?.headset_connected
              ? "Connected"
              : "Waiting for a headset"}
      </span>
      {#if status?.session_running}
        <span class="meta">
          {status.preferred_refresh_rate ? `${Math.round(status.preferred_refresh_rate)} Hz` : ""}
          {status.bitrate ? ` · ${Math.round(status.bitrate / 1e6)} Mbit/s` : ""}
          {status.supported_codecs.length ? ` · ${status.supported_codecs.join("/")}` : ""}
        </span>
        <button onclick={wivrnDisconnect}>Disconnect</button>
      {/if}
    </div>
    <span class="note">
      Open the WiVRn app on the headset and pick this PC. It connects over Wi-Fi
      (the server advertises itself via Avahi) — a session starts the moment the
      headset connects and the overlay/plugins launch with it.
    </span>
  </div>

  <div class="field">
    <span class="lbl">Pairing</span>
    <div class="row">
      {#if status?.pairing_enabled}
        <span class="pin" title="Enter this PIN on the headset">{status.pin}</span>
        <button onclick={wivrnDisablePairing}>Stop pairing</button>
      {:else}
        <button
          class="accent"
          onclick={() => wivrnEnablePairing(2)}
          disabled={!up || sessionUp}
          title={sessionUp ? "Disconnect the headset first" : ""}
        >
          Pair a new headset
        </button>
        <button onclick={() => wivrnEnablePairing(-1)} disabled={!up || sessionUp}>
          Keep pairing open
        </button>
      {/if}
    </div>
    <span class="note">
      {status?.pairing_enabled
        ? "Pairing is open — the PIN is also shown on the deck."
        : status?.encryption_enabled === false
          ? "Encryption is disabled on this server (--no-encrypt); any headset may connect."
          : "New headsets need a one-time PIN. Pairing closes after 2 minutes, or stays open until you stop it."}
    </span>
  </div>

  <div class="field">
    <span class="lbl">Paired headsets</span>
    {#if !up}
      <span class="note">Shown while the server runs.</span>
    {:else if !status || status.known_keys.length === 0}
      <span class="note">None yet — pair one above.</span>
    {:else}
      <div class="list">
        {#each status.known_keys as h (h.public_key)}
          <div class="item">
            {#if renaming === h.public_key}
              <input
                class="rename"
                bind:value={renameText}
                onkeydown={(e) => {
                  if (e.key === "Enter") commitRename(h.public_key);
                  if (e.key === "Escape") renaming = null;
                }}
                onblur={() => commitRename(h.public_key)}
              />
            {:else}
              <span class="name" title={h.public_key}>{h.name || "Unnamed headset"}</span>
            {/if}
            <span class="meta">last seen {ago(h.last_connection)}</span>
            <div class="acts">
              <button
                onclick={() => {
                  renaming = h.public_key;
                  renameText = h.name;
                }}>Rename</button
              >
              {#if confirmRevoke === h.public_key}
                <button
                  class="danger"
                  onclick={() => {
                    confirmRevoke = null;
                    wivrnRevokeKey(h.public_key);
                  }}>Confirm remove</button
                >
                <button onclick={() => (confirmRevoke = null)}>Cancel</button>
              {:else}
                <button onclick={() => (confirmRevoke = h.public_key)}>Remove</button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="field">
    <span class="lbl">Steam launch options</span>
    <div class="row">
      <input readonly value={status?.steam_command ?? ""} placeholder="shown while the server runs" />
      <button onclick={copySteam} disabled={!status?.steam_command}>Copy</button>
    </div>
    <span class="note">
      Steam sandboxes games; this prefix lets them reach WiVRn's OpenXR runtime and
      the OpenVR compatibility layer. Paste it into a game's launch options (or set
      it via the deck's launch-options helper).
    </span>
  </div>

  <div class="field">
    <span class="lbl">Video encoding</span>
    <div class="grid">
      <span class="sub">Encoder</span>
      <div class="seg">
        {#each ENCODERS as e (e)}
          <button
            class:active={enc.encoder === e}
            disabled={!cfg}
            onclick={() => setEncoder(e, enc.codec)}>{e}</button
          >
        {/each}
      </div>
      <span class="sub">Codec</span>
      <div class="seg">
        {#each CODECS as c (c)}
          <button
            class:active={enc.codec === c}
            disabled={!cfg}
            onclick={() => setEncoder(enc.encoder, c)}>{c}</button
          >
        {/each}
      </div>
      <span class="sub">Bit depth</span>
      <div class="seg">
        {#each [8, 10] as b (b)}
          <button class:active={bitDepth === b} disabled={!cfg} onclick={() => setBitDepth(b)}
            >{b}-bit</button
          >
        {/each}
      </div>
    </div>
    <div class="toggle-row">
      <Toggle
        label="TCP only"
        checked={tcpOnly}
        disabled={!cfg}
        onchange={(v) => setBool("tcp-only", v)}
      />
      <span>TCP only <em>(more latency; for networks that drop UDP)</em></span>
    </div>
    <span class="note">
      auto = WiVRn's default (nvenc on NVIDIA, vaapi elsewhere; best codec both sides
      support). x264 is the software fallback and only does h264. Changes apply on the
      next headset connection.
    </span>
  </div>

  <div class="field">
    <span class="lbl">Server configuration (advanced)</span>
    <textarea
      rows="10"
      spellcheck="false"
      bind:value={raw}
      oninput={() => (rawDirty = true)}
      disabled={!cfg}
      placeholder={up ? "" : "loaded from the server while it runs"}
    ></textarea>
    <div class="row">
      <button class="accent" onclick={saveRaw} disabled={!cfg || !rawDirty}>Save</button>
      <button onclick={load} disabled={!up}>Reload</button>
      {#if cfgSaved}<span class="install-ok">Saved to WiVRn's config.json</span>{/if}
      {#if cfgError}<span class="install-ok bad">{cfgError}</span>{/if}
    </div>
    <span class="note">
      The full <code>~/.config/wivrn/config.json</code>, written by the server. Handy
      keys: <code>application</code> (launch a command when the headset connects),
      <code>port</code>, <code>hostname</code>, <code>publish-service</code>.
    </span>
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
  .notice {
    font-size: 12.5px;
    padding: 9px 12px;
    border-radius: var(--radius-s);
    background: hsl(var(--primary) / 0.1);
    border: 1px solid hsl(var(--primary) / 0.35);
    color: hsl(var(--foreground));
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .lbl {
    font-size: 12.5px;
    font-weight: 600;
  }
  .sub {
    font-size: 12px;
    color: hsl(var(--muted));
    align-self: center;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 14px;
    align-items: center;
  }
  input,
  textarea {
    flex: 1;
    min-width: 0;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 7px 10px;
    font-size: 12.5px;
    font-family: ui-monospace, monospace;
  }
  textarea {
    resize: vertical;
    line-height: 1.4;
  }
  input:focus,
  textarea:focus {
    outline: none;
    border-color: hsl(var(--primary));
  }
  input.rename {
    flex: none;
    width: 200px;
    padding: 4px 8px;
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
  button.accent {
    background: hsl(var(--primary));
    border-color: transparent;
    color: hsl(var(--primary-fg));
    font-weight: 700;
  }
  button.danger {
    background: hsl(var(--danger) / 0.85);
    border-color: transparent;
    color: white;
    font-weight: 600;
  }
  button:disabled {
    opacity: 0.55;
  }
  .seg {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .seg button.active {
    border-color: hsl(var(--primary));
    color: hsl(var(--primary));
    background: hsl(var(--primary) / 0.12);
  }
  .pill {
    font-size: 12px;
    font-weight: 600;
    padding: 5px 12px;
    border-radius: 99px;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--muted));
  }
  .pill.good {
    color: hsl(var(--ok));
    border-color: hsl(var(--ok) / 0.4);
  }
  .pill.warn {
    color: hsl(var(--warn));
    border-color: hsl(var(--warn) / 0.4);
  }
  .pin {
    font-family: ui-monospace, monospace;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: 4px;
    color: hsl(var(--primary));
    padding: 4px 12px;
    border-radius: var(--radius-s);
    background: hsl(var(--primary) / 0.12);
    border: 1px solid hsl(var(--primary) / 0.4);
  }
  .meta {
    font-size: 11.5px;
    color: hsl(var(--muted));
  }
  .note {
    font-size: 11px;
    color: hsl(var(--muted));
    line-height: 1.45;
  }
  code {
    background: hsl(var(--background) / 0.7);
    padding: 0 4px;
    border-radius: 4px;
    font-size: 11px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border-radius: var(--radius-s);
    background: hsl(var(--surface-2) / 0.6);
    border: 1px solid hsl(var(--border) / 0.7);
  }
  .item .name {
    font-size: 13px;
    font-weight: 600;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item .meta {
    flex: 1;
  }
  .acts {
    display: flex;
    gap: 6px;
  }
  .acts button {
    padding: 4px 9px;
    font-size: 12px;
  }
  .install-ok {
    font-size: 11.5px;
    color: hsl(var(--ok));
  }
  .install-ok.bad {
    color: hsl(var(--danger));
  }
  .toggle-row {
    display: flex;
    align-items: center;
    gap: 11px;
    font-size: 13px;
    color: hsl(var(--foreground));
  }
  .toggle-row em {
    color: hsl(var(--muted));
    font-style: normal;
  }
</style>
