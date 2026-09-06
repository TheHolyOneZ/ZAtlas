import { create } from "zustand";
import { persist } from "zustand/middleware";


const LIMIT = 8;

export interface RecentRepo {
  path: string;

  name: string;


  openedAt: number;
}

interface RecentState {
  recents: RecentRepo[];
  remember: (path: string, name: string) => void;
  forget: (path: string) => void;
}

export const useRecentStore = create<RecentState>()(
  persist(
    (set) => ({
      recents: [],
      remember: (path, name) =>
        set((s) => ({


          recents: [
            { path, name, openedAt: Math.floor(Date.now() / 1000) },
            ...s.recents.filter((r) => r.path !== path),
          ].slice(0, LIMIT),
        })),
      forget: (path) =>
        set((s) => ({ recents: s.recents.filter((r) => r.path !== path) })),
    }),
    { name: "zatlas.recents" },
  ),
);


export function lastRepo(): string | null {
  return useRecentStore.getState().recents[0]?.path ?? null;
}
