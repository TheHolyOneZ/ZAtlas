import { create } from "zustand";
import { persist } from "zustand/middleware";

export const THEMES = [
  { id: "cartograph", label: "Cartograph" },
  { id: "topographic-light", label: "Topographic Light" },
  { id: "slate", label: "Slate" },
  { id: "contrast", label: "Contrast" },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];


export type LspChoice = "off" | "repair" | "verify" | null;

interface SettingsState {
  theme: ThemeId;

  contours: boolean;
  lspMode: LspChoice;
  setTheme: (theme: ThemeId) => void;
  setContours: (on: boolean) => void;
  setLspMode: (mode: LspChoice) => void;
}

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      theme: "cartograph",
      contours: true,
      lspMode: null,
      setTheme: (theme) => {
        document.documentElement.dataset.theme = theme;
        set({ theme });
      },
      setContours: (contours) => set({ contours }),
      setLspMode: (lspMode) => set({ lspMode }),
    }),
    { name: "zatlas.settings" },
  ),
);


export function initTheme(): void {
  document.documentElement.dataset.theme = useSettingsStore.getState().theme;
}
