import { create } from "zustand";

import { api, type EdgeDetail, type ImpactResult, type NodeDetail } from "../lib/tauri";

interface SelectionState {


  selected: number | null;
  detail: NodeDetail | null;
  loadingDetail: boolean;

  highlighted: Set<number>;
  impact: ImpactResult | null;
  focusMode: boolean;


  edge: EdgeDetail | null;

  select: (id: number | null) => Promise<void>;
  selectEdge: (from: number, to: number) => Promise<void>;
  runImpact: () => Promise<void>;
  clearHighlight: () => void;
  setFocusMode: (on: boolean) => void;
  highlight: (ids: number[]) => void;
}

let detailSequence = 0;

export const useSelectionStore = create<SelectionState>((set, get) => ({
  selected: null,
  detail: null,
  loadingDetail: false,
  highlighted: new Set(),
  impact: null,
  focusMode: false,
  edge: null,

  select: async (id) => {
    if (id === null) {
      detailSequence++;
      set({ selected: null, detail: null, impact: null, highlighted: new Set(), edge: null });
      return;
    }
    const mine = ++detailSequence;
    set({ selected: id, loadingDetail: true, impact: null, edge: null });
    try {
      const detail = await api.getNode(id);
      if (mine !== detailSequence) return;
      set({ detail, loadingDetail: false });
    } catch {
      if (mine !== detailSequence) return;
      set({ detail: null, loadingDetail: false });
    }
  },

  selectEdge: async (from, to) => {
    const mine = ++detailSequence;


    set({
      selected: null,
      detail: null,
      impact: null,
      highlighted: new Set([from, to]),
    });
    try {
      const edge = await api.edgeDetail(from, to);
      if (mine !== detailSequence) return;
      set({ edge });
    } catch {
      if (mine !== detailSequence) return;
      set({ edge: null, highlighted: new Set() });
    }
  },

  runImpact: async () => {
    const id = get().selected;
    if (id === null) return;
    const result = await api.impact([id]);
    set({
      impact: result,
      highlighted: new Set([id, ...result.affected]),
    });
  },

  clearHighlight: () =>
    set({ highlighted: new Set(), impact: null, focusMode: false, edge: null }),
  setFocusMode: (on) => set({ focusMode: on }),
  highlight: (ids) => set({ highlighted: new Set(ids) }),
}));
