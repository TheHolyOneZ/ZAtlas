import { create } from "zustand";
import { persist } from "zustand/middleware";

export type ViewId = "graph" | "treemap" | "matrix" | "timeline";

export const VIEWS: { id: ViewId; label: string; key: string }[] = [
  { id: "graph", label: "Graph", key: "1" },
  { id: "treemap", label: "Treemap", key: "2" },
  { id: "matrix", label: "Matrix", key: "3" },
  { id: "timeline", label: "Timeline", key: "4" },
];

interface ViewState {
  view: ViewId;

  showCycles: boolean;
  showHotspots: boolean;
  showOrphans: boolean;
  showTypeOnly: boolean;
  minLoc: number;

  query: string;

  collapsed: Set<number>;

  setView: (view: ViewId) => void;
  set: <K extends keyof ViewState>(key: K, value: ViewState[K]) => void;
  toggleCollapsed: (moduleId: number) => void;
  reset: () => void;
}

export const useViewStore = create<ViewState>()(
  persist(
    (set) => ({
      view: "graph",
      showCycles: true,
      showHotspots: true,
      showOrphans: false,
      showTypeOnly: true,
      minLoc: 0,
      query: "",
      collapsed: new Set<number>(),

      setView: (view) => set({ view }),
      set: (key, value) => set({ [key]: value } as never),
      toggleCollapsed: (moduleId) =>
        set((s) => {
          const next = new Set(s.collapsed);
          if (next.has(moduleId)) next.delete(moduleId);
          else next.add(moduleId);
          return { collapsed: next };
        }),
      reset: () =>
        set({
          showCycles: true,
          showHotspots: true,
          showOrphans: false,
          showTypeOnly: true,
          minLoc: 0,
          query: "",
          collapsed: new Set<number>(),
        }),
    }),
    {
      name: "zatlas.view",

      partialize: (s) => ({
        view: s.view,
        showCycles: s.showCycles,
        showHotspots: s.showHotspots,
        showOrphans: s.showOrphans,
        showTypeOnly: s.showTypeOnly,
        minLoc: s.minLoc,
      }),
    },
  ),
);
