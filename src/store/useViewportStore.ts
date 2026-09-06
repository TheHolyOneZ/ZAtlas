import { create } from "zustand";

import { IDENTITY, type Viewport } from "../lib/viewport";


interface ViewportState {
  viewport: Viewport;
  setViewport: (viewport: Viewport) => void;

  updateViewport: (fn: (current: Viewport) => Viewport) => void;

  restored: number;
}

export const useViewportStore = create<ViewportState>((set) => ({
  viewport: IDENTITY,
  setViewport: (viewport) => set({ viewport }),
  updateViewport: (fn) => set((s) => ({ viewport: fn(s.viewport) })),
  restored: 0,
}));


export function restoreViewport(viewport: Viewport): void {
  useViewportStore.setState((s) => ({ viewport, restored: s.restored + 1 }));
}
