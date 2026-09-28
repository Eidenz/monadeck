// What joins an input's list entry to its part of the drawings: the input under
// the pointer (either place), and where each entry and part is on screen. Keys
// are `<hand>:<input>`.
import { getContext, setContext } from "svelte";

export class Links {
  hover = $state<string | null>(null);
  anchors = new Map<string, HTMLElement>();
  spots = new Map<string, HTMLElement>();

  enter(key: string) {
    this.hover = key;
  }

  leave(key: string) {
    if (this.hover === key) this.hover = null;
  }

  /** Whether an entry is in view in its scrolling list. */
  inView(key: string): boolean {
    const a = this.anchors.get(key);
    const list = a?.closest("[data-list]");
    if (!a || !list) return false;
    const r = a.getBoundingClientRect();
    const l = list.getBoundingClientRect();
    return r.bottom > l.top + 6 && r.top < l.bottom - 6;
  }

  /** Scroll an entry into the middle of its list. */
  reveal(key: string) {
    this.anchors.get(key)?.scrollIntoView({ block: "center", behavior: "smooth" });
  }
}

const KEY = Symbol("binding-links");
export const provideLinks = (l: Links) => setContext(KEY, l);
export const useLinks = () => getContext<Links>(KEY);

/** `use:register={[map, key]}`: keep an element in a Links map. */
export function register(el: HTMLElement, [map, key]: [Map<string, HTMLElement>, string]) {
  map.set(key, el);
  return {
    update([m, k]: [Map<string, HTMLElement>, string]) {
      if (m.get(key) === el) m.delete(key);
      m.set(k, el);
      key = k;
      map = m;
    },
    destroy() {
      if (map.get(key) === el) map.delete(key);
    },
  };
}
