import { useEffect, useState } from "react";
import { X } from "lucide-react";

import { Empty, Spinner } from "./ui";
import { api, describeError, type SymbolGraph } from "../lib/tauri";
import { useSelectionStore } from "../store/useSelectionStore";


export default function SymbolPanel({
  module,
  label,
  onClose,
}: {
  module: number;
  label: string;
  onClose: () => void;
}) {
  const [graph, setGraph] = useState<SymbolGraph | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { select } = useSelectionStore();

  useEffect(() => {
    let cancelled = false;
    void api
      .getSymbols(module)
      .then((g) => {
        if (!cancelled) setGraph(g);
      })
      .catch((e) => {
        if (!cancelled) setError(describeError(e));
      });
    return () => {
      cancelled = true;
    };
  }, [module]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const callsFrom = (id: number) =>
    (graph?.edges ?? []).filter((e) => e.from === id).map((e) => e.to);
  const callersOf = (id: number) =>
    (graph?.edges ?? []).filter((e) => e.to === id).map((e) => e.from);

  return (
    <div
      className="fixed inset-0 z-40 grid place-items-center"
      style={{ background: "rgba(0,0,0,0.5)" }}
      onClick={onClose}
    >
      <div
        className="w-[760px] max-w-[92vw] max-h-[80vh] flex flex-col rounded-lg overflow-hidden"
        style={{
          background: "var(--raised)",
          border: "1px solid var(--border-strong)",
          boxShadow: "var(--shadow)",
          backdropFilter: "blur(24px) saturate(140%)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="flex items-center gap-2 px-3 h-10 shrink-0"
          style={{ borderBottom: "1px solid var(--border)" }}
        >
          <span className="text-[12px]" style={{ color: "var(--text)" }}>
            Symbols in
          </span>
          <span className="mono" style={{ color: "var(--accent)" }}>
            {label}
          </span>
          {graph && (
            <span className="text-[11px]" style={{ color: "var(--text-3)" }}>
              {graph.nodes.length} definitions · {graph.edges.length} references
            </span>
          )}
          <button
            type="button"
            onClick={onClose}
            className="ml-auto"
            style={{ color: "var(--text-3)" }}
            aria-label="Close"
          >
            <X size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto">
          {error && <Empty title="No symbol graph" hint={error} />}
          {!error && !graph && (
            <div className="p-4">
              <Spinner label="reading definitions" />
            </div>
          )}
          {graph?.nodes.length === 0 && (
            <Empty
              title="No definitions here"
              hint="This module has no top-level functions, types or constants."
            />
          )}

          {graph?.nodes.map((n) => {
            const out = callsFrom(n.id);
            const inbound = callersOf(n.id);
            return (
              <div
                key={n.id}
                className="px-3 py-1.5"
                style={{ borderBottom: "1px solid var(--border)" }}
              >
                <button
                  type="button"
                  onClick={() => {
                    void select(n.file);
                    onClose();
                  }}
                  className="flex items-baseline gap-2 w-full text-left"
                >
                  <span className="mono" style={{ color: "var(--text)" }}>
                    {n.name}
                  </span>
                  <span className="text-[11px]" style={{ color: "var(--text-3)" }}>
                    {String(n.kind)}
                  </span>
                  {!n.exported && (
                    <span className="text-[10px]" style={{ color: "var(--text-3)" }}>
                      private
                    </span>
                  )}
                  <span
                    className="mono ml-auto text-[11px] truncate max-w-[45%]"
                    style={{ color: "var(--text-3)" }}
                    title={`${n.path}:${n.line}`}
                  >
                    {n.path}:{n.line}
                  </span>
                </button>
                {(out.length > 0 || inbound.length > 0) && (
                  <div className="mt-0.5 text-[11px]" style={{ color: "var(--text-3)" }}>
                    {out.length > 0 && (
                      <span>
                        calls{" "}
                        <span className="mono">
                          {out.map((i) => graph.nodes[i]?.name).join(", ")}
                        </span>
                      </span>
                    )}
                    {out.length > 0 && inbound.length > 0 && " · "}
                    {inbound.length > 0 && (
                      <span>
                        used by{" "}
                        <span className="mono">
                          {inbound.map((i) => graph.nodes[i]?.name).join(", ")}
                        </span>
                      </span>
                    )}
                  </div>
                )}
              </div>
            );
          })}

          {graph && graph.externalRefs.length > 0 && (
            <div className="px-3 py-2">
              <div className="text-[11px]" style={{ color: "var(--text-3)" }}>
                Also references {graph.externalRefs.length} name
                {graph.externalRefs.length === 1 ? "" : "s"} defined outside this
                module — the picture is partial by design.
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
