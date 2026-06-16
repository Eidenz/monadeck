<script lang="ts">
  import type { DeviceInfo, DeviceKind } from "$lib/types";

  let { devices }: { devices: DeviceInfo[] } = $props();

  const ICONS: Record<DeviceKind, string> = {
    hmd: "M4 8.5h16a2 2 0 0 1 2 2v3.2a3 3 0 0 1-3 3h-2.4a2 2 0 0 1-1.7-.95L13.6 14.8a2 2 0 0 0-3.2 0l-1.3 1.95A2 2 0 0 1 7.4 17.7H5a3 3 0 0 1-3-3V10.5a2 2 0 0 1 2-2z",
    controller:
      "M9 3.2h6a3.2 3.2 0 0 1 3.2 3.2v9.4a3.4 3.4 0 0 1-6.5 1.4l-.9-2.1a1 1 0 0 0-1.6 0l-.9 2.1A3.4 3.4 0 0 1 5.8 15.8V6.4A3.2 3.2 0 0 1 9 3.2z",
    tracker:
      "M12 6.5c4 0 6.5 1.8 6.5 4s-2.5 4-6.5 4-6.5-1.8-6.5-4 2.5-4 6.5-4zm-4.2 8.2A8.7 8.7 0 0 0 12 15.6a8.7 8.7 0 0 0 4.2-.9l1.2 1.9a1.6 1.6 0 0 1-1.36 2.45H7.96A1.6 1.6 0 0 1 6.6 16.6l1.2-1.9z",
    gamepad:
      "M7.5 7.5h9a4.2 4.2 0 0 1 4.1 3.3l1 4.6a2.1 2.1 0 0 1-3.9 1.4l-1-1.8a2 2 0 0 0-1.75-1H9.05a2 2 0 0 0-1.75 1l-1 1.8a2.1 2.1 0 0 1-3.9-1.4l1-4.6A4.2 4.2 0 0 1 7.5 7.5z",
    basestation:
      "M6.5 3.5h11a2 2 0 0 1 2 2v13a2 2 0 0 1-2 2h-11a2 2 0 0 1-2-2v-13a2 2 0 0 1 2-2zM12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z",
    unknown:
      "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zm0 12.4a1.1 1.1 0 1 0 0 2.2 1.1 1.1 0 0 0 0-2.2zm0-9.1a3 3 0 0 0-2.9 2.2 1 1 0 1 0 1.93.5A1 1 0 1 1 12 11a1 1 0 0 0-1 1v1.2a1 1 0 0 0 2 0v-.36A3 3 0 0 0 12 6.3z",
  };

  // Fixed skeleton: HMD + left + right are always shown (grayed when absent).
  const head = $derived(
    devices.find((d) => d.role === "head" || d.kind === "hmd") ?? null,
  );
  const left = $derived(devices.find((d) => d.role === "left") ?? null);
  const right = $derived(devices.find((d) => d.role === "right") ?? null);
  const extras = $derived(
    devices.filter((d) => d !== head && d !== left && d !== right),
  );

  const slots = $derived([
    { key: "head", icon: "hmd" as DeviceKind, dev: head, flip: false, label: "Headset" },
    { key: "left", icon: "controller" as DeviceKind, dev: left, flip: false, label: "Left controller" },
    { key: "right", icon: "controller" as DeviceKind, dev: right, flip: true, label: "Right controller" },
  ]);

  function batteryColor(charge: number): string {
    if (charge > 0.5) return "var(--ok)";
    if (charge > 0.2) return "var(--warn)";
    return "var(--danger)";
  }
</script>

<svg width="0" height="0" style="position:absolute" aria-hidden="true">
  <defs>
    <linearGradient id="devgrad" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="hsl(185 70% 52%)" />
      <stop offset="1" stop-color="hsl(150 62% 46%)" />
    </linearGradient>
  </defs>
</svg>

<div class="strip">
  {#each slots as slot (slot.key)}
    <div
      class="dev"
      class:off={!slot.dev}
      title={slot.dev ? `${slot.dev.name}${slot.dev.serial ? `\n${slot.dev.serial}` : ""}` : `${slot.label} — not detected`}
    >
      <svg viewBox="0 0 24 24" width="46" height="46" aria-hidden="true" style={slot.flip ? "transform:scaleX(-1)" : ""}>
        <path d={ICONS[slot.icon]} fill={slot.dev ? "url(#devgrad)" : "hsl(var(--muted) / 0.3)"} />
      </svg>
      {#if slot.dev?.battery}
        <span class="batt" style={`--c:${batteryColor(slot.dev.battery.charge)}`}>
          {Math.round(slot.dev.battery.charge * 100)}
        </span>
      {/if}
    </div>
  {/each}

  {#each extras as d (d.index)}
    <div class="dev" title={`${d.name}${d.serial ? `\n${d.serial}` : ""}`}>
      <svg viewBox="0 0 24 24" width="46" height="46" aria-hidden="true">
        <path d={ICONS[d.kind] ?? ICONS.unknown} fill="url(#devgrad)" />
      </svg>
      {#if d.battery}
        <span class="batt" style={`--c:${batteryColor(d.battery.charge)}`}>
          {Math.round(d.battery.charge * 100)}
        </span>
      {/if}
    </div>
  {/each}
</div>

<style>
  .strip {
    display: flex;
    align-items: flex-start;
    gap: 18px;
    padding: 6px 2px 2px;
    flex-wrap: wrap;
    row-gap: 12px;
  }
  .dev {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    filter: drop-shadow(0 1px 4px hsl(185 70% 30% / 0.35));
  }
  .dev.off {
    filter: none;
  }
  .batt {
    position: absolute;
    bottom: -6px;
    right: -5px;
    font-size: 9.5px;
    font-variant-numeric: tabular-nums;
    color: hsl(var(--c));
    font-weight: 700;
    background: hsl(var(--surface) / 0.95);
    border-radius: 6px;
    padding: 0 3px;
    line-height: 1.5;
  }
</style>
