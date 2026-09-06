import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { Viewport } from "../lib/viewport";
import type { ViewId } from "./useViewStore";


export interface Preset {
  name: string;
  view: ViewId;
  query: string;
  showCycles: boolean;
  showHotspots: boolean;
  showOrphans: boolean;
  showTypeOnly: boolean;
  minLoc: number;

  camera?: Viewport;
}

interface PresetState {


  byRepo: Record<string, Preset[]>;
  save: (repo: string, preset: Preset) => void;
  remove: (repo: string, name: string) => void;
  list: (repo: string) => Preset[];
}

export const usePresetStore = create<PresetState>()(
  persist(
    (set, get) => ({
      byRepo: {},
      save: (repo, preset) =>
        set((s) => {
          const existing = s.byRepo[repo] ?? [];


          const without = existing.filter((p) => p.name !== preset.name);
          return {
            byRepo: {
              ...s.byRepo,
              [repo]: [...without, preset].sort((a, b) => a.name.localeCompare(b.name)),
            },
          };
        }),
      remove: (repo, name) =>
        set((s) => ({
          byRepo: {
            ...s.byRepo,
            [repo]: (s.byRepo[repo] ?? []).filter((p) => p.name !== name),
          },
        })),
      list: (repo) => get().byRepo[repo] ?? [],
    }),
    { name: "zatlas.presets" },
  ),
);
