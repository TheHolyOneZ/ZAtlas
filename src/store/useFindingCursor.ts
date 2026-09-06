import { create } from "zustand";


interface CursorState {

  cursor: number;
  count: number;
  setCount: (count: number) => void;
  step: (delta: number) => void;
  reset: () => void;
}

export const useFindingCursor = create<CursorState>((set) => ({
  cursor: -1,
  count: 0,
  setCount: (count) =>
    set((s) => ({
      count,


      cursor: s.cursor >= count ? count - 1 : s.cursor,
    })),
  step: (delta) =>
    set((s) => {
      if (s.count === 0) return { cursor: -1 };


      if (s.cursor < 0) return { cursor: delta > 0 ? 0 : s.count - 1 };


      return { cursor: Math.min(s.count - 1, Math.max(0, s.cursor + delta)) };
    }),
  reset: () => set({ cursor: -1 }),
}));
