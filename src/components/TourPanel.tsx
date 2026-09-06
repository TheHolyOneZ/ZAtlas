import { useEffect, useState } from "react";
import { BookOpen, X } from "lucide-react";

import { Empty, Spinner, compact } from "./ui";
import { api, describeError, type TourStop } from "../lib/tauri";
import { useSelectionStore } from "../store/useSelectionStore";


export default function TourPanel({ onClose }: { onClose: () => void }) {
  const [stops, setStops] = useState<TourStop[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { select } = useSelectionStore();

  useEffect(() => {
    let cancelled = false;
    void api
      .getTour()
      .then((t) => {
        if (!cancelled) setStops(t);
      })
      .catch((e) => {
        if (!cancelled) setError(describeError(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-40 grid place-items-center"
      style={{ background: "rgba(0,0,0,0.5)" }}
      onClick={onClose}
    >
      <div
        className="w-[720px] max-w-[92vw] max-h-[80vh] flex flex-col rounded-lg overflow-hidden"
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
          <BookOpen size={14} style={{ color: "var(--accent)" }} />
          <span className="text-[12px]" style={{ color: "var(--text)" }}>
            Where to start
          </span>
          <span className="text-[11px]" style={{ color: "var(--text-3)" }}>
            read these, in this order
          </span>
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
          {error && <Empty title="No tour" hint={error} />}
          {!error && stops === null && (
            <div className="p-4">
              <Spinner label="working out a reading order" />
            </div>
          )}
          {stops?.length === 0 && (
            <Empty
              title="Nothing to tour"
              hint="This repository is small enough to read end to end."
            />
          )}
          {stops?.map((s, i) => (
            <button
              key={s.file}
              type="button"
              onClick={() => {
                void select(s.file);
                onClose();
              }}
              className="w-full text-left flex gap-3 px-3 py-2.5 hover:bg-[var(--row-hover)]"
              style={{ borderBottom: "1px solid var(--border)" }}
            >
              <span
                className="mono shrink-0 w-5 text-right"
                style={{ color: "var(--accent)" }}
              >
                {i + 1}
              </span>
              <span className="min-w-0">
                <span className="mono block truncate" style={{ color: "var(--text)" }}>
                  {s.path}
                </span>
                <span
                  className="block text-[11px] mt-0.5 leading-relaxed"
                  style={{ color: "var(--text-3)" }}
                >
                  {s.reason} · {compact(s.loc)} lines
                </span>
              </span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
