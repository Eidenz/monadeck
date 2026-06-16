<script lang="ts">
  let {
    checked = false,
    label = "Toggle",
    onchange,
  }: { checked?: boolean; label?: string; onchange: (v: boolean) => void } =
    $props();
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  class="sw"
  class:on={checked}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .sw {
    flex: none;
    width: 38px;
    height: 22px;
    padding: 0;
    border-radius: 999px;
    border: 1px solid hsl(var(--border));
    background: hsl(var(--surface-2));
    position: relative;
    cursor: pointer;
    transition:
      background 0.15s ease,
      border-color 0.15s ease;
  }
  .sw.on {
    background: hsl(var(--primary) / 0.9);
    border-color: transparent;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: hsl(var(--foreground));
    transition: transform 0.15s ease;
  }
  .sw.on .knob {
    transform: translateX(16px);
    background: #fff;
  }
</style>
