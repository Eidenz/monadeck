<script lang="ts">
  import {
    buildInputPath,
    getInputsForHand,
    getMirrorPath,
    type ControllerInput,
  } from "./data/controllers";
  import { getSvgAssetUrl } from "./data/svgAssets";
  import {
    editor,
    activeProfile,
    activeBindingSources,
    setSelectedInput,
    setHoveredInput,
  } from "./store.svelte";

  let { hand }: { hand: "left" | "right" } = $props();

  const profile = $derived(activeProfile());
  const isLeft = $derived(hand === "left");
  const vb = $derived(profile.svgViewBox.split(" ").map(Number));
  const svgUrl = $derived(getSvgAssetUrl(profile.svgAsset));
  const handInputs = $derived(getInputsForHand(profile, hand));
  const boundPaths = $derived(new Set(activeBindingSources().map((s) => s.path)));

  type ZoneState = {
    input: ControllerInput;
    fullPath: string;
    isBound: boolean;
    isSelected: boolean;
    isMirrorSel: boolean;
    isHovered: boolean;
  };
  const zones = $derived(
    handInputs.map((input): ZoneState => {
      const fullPath = buildInputPath(hand, input.pathSuffix);
      const mirrored = getMirrorPath(profile, fullPath);
      return {
        input,
        fullPath,
        isBound: boundPaths.has(fullPath),
        isSelected: editor.selectedInput === fullPath,
        isMirrorSel:
          editor.mirrorMode && !!mirrored && editor.selectedInput === mirrored,
        isHovered:
          editor.hoveredInput === fullPath ||
          (editor.mirrorMode && !!mirrored && editor.hoveredInput === mirrored),
      };
    }),
  );

  function stroke(z: ZoneState): string {
    if (z.isSelected || z.isHovered) return "hsl(var(--primary))";
    if (z.isMirrorSel) return "hsl(var(--primary) / 0.4)";
    if (z.isBound) return "hsl(var(--primary) / 0.55)";
    return "hsl(var(--foreground) / 0.14)";
  }
  function fill(z: ZoneState): string {
    if (z.isSelected) return "hsl(var(--primary) / 0.18)";
    if (z.isHovered) return "hsl(var(--primary) / 0.12)";
    if (z.isMirrorSel) return "hsl(var(--primary) / 0.08)";
    if (z.isBound) return "hsl(var(--primary) / 0.1)";
    return "transparent";
  }
  function sw(z: ZoneState): number {
    if (z.isSelected) return 2.5;
    if (z.isHovered) return 2;
    if (z.isMirrorSel) return 1.8;
    if (z.isBound) return 1.2;
    return 0.8;
  }
  const innerLabel = (z: ZoneState) =>
    z.input.id.length === 1 && z.input.shape === "circle";
  function labelY(i: ControllerInput): number {
    if (i.shape === "rect" || i.shape === "pill") return i.cy + (i.h ?? 0) / 2 + 16;
    return i.cy + (i.r ?? 16) + 16;
  }
  function toggle(z: ZoneState) {
    setSelectedInput(z.isSelected ? null : z.fullPath);
  }
</script>

<div class="diagram">
  <div class="hand-label">{hand}</div>
  <svg
    viewBox={profile.svgViewBox}
    preserveAspectRatio="xMidYMid meet"
    style={isLeft ? "transform: scaleX(-1)" : ""}
  >
    {#if svgUrl}
      <image href={svgUrl} x="0" y="0" width={vb[2]} height={vb[3]} opacity="0.4" />
    {/if}

    {#each zones as z (z.input.id)}
      {@const i = z.input}
      {@const rot = i.rotation ? `rotate(${i.rotation} ${i.cx} ${i.cy})` : ""}
      <g
        class="zone"
        role="button"
        tabindex="-1"
        aria-label={z.input.label}
        onclick={() => toggle(z)}
        onkeydown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            toggle(z);
          }
        }}
        onmouseenter={() => setHoveredInput(z.fullPath)}
        onmouseleave={() => setHoveredInput(null)}
      >
        {#if i.shape === "circle"}
          <circle cx={i.cx} cy={i.cy} r={i.r ?? 16} fill={fill(z)} stroke={stroke(z)} stroke-width={sw(z)} />
        {:else if i.shape === "rect"}
          <rect x={i.cx - (i.w ?? 20) / 2} y={i.cy - (i.h ?? 20) / 2} width={i.w ?? 20} height={i.h ?? 20} rx="5" fill={fill(z)} stroke={stroke(z)} stroke-width={sw(z)} transform={rot} />
        {:else}
          <rect x={i.cx - (i.w ?? 30) / 2} y={i.cy - (i.h ?? 16) / 2} width={i.w ?? 30} height={i.h ?? 16} rx={Math.min(i.w ?? 30, i.h ?? 16) / 2} fill={fill(z)} stroke={stroke(z)} stroke-width={sw(z)} transform={rot} />
        {/if}

        {#if innerLabel(z)}
          <g transform={isLeft ? `translate(${i.cx},${i.cy}) scale(-1,1) translate(${-i.cx},${-i.cy})` : ""}>
            <text x={i.cx} y={i.cy + 4} text-anchor="middle" font-size="11" font-weight="600"
              fill={z.isSelected || z.isHovered ? "hsl(var(--primary))" : z.isBound ? "hsl(var(--primary) / 0.8)" : "hsl(var(--foreground) / 0.3)"}>
              {i.id.toUpperCase()}
            </text>
          </g>
        {/if}

        {#if !i.hideLabel}
          {@const ly = labelY(i)}
          <g transform={isLeft ? `translate(${i.cx},${ly}) scale(-1,1) translate(${-i.cx},${-ly})` : ""}>
            <text x={i.cx} y={ly} text-anchor="middle" font-size="9"
              fill={z.isSelected || z.isHovered ? "hsl(var(--primary))" : z.isBound ? "hsl(var(--primary) / 0.7)" : "hsl(var(--foreground) / 0.25)"}>
              {i.shortLabel}
            </text>
          </g>
        {/if}
      </g>
    {/each}
  </svg>
</div>

<style>
  .diagram {
    display: flex;
    flex-direction: column;
    align-items: center;
    height: 100%;
    min-height: 0;
    width: 100%;
    user-select: none;
  }
  .hand-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.15em;
    color: hsl(var(--muted));
    margin-bottom: 6px;
    flex: none;
  }
  svg {
    flex: 1;
    min-height: 0;
    width: 100%;
  }
  .zone {
    cursor: pointer;
  }
</style>
