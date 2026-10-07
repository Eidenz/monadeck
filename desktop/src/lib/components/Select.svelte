<script lang="ts">
  // A dropdown drawn in the page. WebKitGTK shows a native <select>'s list as a
  // menu of its own, and picking from it could land a click on the page
  // underneath (in Settings, the General tab); this list belongs to the page.
  import type { SelectOption } from "$lib/types";

  let {
    value,
    options,
    onchange,
    label = "",
    placeholder = "",
    disabled = false,
    wide = false,
    small = false,
  }: {
    value: string;
    options: SelectOption[];
    onchange: (value: string) => void;
    label?: string;
    placeholder?: string;
    disabled?: boolean;
    /** Fill the width it's given. */
    wide?: boolean;
    /** The compact size of an inline control. */
    small?: boolean;
  } = $props();

  let trigger = $state<HTMLButtonElement>();
  let open = $state(false);
  // Keyboard highlight while open.
  let active = $state(-1);
  let pos = $state({ left: 0, top: 0, width: 0, up: false, maxH: 280 });

  const current = $derived(options.find((o) => o.value === value));

  function show() {
    if (disabled || !trigger) return;
    const r = trigger.getBoundingClientRect();
    const below = window.innerHeight - r.bottom - 10;
    const above = r.top - 10;
    const up = below < 160 && above > below;
    pos = {
      left: r.left,
      top: up ? r.top - 4 : r.bottom + 4,
      width: r.width,
      up,
      maxH: Math.max(120, Math.min(300, up ? above : below)),
    };
    active = Math.max(0, options.findIndex((o) => o.value === value));
    open = true;
  }

  function close() {
    open = false;
    trigger?.focus();
  }

  function pick(v: string) {
    close();
    if (v !== value) onchange(v);
  }

  function onKey(e: KeyboardEvent) {
    if (!open) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(e.key)) {
        e.preventDefault();
        show();
      }
      return;
    }
    if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      active = Math.min(options.length - 1, active + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      active = Math.max(0, active - 1);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (options[active]) pick(options[active].value);
    }
  }

  // Keep the highlighted option in view.
  function reveal(node: HTMLElement, on: boolean) {
    if (on) node.scrollIntoView({ block: "nearest" });
    return {
      update(now: boolean) {
        if (now) node.scrollIntoView({ block: "nearest" });
      },
    };
  }
</script>

<svelte:window onresize={() => (open = false)} />

<button
  bind:this={trigger}
  type="button"
  class="trigger"
  class:open
  class:wide
  class:small
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-label={label}
  {disabled}
  onclick={() => (open ? close() : show())}
  onkeydown={onKey}
>
  <span class="text" class:placeholder={!current}>{current?.label ?? placeholder}</span>
  <svg class="caret" viewBox="0 0 10 6" aria-hidden="true"><path d="M1 1l4 4 4-4" /></svg>
</button>

{#if open}
  <!-- Takes the click (and the wheel) that closes the list, so nothing under it gets them. -->
  <div class="backdrop" role="presentation" onclick={close} onwheel={(e) => e.preventDefault()}></div>
  <ul
    class="list"
    class:up={pos.up}
    role="listbox"
    aria-label={label}
    style:left="{pos.left}px"
    style:top="{pos.top}px"
    style:min-width="{pos.width}px"
    style:max-height="{pos.maxH}px"
  >
    {#each options as o, i (o.value)}
      <li
        role="option"
        aria-selected={o.value === value}
        class:selected={o.value === value}
        class:active={i === active}
        use:reveal={i === active}
        onclick={() => pick(o.value)}
        onkeydown={onKey}
        onpointerenter={() => (active = i)}
      >
        {o.label}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .trigger {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-width: 0;
    background: hsl(var(--surface-2));
    border: 1px solid hsl(var(--border));
    color: hsl(var(--foreground));
    border-radius: var(--radius-s);
    padding: 7px 9px;
    font-size: 12.5px;
    font-family: inherit;
    text-align: left;
  }
  .trigger.wide {
    display: flex;
    width: 100%;
  }
  .trigger.small {
    padding: 4px 8px;
    font-size: 12px;
  }
  .trigger:focus-visible,
  .trigger.open {
    outline: none;
    border-color: hsl(var(--primary));
  }
  .trigger:disabled {
    opacity: 0.55;
  }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text.placeholder {
    color: hsl(var(--muted));
  }
  .caret {
    flex: none;
    width: 10px;
    height: 6px;
    fill: none;
    stroke: hsl(var(--muted));
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
    transition: transform 0.12s;
  }
  .open .caret {
    transform: rotate(180deg);
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 1000;
  }
  .list {
    position: fixed;
    z-index: 1001;
    margin: 0;
    padding: 4px;
    list-style: none;
    overflow-y: auto;
    background: hsl(var(--surface));
    border: 1px solid hsl(var(--border));
    border-radius: var(--radius-s);
    box-shadow: 0 12px 30px hsl(0 0% 0% / 0.45);
    max-width: calc(100vw - 20px);
  }
  .list.up {
    transform: translateY(-100%);
  }
  li {
    padding: 7px 10px;
    border-radius: calc(var(--radius-s) - 2px);
    font-size: 12.5px;
    color: hsl(var(--foreground));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: default;
  }
  li.active {
    background: hsl(var(--surface-2));
  }
  li.selected {
    color: hsl(var(--primary));
    font-weight: 600;
  }
</style>
