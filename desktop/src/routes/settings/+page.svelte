<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import WindowControls from "$lib/components/WindowControls.svelte";
  import GeneralView from "$lib/views/GeneralView.svelte";
  import EnvironmentView from "$lib/views/EnvironmentView.svelte";
  import PluginsView from "$lib/views/PluginsView.svelte";
  import LogsView from "$lib/views/LogsView.svelte";
  import AboutView from "$lib/views/AboutView.svelte";
  import { loadInitial, refreshStatus } from "$lib/state.svelte";
  import type { SettingsSection } from "$lib/windows";

  const nav: { id: SettingsSection; label: string }[] = [
    { id: "general", label: "General" },
    { id: "environment", label: "Environment" },
    { id: "plugins", label: "Plugins" },
    { id: "logs", label: "Logs" },
    { id: "about", label: "About" },
  ];
  let active = $state<SettingsSection>("general");

  onMount(() => {
    loadInitial();
    const t = setInterval(refreshStatus, 2000);
    const un = listen<SettingsSection>("monadeck:section", (e) => {
      active = e.payload;
    });
    return () => {
      clearInterval(t);
      un.then((f) => f());
    };
  });
</script>

<div class="settings-window">
  <header class="topbar" data-tauri-drag-region>
    <span class="ttl" data-tauri-drag-region>Monad<b>eck</b> · Settings</span>
    <div class="spacer" data-tauri-drag-region></div>
    <WindowControls closeAction="hide" />
  </header>

  <div class="cols">
    <nav class="side">
      {#each nav as n (n.id)}
        <button
          class="nav-item state-layer"
          class:active={active === n.id}
          onclick={() => (active = n.id)}
        >
          {n.label}
        </button>
      {/each}
    </nav>

    <main class="content">
      {#if active === "general"}
        <GeneralView />
      {:else if active === "environment"}
        <EnvironmentView />
      {:else if active === "plugins"}
        <PluginsView />
      {:else if active === "logs"}
        <LogsView />
      {:else if active === "about"}
        <AboutView />
      {/if}
    </main>
  </div>
</div>

<style>
  .settings-window {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: hsl(var(--background));
  }
  .topbar {
    display: flex;
    align-items: center;
    height: 40px;
    padding: 0 6px 0 14px;
    border-bottom: 1px solid hsl(var(--border) / 0.6);
    flex: none;
  }
  .ttl {
    font-size: 13px;
    color: hsl(var(--muted));
  }
  .ttl b {
    color: hsl(var(--foreground));
    font-weight: 600;
  }
  .spacer {
    flex: 1;
    align-self: stretch;
  }
  .cols {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .side {
    width: 184px;
    flex: none;
    background: hsl(var(--surface) / 0.5);
    border-right: 1px solid hsl(var(--border) / 0.6);
    padding: 12px 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
  }
  .nav-item {
    text-align: left;
    background: transparent;
    border: none;
    color: hsl(var(--muted));
    padding: 9px 12px;
    font-size: 13px;
    font-weight: 500;
    border-radius: var(--radius-s);
  }
  .nav-item.active {
    background: hsl(var(--primary) / 0.16);
    color: hsl(var(--primary));
  }
  .content {
    flex: 1;
    overflow-y: auto;
    min-width: 0;
  }
</style>
