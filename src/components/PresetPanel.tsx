import { useState } from "react";
import { Bookmark, Plus, X } from "lucide-react";

import { CornerTicks, RailLabel, TextInput } from "./ui";
import { usePresetStore, type Preset } from "../store/usePresetStore";
import { useRepoStore } from "../store/useRepoStore";
import { restoreViewport, useViewportStore } from "../store/useViewportStore";
import { useViewStore } from "../store/useViewStore";


const NONE: Preset[] = [];

export default function PresetPanel() {
  const repo = useRepoStore((s) => s.summary?.root ?? "");
  const presets = usePresetStore((s) => (repo ? (s.byRepo[repo] ?? NONE) : NONE));
  const save = usePresetStore((s) => s.save);
  const remove = usePresetStore((s) => s.remove);

  const view = useViewStore();
  const camera = useViewportStore((s) => s.viewport);

  const [naming, setNaming] = useState(false);
  const [name, setName] = useState("");

  if (!repo) return null;

  function apply(preset: Preset) {
    view.setView(preset.view);
    view.set("query", preset.query);
    view.set("showCycles", preset.showCycles);
    view.set("showHotspots", preset.showHotspots);
    view.set("showOrphans", preset.showOrphans);
    view.set("showTypeOnly", preset.showTypeOnly);
    view.set("minLoc", preset.minLoc);
    if (preset.camera) restoreViewport(preset.camera);
  }

  function commit() {
    const trimmed = name.trim();
    if (!trimmed) {
      setNaming(false);
      return;
    }
    save(repo, {
      name: trimmed,
      view: view.view,
      query: view.query,
      showCycles: view.showCycles,
      showHotspots: view.showHotspots,
      showOrphans: view.showOrphans,
      showTypeOnly: view.showTypeOnly,
      minLoc: view.minLoc,


      camera: view.view === "graph" ? camera : undefined,
    });
    setName("");
    setNaming(false);
  }

  return (
    <>
      <RailLabel>Saved views</RailLabel>

      {presets.length === 0 && !naming && (
        <p className="px-4 pb-1 text-[11px] leading-snug" style={{ color: "var(--text-3)" }}>
          Set up a view you will want again, then save it here.
        </p>
      )}

      {presets.map((p) => (
        <div key={p.name} className="group/preset relative flex items-center">
          <button
            type="button"
            onClick={() => apply(p)}
            title={`${p.view}${p.query ? ` · "${p.query}"` : ""}${
              p.minLoc > 0 ? ` · ≥${p.minLoc} LOC` : ""
            }`}
            className="flex items-center gap-2.5 flex-1 min-w-0 px-4 h-7 text-left text-[12px] transition-colors duration-100 hover:bg-[var(--row-hover)]"
            style={{ color: "var(--text-2)" }}
          >
            <Bookmark size={11} className="shrink-0" style={{ color: "var(--text-3)" }} />
            <span className="truncate">{p.name}</span>
          </button>
          <button
            type="button"
            onClick={() => remove(repo, p.name)}
            aria-label={`Delete ${p.name}`}
            title="Delete this saved view"
            className="absolute right-2 grid place-items-center w-5 h-5 rounded-[3px] opacity-0 group-hover/preset:opacity-100 focus:opacity-100 transition-opacity duration-100"
            style={{ color: "var(--text-3)", background: "var(--surface)" }}
          >
            <X size={11} />
          </button>
        </div>
      ))}

      {naming ? (
        <div className="px-4 pt-1.5 pb-2">
          <TextInput
            value={name}
            onChange={setName}
            placeholder="name this view…"
            mono={false}
          />
          <div className="flex gap-2 mt-1.5">
            <button
              type="button"
              onClick={commit}
              className="relative px-2 h-6 rounded-[3px] text-[11px]"
              style={{
                background: "var(--accent-soft)",
                border: "1px solid var(--accent)",
                color: "var(--accent)",
              }}
            >
              Save
              <CornerTicks inset={2} />
            </button>
            <button
              type="button"
              onClick={() => {
                setName("");
                setNaming(false);
              }}
              className="px-2 h-6 rounded-[3px] text-[11px]"
              style={{ border: "1px solid var(--border)", color: "var(--text-3)" }}
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setNaming(true)}
          className="flex items-center gap-2.5 w-full px-4 h-7 text-left text-[12px] transition-colors duration-100 hover:bg-[var(--row-hover)]"
          style={{ color: "var(--text-3)" }}
        >
          <Plus size={11} className="shrink-0" />
          Save this view
        </button>
      )}
    </>
  );
}
