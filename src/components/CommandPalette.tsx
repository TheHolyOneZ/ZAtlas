import { useEffect, useMemo, useRef, useState } from "react";
import { Command } from "cmdk";
import { FileCode, Layers, Search } from "lucide-react";

import { severityCss } from "../lib/severity";
import { useGraphStore } from "../store/useGraphStore";
import { useSelectionStore } from "../store/useSelectionStore";
import { useViewStore, VIEWS, type ViewId } from "../store/useViewStore";


export default function CommandPalette({ onClose }: { onClose: () => void }) {
  const { graph } = useGraphStore();
  const { select } = useSelectionStore();
  const { setView } = useViewStore();
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {

    const raf = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(raf);
  }, []);


  const files = useMemo(() => {
    if (!graph) return [];
    const q = query.toLowerCase().trim();
    const matched = q
      ? graph.nodes.filter((n) => n.path.toLowerCase().includes(q))
      : graph.nodes;


    return [...matched]
      .sort((a, b) => b.fanIn - a.fanIn || a.path.localeCompare(b.path))
      .slice(0, 60);
  }, [graph, query]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[12vh]"
      style={{ background: "rgba(0,0,0,0.45)" }}
      onClick={onClose}
    >

      <Command
        label="Command palette"
        shouldFilter={false}
        onClick={(e: React.MouseEvent) => e.stopPropagation()}
        className="w-[620px] max-w-[92vw] rounded-lg overflow-hidden"
        style={{
          background: "var(--raised)",
          border: "1px solid var(--border-strong)",
          boxShadow: "var(--shadow)",
          backdropFilter: "blur(24px) saturate(140%)",
        }}
      >
        <div
          className="flex items-center gap-2 px-3 h-11"
          style={{ borderBottom: "1px solid var(--border)" }}
        >
          <Search size={14} style={{ color: "var(--text-3)" }} />
          <Command.Input
            ref={inputRef}
            value={query}
            onValueChange={setQuery}
            placeholder="Jump to a file, or switch view…"
            className="mono flex-1 bg-transparent outline-none"
            style={{ color: "var(--text)" }}
          />
          <kbd className="mono text-[10px]" style={{ color: "var(--text-3)" }}>
            esc
          </kbd>
        </div>

        <Command.List className="max-h-[52vh] overflow-y-auto py-1">
          <Command.Empty className="px-3 py-6 text-center text-[12px]" style={{ color: "var(--text-3)" }}>
            Nothing matches “{query}”.
          </Command.Empty>

          {query.length === 0 && (
            <Command.Group heading="Views" className="px-1">
              {VIEWS.map((v) => (
                <Row
                  key={v.id}
                  value={`view-${v.id}`}
                  onSelect={() => {
                    setView(v.id as ViewId);
                    onClose();
                  }}
                  icon={<Layers size={13} />}
                  label={v.label}
                  hint={v.key}
                />
              ))}
            </Command.Group>
          )}

          {files.length > 0 && (
            <Command.Group heading="Files" className="px-1">
              {files.map((n) => (
                <Row
                  key={n.id}
                  value={`file-${n.id}`}
                  onSelect={() => {
                    void select(n.id);
                    onClose();
                  }}
                  icon={
                    <FileCode size={13} style={{ color: severityCss(n.heat) }} />
                  }
                  label={n.path}
                  hint={`${n.fanIn} dependents · ${n.loc} LOC`}
                />
              ))}
            </Command.Group>
          )}
        </Command.List>
      </Command>
    </div>
  );
}

function Row({
  value,
  onSelect,
  icon,
  label,
  hint,
}: {
  value: string;
  onSelect: () => void;
  icon: React.ReactNode;
  label: string;
  hint?: string;
}) {
  return (
    <Command.Item
      value={value}
      onSelect={onSelect}
      className="flex items-center gap-2 px-2 h-8 rounded cursor-pointer data-[selected=true]:bg-[var(--row-selected)]"
    >
      <span className="shrink-0" style={{ color: "var(--text-3)" }}>
        {icon}
      </span>
      <span className="mono truncate" style={{ color: "var(--text)" }}>
        {label}
      </span>
      {hint && (
        <span className="mono ml-auto shrink-0 text-[11px]" style={{ color: "var(--text-3)" }}>
          {hint}
        </span>
      )}
    </Command.Item>
  );
}
