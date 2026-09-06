import { create } from "zustand";

import { useRecentStore } from "./useRecentStore";

import {
  api,
  describeError,
  EVENTS,
  on,
  type ScanProgress,
  type ScanSummary,
} from "../lib/tauri";

export type ScanState = "idle" | "scanning" | "ready" | "failed";

interface RepoState {
  state: ScanState;

  watching: boolean;

  stale: boolean;

  roots: string[];
  summary: ScanSummary | null;
  scanId: string | null;
  progress: ScanProgress | null;
  error: string | null;

  version: number;

  open: (path: string) => Promise<void>;
  pick: () => Promise<void>;
  addRepo: () => Promise<void>;
  removeRepo: (path: string) => Promise<void>;
  rescan: () => Promise<void>;
  cancel: () => Promise<void>;
  setWatching: (enabled: boolean) => Promise<void>;
  subscribe: () => Promise<() => void>;
}

export const useRepoStore = create<RepoState>((set, get) => ({
  state: "idle",
  watching: false,
  stale: false,
  roots: [],
  summary: null,
  scanId: null,
  progress: null,
  error: null,
  version: 0,

  open: async (path) => {
    set({
      state: "scanning",
      error: null,
      progress: null,
      summary: null,
      stale: false,
      roots: [path],
    });
    try {
      const scanId = await api.startScan(path);
      set({ scanId });
    } catch (e) {
      set({ state: "failed", error: describeError(e) });
    }
  },

  pick: async () => {
    try {
      const path = await api.pickRepo();
      if (path) await get().open(path);
    } catch (e) {
      set({ state: "failed", error: describeError(e) });
    }
  },


  removeRepo: async (path) => {
    const remaining = get().roots.filter((r) => r !== path);
    if (remaining.length === 0) {
      set({
        state: "idle",
        summary: null,
        roots: [],
        scanId: null,
        progress: null,
        error: null,
        stale: false,
      });
      return;
    }
    if (remaining.length === 1) {
      await get().open(remaining[0]!);
      return;
    }
    set({ state: "scanning", error: null, progress: null, stale: false, roots: remaining });
    try {
      const scanId = await api.startWorkspaceScan(remaining);
      set({ scanId });
    } catch (e) {
      set({ state: "failed", error: describeError(e) });
    }
  },


  addRepo: async () => {
    try {
      const path = await api.pickRepo();
      if (!path) return;


      if (get().roots.includes(path)) return;
      const roots = [...get().roots, path];
      set({
        state: "scanning",
        error: null,
        progress: null,
        summary: null,
        stale: false,
        roots,
      });
      const scanId = await api.startWorkspaceScan(roots);
      set({ scanId });
    } catch (e) {
      set({ state: "failed", error: describeError(e) });
    }
  },

  rescan: async () => {
    const roots = get().roots;
    if (roots.length > 1) {
      set({ state: "scanning", error: null, progress: null, stale: false });
      const scanId = await api.startWorkspaceScan(roots);
      set({ scanId });
      return;
    }
    const root = roots[0] ?? get().summary?.root;
    if (root) await get().open(root);
  },

  cancel: async () => {
    const id = get().scanId;
    if (id) await api.cancelScan(id);
  },

  setWatching: async (enabled) => {
    const on = await api.setWatching(enabled);
    set({ watching: on, stale: enabled ? get().stale : false });
  },


  subscribe: async () => {
    let cancelled = false;
    const unlisteners = await Promise.all([
      on<ScanProgress>(EVENTS.scanProgress, (p) => {
        if (get().scanId === p.scanId) set({ progress: p });
      }),
      on<ScanSummary>(EVENTS.scanDone, (summary) => {


        if (get().roots.length === 1) {
          useRecentStore.getState().remember(summary.root, summary.name);
        }
        set((s) => ({
          state: "ready",
          summary,
          progress: null,
          error: null,
          stale: false,
          version: s.version + 1,
        }));
      }),
      on<{ message?: string }>(EVENTS.scanFailed, (e) => {
        set({ state: "failed", error: e?.message ?? "Scan failed", progress: null });
      }),
      on<string>(EVENTS.scanCancelled, () => {
        set({ state: "idle", progress: null, scanId: null });
      }),


      on<string[]>(EVENTS.repoChanged, () => {
        if (get().state === "ready") set({ stale: true });
      }),
    ]);

    if (cancelled) {
      unlisteners.forEach((fn) => fn());
      return () => {};
    }
    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  },
}));
