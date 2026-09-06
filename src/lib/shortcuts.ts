

export type ShortcutId =
  | "palette"
  | "viewGraph"
  | "viewTreemap"
  | "viewMatrix"
  | "viewTimeline"
  | "focus"
  | "clearFocus"
  | "impact"
  | "openInEditor"
  | "collapseCluster"
  | "filter"
  | "rescan"
  | "help"
  | "export"
  | "settings"
  | "nextFinding"
  | "prevFinding";

export interface ShortcutSpec {
  id: ShortcutId;

  keys: string;
  label: string;
  group: "Global" | "Views" | "Graph" | "Selection" | "Findings";
}


export const MOD_KEY = "Ctrl";

export const SHORTCUTS: ShortcutSpec[] = [
  { id: "palette", keys: "ctrl+k", label: "Command palette", group: "Global" },
  { id: "filter", keys: "/", label: "Filter", group: "Global" },
  { id: "rescan", keys: "f5", label: "Re-scan repository", group: "Global" },
  { id: "help", keys: "?", label: "Where to start reading", group: "Global" },
  { id: "export", keys: "ctrl+e", label: "Export", group: "Global" },
  { id: "settings", keys: "ctrl+,", label: "Settings", group: "Global" },

  { id: "viewGraph", keys: "1", label: "Graph view", group: "Views" },
  { id: "viewTreemap", keys: "2", label: "Treemap view", group: "Views" },
  { id: "viewMatrix", keys: "3", label: "Matrix view", group: "Views" },
  { id: "viewTimeline", keys: "4", label: "Timeline view", group: "Views" },

  { id: "focus", keys: "f", label: "Focus selected", group: "Graph" },
  { id: "clearFocus", keys: "escape", label: "Clear focus", group: "Graph" },
  { id: "collapseCluster", keys: "c", label: "Collapse / expand cluster", group: "Graph" },

  { id: "impact", keys: "i", label: "Impact analysis", group: "Selection" },
  { id: "openInEditor", keys: "e", label: "Open in editor", group: "Selection" },

  { id: "nextFinding", keys: "]", label: "Next finding", group: "Findings" },
  { id: "prevFinding", keys: "[", label: "Previous finding", group: "Findings" },
];


const KEY_ALIASES: Record<string, string> = {
  " ": "space",
  esc: "escape",
  del: "delete",
  arrowup: "up",
  arrowdown: "down",
  arrowleft: "left",
  arrowright: "right",
};


export function bindingFor(e: KeyboardEvent): string {
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push("ctrl");
  if (e.shiftKey) parts.push("shift");
  if (e.altKey) parts.push("alt");

  const raw = e.key.toLowerCase();
  parts.push(KEY_ALIASES[raw] ?? raw);
  return parts.join("+");
}


export function isTypingTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el || typeof el.closest !== "function") return false;
  return Boolean(el.closest("input, textarea, select, [contenteditable='true']"));
}


export function isBareKey(keys: string): boolean {
  return !keys.includes("+");
}

export function shortcutLabel(keys: string): string {
  return keys
    .split("+")
    .map((part) => {
      if (part === "ctrl") return MOD_KEY;
      if (part.length === 1) return part.toUpperCase();
      return part.charAt(0).toUpperCase() + part.slice(1);
    })
    .join(" + ");
}
