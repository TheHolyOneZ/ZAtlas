import { create } from "zustand";

import { api, describeError, type GraphPayload, type LayoutPayload } from "../lib/tauri";

interface GraphState {
  graph: GraphPayload | null;
  layout: LayoutPayload | null;

  positions: Float32Array | null;
  radii: Float32Array | null;
  loading: boolean;
  error: string | null;

  load: () => Promise<void>;
  clear: () => void;
}


let sequence = 0;

export const useGraphStore = create<GraphState>((set) => ({
  graph: null,
  layout: null,
  positions: null,
  radii: null,
  loading: false,
  error: null,

  load: async () => {
    const mine = ++sequence;
    set({ loading: true, error: null });
    try {
      const [graph, layout] = await Promise.all([api.getGraph(), api.getLayout()]);
      if (mine !== sequence) return;
      set({
        graph,
        layout,
        positions: Float32Array.from(layout.xy),
        radii: Float32Array.from(layout.radius),
        loading: false,
      });
    } catch (e) {
      if (mine !== sequence) return;
      set({ loading: false, error: describeError(e) });
    }
  },

  clear: () => {
    sequence++;
    set({ graph: null, layout: null, positions: null, radii: null, error: null });
  },
}));
