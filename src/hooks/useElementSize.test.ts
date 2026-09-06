import { beforeEach, describe, expect, it, vi } from "vitest";

import { observeSize } from "./useElementSize";

let observed: Element[] = [];
let disconnected = 0;
let trigger: (() => void) | null = null;

beforeEach(() => {
  observed = [];
  disconnected = 0;
  trigger = null;
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(cb: () => void) {
        trigger = cb;
      }
      observe(node: Element) {
        observed.push(node);
      }
      disconnect() {
        disconnected++;
      }
      unobserve() {}
    },
  );
});

function nodeOf(w: number, h: number): HTMLElement {
  const el = document.createElement("div");
  Object.defineProperty(el, "clientWidth", { value: w, configurable: true });
  Object.defineProperty(el, "clientHeight", { value: h, configurable: true });
  return el;
}

describe("observeSize", () => {
  it("reports the size immediately, without waiting for a resize", () => {


    const seen: { w: number; h: number }[] = [];
    observeSize(nodeOf(800, 600), (s) => seen.push(s));
    expect(seen).toEqual([{ w: 800, h: 600 }]);
  });

  it("observes the node it was given", () => {
    const node = nodeOf(10, 10);
    observeSize(node, () => {});
    expect(observed).toEqual([node]);
  });

  it("reports again when the observer fires", () => {
    const node = nodeOf(100, 100);
    const seen: { w: number; h: number }[] = [];
    observeSize(node, (s) => seen.push(s));
    Object.defineProperty(node, "clientWidth", { value: 250, configurable: true });
    trigger?.();
    expect(seen).toHaveLength(2);
    expect(seen[1]).toEqual({ w: 250, h: 100 });
  });

  it("disconnects when disposed, so a remount does not leak an observer", () => {
    const dispose = observeSize(nodeOf(1, 1), () => {});
    dispose();
    expect(disconnected).toBe(1);
  });
});
