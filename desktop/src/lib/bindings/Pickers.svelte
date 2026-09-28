<script lang="ts">
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import type { HandId } from "./api";
  import { bind, apply, save, close, switchController, undo } from "./store.svelte";
  import { inputLabel, kindWords, KIND_ICONS, MODE_ICONS, POSE_POINTS, prettyName } from "./labels";

  const e = $derived(bind.editing!);
  const popup = $derived(e.popup);
  const setName = $derived(e.manifest.sets.find((s) => s.key === e.set)?.name ?? "");
  const done = () => (e.popup = null);
  const inputName = (hand: HandId | null, id: string) => {
    const l = e.ctrl.inputs.find((d) => d.id === id)?.label ?? prettyName(id);
    return hand ? inputLabel(hand, l) : l;
  };

  // --- mode pickers
  const addDef = $derived(popup?.kind === "addMode" ? e.ctrl.inputs.find((d) => d.id === popup.input) : undefined);
  async function addMode(hand: HandId, input: string, mode: string) {
    const index = await apply({ op: "add", hand, input, mode });
    const def = e.ctrl.inputs.find((d) => d.id === input);
    // Straight on to what its first slot does.
    const first = bind.modes.find((m) => m.id === mode)?.slots.find((s) => s.key !== "touch" || def?.touch);
    e.popup = index !== null && first ? { kind: "action", index, slot: first.key } : null;
  }
  const changeSrc = $derived(popup?.kind === "changeMode" ? e.view?.sources.find((s) => s.index === popup.index) : undefined);
  const changeDef = $derived(changeSrc ? e.ctrl.inputs.find((d) => d.id === changeSrc.input) : undefined);

  // --- action picker
  const actSrc = $derived(popup?.kind === "action" ? e.view?.sources.find((s) => s.index === popup.index) : undefined);
  const actSlot = $derived(popup?.kind === "action" && actSrc ? bind.modes.find((m) => m.id === actSrc.mode)?.slots.find((s) => s.key === popup.slot) : undefined);
  const current = $derived(popup?.kind === "action" && actSrc ? (actSrc.outputs.find(([k]) => k === popup.slot)?.[1]?.toLowerCase() ?? null) : null);
  const choices = $derived.by(() => {
    if (popup?.kind !== "action" || !actSrc || !actSlot) return [];
    const others = (e.view?.sources ?? []).filter((s) => s.index !== actSrc.index);
    return e.manifest.actions
      .filter((a) => a.set === e.set && a.kind === actSlot.kind)
      .map((a) => {
        const used = others.flatMap((s) =>
          s.outputs
            .filter(([, o]) => o.toLowerCase() === a.key)
            .map(([k]) => `${inputName(s.hand, s.input)} ${(bind.modes.find((m) => m.id === s.mode)?.slots.find((x) => x.key === k)?.label ?? k).toLowerCase()}`),
        );
        return { key: a.key, name: a.name, mandatory: a.mandatory, used };
      })
      .sort((x, y) => x.name.localeCompare(y.name));
  });
  async function pick(action: string | null) {
    if (popup?.kind !== "action") return;
    await apply({ op: "bind", index: popup.index, slot: popup.slot, action });
    done();
  }

  // --- controller
  const subjectControllers = $derived(e.own ? bind.ownControllers.map((c) => c.ty) : (e.game?.controllers ?? []));
  const personalTypes = $derived(e.own ? bind.ownPersonal : (e.game?.personal ?? []));
  const controllerDefs = $derived(e.own ? bind.ownControllers : bind.gameControllers);

  // --- haptics / poses
  const vib = $derived(e.manifest.actions.filter((a) => a.set === e.set && a.kind === "vibration"));
  const poseActions = $derived(e.manifest.actions.filter((a) => a.set === e.set && a.kind === "pose"));
  const boolActions = $derived(e.manifest.actions.filter((a) => a.set === e.set && a.kind === "boolean"));

  // --- leave
  async function leaveWith(choice: "save" | "discard") {
    if (popup?.kind !== "leave") return;
    const to = popup.to;
    done();
    if (choice === "save") return save(to);
    undo();
    if (to === "list") close();
    else switchController(to.controller, true);
  }
</script>

{#snippet row(icon: string, title: string, sub: string, badge: string, current: boolean, onpick: () => void)}
  <button class="pick" class:current onclick={onpick}>
    <span class="chip"><Icon name={icon} size={18} /></span>
    <span class="txt">
      <span class="t">{title}</span>
      {#if sub}<span class="s">{sub}</span>{/if}
    </span>
    {#if badge}<span class="badge">{badge}</span>{/if}
    {#if current}<span class="check"><Icon name="check" size={17} /></span>{/if}
  </button>
{/snippet}

{#if popup?.kind === "addMode" && addDef}
  <Modal title={inputLabel(popup.hand, addDef.label)} sub="Use it as…" onclose={done}>
    {#each bind.modes.filter((m) => addDef.modes.includes(m.id)).sort((a, b) => addDef.modes.indexOf(a.id) - addDef.modes.indexOf(b.id)) as m (m.id)}
      {@render row(MODE_ICONS[m.id] ?? "circle", m.label, m.blurb, "", false, () => addMode(popup.hand, popup.input, m.id))}
    {/each}
  </Modal>
{:else if popup?.kind === "changeMode" && changeSrc && changeDef && changeSrc.hand}
  <Modal title={inputLabel(changeSrc.hand, changeDef.label)} sub="Use it as…" onclose={done}>
    {#each bind.modes.filter((m) => changeDef.modes.includes(m.id)).sort((a, b) => changeDef.modes.indexOf(a.id) - changeDef.modes.indexOf(b.id)) as m (m.id)}
      {@render row(MODE_ICONS[m.id] ?? "circle", m.label, m.blurb, "", m.id === changeSrc.mode, async () => {
        if (m.id !== changeSrc.mode) await apply({ op: "changeMode", index: changeSrc.index, mode: m.id });
        done();
      })}
    {/each}
  </Modal>
{:else if popup?.kind === "action" && actSrc && actSlot}
  <Modal title={`${inputName(actSrc.hand, actSrc.input)} · ${actSlot.label}`} sub={`What it does in ${setName}`} width={600} onclose={done}>
    {@render row("prohibit", "None", "Does nothing", "", current === null, () => pick(null))}
    {#if choices.length === 0}
      <div class="empty">
        <Icon name={KIND_ICONS[actSlot.kind] ?? "circle"} size={30} />
        <b>Nothing in {setName} fits here</b>
        <span>{actSlot.label} needs {kindWords(actSlot.kind)}</span>
      </div>
    {/if}
    {#each choices as c (c.key)}
      {@const sub = c.used.length === 0 ? "" : c.used.length === 1 ? `Also on ${c.used[0]}` : `Also on ${c.used[0]} and ${c.used.length - 1} more`}
      {@render row(KIND_ICONS[actSlot.kind] ?? "circle", c.name, sub, c.mandatory && !c.used.length && current !== c.key ? "Required" : "", current === c.key, () => pick(c.key))}
    {/each}
  </Modal>
{:else if popup?.kind === "controller"}
  <Modal title="Controller" sub="Each controller has its own binding" width={520} onclose={done}>
    {#each subjectControllers as ty (ty)}
      {@const c = controllerDefs.find((x) => x.ty === ty)}
      {#if c}
        {@render row(
          "game-controller",
          c.name,
          personalTypes.includes(ty) ? "Your personal binding" : e.own ? "Monadeck's default controls" : "The game's own binding",
          "",
          ty === e.ty,
          () => {
            done();
            switchController(ty);
          },
        )}
      {/if}
    {/each}
  </Modal>
{:else if popup?.kind === "haptics"}
  <Modal title="Haptics" sub={`Which vibration each hand plays in ${setName}`} width={620} onclose={done}>
    {#if vib.length === 0}
      <div class="empty"><Icon name="vibrate" size={30} /><b>No vibration in {setName}</b><span>The game doesn't vibrate the controllers here</span></div>
    {:else}
      {#each ["left", "right"] as const as hand, hi (hand)}
        {@const cur = e.view?.haptics[hi]?.toLowerCase() ?? null}
        <div class="group">{hand === "left" ? "Left" : "Right"} controller</div>
        <div class="chips">
          <button class="chipbtn" class:on={cur === null} onclick={() => apply({ op: "haptic", hand, action: null })}>Off</button>
          {#each vib as a (a.key)}
            <button class="chipbtn" class:on={cur === a.key} onclick={() => apply({ op: "haptic", hand, action: a.key })}>{a.name}</button>
          {/each}
        </div>
      {/each}
    {/if}
  </Modal>
{:else if popup?.kind === "poses"}
  <Modal title="Poses" sub={`Where ${setName} follows your hands`} width={640} onclose={done}>
    {#if poseActions.length === 0}
      <div class="empty"><Icon name="hand" size={30} /><b>No poses in {setName}</b><span>The game tracks your hands through another set, or not at all</span></div>
    {:else}
      {#each poseActions as a (a.key)}
        <div class="group big">{a.name}</div>
        {#each ["left", "right"] as const as hand (hand)}
          {@const cur = e.view?.poses.find((p) => p.hand === hand && p.action.toLowerCase() === a.key)?.point ?? null}
          <div class="pose-row">
            <span>{hand === "left" ? "Left hand" : "Right hand"}</span>
            <div class="seg">
              <button class:on={cur === null} onclick={() => apply({ op: "pose", action: a.key, hand, point: null })}>Off</button>
              {#each POSE_POINTS as [id, label] (id)}
                <button class:on={cur === id} onclick={() => apply({ op: "pose", action: a.key, hand, point: id })}>{label}</button>
              {/each}
            </div>
          </div>
        {/each}
      {/each}
    {/if}
  </Modal>
{:else if popup?.kind === "chords"}
  <Modal title="Chords" sub={`Buttons held together · ${setName}`} width={720} onclose={done}>
    {#each e.view?.chords ?? [] as c (c.index)}
      <div class="chord-card">
        <div class="chord-top">
          <span class="does">Does</span>
          {#each boolActions as a (a.key)}
            <button class="chipbtn" class:on={c.action.toLowerCase() === a.key} onclick={() => apply({ op: "chordAction", index: c.index, action: a.key })}>{a.name}</button>
          {/each}
          <span class="grow"></span>
          <button class="mini danger" title="Remove this chord" onclick={() => apply({ op: "removeChord", index: c.index })}><Icon name="trash" size={15} /></button>
        </div>
        {#each ["left", "right"] as const as hand (hand)}
          <div class="chips">
            <span class="hand">{hand === "left" ? "Left" : "Right"}</span>
            {#each e.ctrl.inputs.filter((d) => (d.side === "both" || d.side === hand) && d.modes.length) as d (d.id)}
              <button class="chipbtn" class:on={c.inputs.some((i) => i.hand === hand && i.input === d.id)} onclick={() => apply({ op: "chordToggle", index: c.index, hand, input: d.id })}>{d.label}</button>
            {/each}
          </div>
        {/each}
        {#if c.inputs.length === 1}<p class="warn"><Icon name="warning" size={14} /> Pick at least two buttons</p>{/if}
      </div>
    {:else}
      <div class="empty"><Icon name="link-simple" size={30} /><b>No chords yet</b><span>A chord does something while all its buttons are held</span></div>
    {/each}
    {#if boolActions.length}
      <button class="new" onclick={() => apply({ op: "addChord", action: boolActions[0].key })}><Icon name="plus" size={15} /> New chord</button>
    {/if}
  </Modal>
{:else if popup?.kind === "leave"}
  <Modal title="Save your changes?" sub={`You changed ${e.name}'s controls`} width={520} onclose={done}>
    {#if e.view?.blocker}<p class="warn"><Icon name="warning" size={14} /> {e.view.blocker}</p>{/if}
    <div class="actions">
      <button class="primary" disabled={!!e.view?.blocker} onclick={() => leaveWith("save")}><Icon name="floppy-disk" size={16} /> Save</button>
      <button class="danger-btn" onclick={() => leaveWith("discard")}><Icon name="trash" size={16} /> Don't save</button>
      <button class="neutral" onclick={done}>Keep editing</button>
    </div>
  </Modal>
{/if}

<style>
  .pick {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 46px;
    padding: 7px 12px;
    border-radius: 11px;
    border: 1px solid transparent;
    background: hsl(var(--foreground) / 0.03);
    color: hsl(var(--foreground));
    text-align: left;
  }
  .pick:hover {
    background: hsl(var(--foreground) / 0.08);
  }
  .pick.current {
    background: hsl(var(--primary) / 0.14);
    border-color: hsl(var(--primary) / 0.55);
  }
  .chip {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 9px;
    background: hsl(var(--foreground) / 0.06);
    color: hsl(var(--muted));
    flex: none;
  }
  .pick.current .chip,
  .pick:hover .chip {
    background: hsl(var(--primary) / 0.2);
    color: hsl(var(--primary));
  }
  .txt {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .t {
    font-size: 14px;
  }
  .s {
    font-size: 12px;
    color: hsl(var(--muted));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .badge {
    font-size: 11px;
    color: hsl(var(--warn));
    background: hsl(var(--warn) / 0.16);
    border-radius: 99px;
    padding: 2px 9px;
  }
  .check {
    color: hsl(var(--primary));
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    padding: 22px 0;
    color: hsl(var(--muted));
    font-size: 12.5px;
  }
  .empty b {
    color: hsl(var(--foreground));
    font-weight: 500;
    font-size: 14px;
  }
  .group {
    margin: 8px 2px 4px;
    font-size: 12.5px;
    color: hsl(var(--muted));
  }
  .group.big {
    font-size: 14.5px;
    color: hsl(var(--foreground));
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-bottom: 4px;
  }
  .chipbtn {
    border: none;
    border-radius: 99px;
    padding: 6px 13px;
    font-size: 12.5px;
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
  }
  .chipbtn:hover {
    background: hsl(var(--border));
  }
  .chipbtn.on {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
    font-weight: 600;
  }
  .pose-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 2px;
    font-size: 13px;
  }
  .seg {
    display: flex;
    gap: 2px;
    padding: 3px;
    background: hsl(var(--background));
    border-radius: 10px;
  }
  .seg button {
    border: none;
    background: transparent;
    color: hsl(var(--muted));
    border-radius: 7px;
    padding: 5px 12px;
    font-size: 12.5px;
  }
  .seg button:hover {
    color: hsl(var(--foreground));
  }
  .seg button.on {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
    font-weight: 600;
  }
  .chord-card {
    background: hsl(var(--background) / 0.55);
    border-radius: 13px;
    padding: 10px 12px;
    margin-bottom: 6px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .chord-top {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .does,
  .hand {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: hsl(var(--muted));
    width: 46px;
  }
  .grow {
    flex: 1;
  }
  .mini {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    border: none;
    background: transparent;
    color: hsl(var(--muted));
  }
  .mini.danger:hover {
    background: hsl(var(--danger) / 0.14);
    color: hsl(var(--danger));
  }
  .warn {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 2px 0;
    font-size: 12.5px;
    color: hsl(var(--warn));
  }
  .new {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    align-self: flex-start;
    margin-top: 4px;
    height: 34px;
    padding: 0 14px;
    border-radius: 9px;
    border: none;
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
    font-size: 13px;
  }
  .new:hover {
    background: hsl(var(--border));
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }
  .actions button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 36px;
    padding: 0 16px;
    border-radius: 10px;
    font-size: 13px;
    border: 1px solid transparent;
  }
  .primary {
    background: hsl(var(--primary));
    color: hsl(var(--primary-fg));
    font-weight: 600;
    border: none;
  }
  .primary:disabled {
    opacity: 0.45;
  }
  .danger-btn {
    background: transparent;
    border-color: hsl(var(--danger) / 0.6) !important;
    color: hsl(var(--danger));
  }
  .danger-btn:hover {
    background: hsl(var(--danger) / 0.12);
  }
  .neutral {
    background: hsl(var(--surface-2));
    color: hsl(var(--foreground));
  }
</style>
