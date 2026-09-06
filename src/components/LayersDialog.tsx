import { useEffect, useMemo, useState } from "react";
import { Check, Layers, X } from "lucide-react";

import { Button, CornerTicks, Empty, Spinner, TextInput } from "./ui";
import { severityCss } from "../lib/severity";
import { api, describeError } from "../lib/tauri";
import { useRepoStore } from "../store/useRepoStore";
import { toast } from "../store/useToastStore";


export default function LayersDialog({ onClose }: { onClose: () => void }) {
  const rescan = useRepoStore((s) => s.rescan);
  const [candidates, setCandidates] = useState<[string, number][]>([]);
  const [loading, setLoading] = useState(true);

  const [assigned, setAssigned] = useState<Record<string, string>>({});

  const [allowed, setAllowed] = useState<Set<string>>(new Set());
  const [newLayer, setNewLayer] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let live = true;
    Promise.all([api.layerCandidates(), api.getConfig()])
      .then(([dirs, config]) => {
        if (!live) return;
        setCandidates(dirs);


        const seed: Record<string, string> = {};
        for (const layer of config.layers) {
          for (const glob of layer.paths) {
            seed[glob.replace(/\/\*\*$/, "")] = layer.name;
          }
        }
        setAssigned(seed);
        const rules = new Set<string>();
        for (const raw of [...config.rules, ...config.layers.flatMap((l) => l.rules)]) {
          const [from, to] = raw.split("->").map((s) => s.trim());
          if (from && to) rules.add(`${from}→${to}`);
        }
        setAllowed(rules);
      })
      .catch(() => {})
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const layers = useMemo(() => {
    const names = new Set(Object.values(assigned).filter(Boolean));
    return [...names].sort();
  }, [assigned]);

  function assign(dir: string, layer: string) {
    setAssigned((prev) => {
      const next = { ...prev };
      if (!layer || prev[dir] === layer) delete next[dir];
      else next[dir] = layer;
      return next;
    });
  }

  function toggleRule(from: string, to: string) {
    setAllowed((prev) => {
      const next = new Set(prev);
      const key = `${from}→${to}`;
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  async function save() {
    const grouped: Record<string, string[]> = {};
    for (const [dir, layer] of Object.entries(assigned)) {
      (grouped[layer] ??= []).push(`${dir}/**`);
    }
    setSaving(true);
    try {
      const path = await api.writeLayers(
        Object.entries(grouped).map(([name, paths]) => ({ name, paths: paths.sort() })),
        [...allowed].map((r) => r.replace("→", " -> ")),
      );
      toast.success("Wrote zatlas.toml", `${path} — commit it to share the rules.`);
      onClose();


      void rescan();
    } catch (e) {
      toast.error("Could not write zatlas.toml", describeError(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-40 grid place-items-center"
      style={{ background: "rgba(0,0,0,0.5)" }}
      onClick={onClose}
    >
      <div
        className="w-[820px] max-w-[94vw] max-h-[84vh] flex flex-col rounded-lg overflow-hidden"
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
          <Layers size={13} style={{ color: "var(--text-3)" }} />
          <span
            className="text-[10px] font-semibold tracking-[0.12em] uppercase"
            style={{ color: "var(--text-3)" }}
          >
            Layers
          </span>
          <span className="text-[11px] ml-2" style={{ color: "var(--text-3)" }}>
            Name your layers, then say what may depend on what.
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

        <div className="flex-1 overflow-auto p-4" style={{ background: "var(--surface-2)" }}>
          {loading ? (
            <Spinner label="reading the repository" />
          ) : candidates.length === 0 ? (
            <Empty title="Nothing to group" hint="This repository has no subdirectories to make layers from." />
          ) : (
            <>
              <Section label="Layers">
                <div className="flex flex-wrap items-center gap-1.5">
                  {layers.map((name) => (
                    <span
                      key={name}
                      className="mono px-2 h-6 grid place-items-center rounded-[3px] text-[11px]"
                      style={{
                        background: "var(--accent-soft)",
                        border: "1px solid var(--accent)",
                        color: "var(--accent)",
                      }}
                    >
                      {name}
                    </span>
                  ))}
                  <div className="w-[180px]">
                    <TextInput
                      value={newLayer}
                      onChange={setNewLayer}
                      placeholder="new layer name…"
                      mono={false}
                    />
                  </div>
                </div>
                <p className="text-[11px] mt-1.5" style={{ color: "var(--text-3)" }}>
                  Type a name, then click the directories that belong to it.
                </p>
              </Section>

              <Section label="Directories">
                <div className="grid grid-cols-2 gap-1">
                  {candidates.map(([dir, count]) => {
                    const layer = assigned[dir];
                    const target = newLayer.trim() || layer || "";
                    return (
                      <button
                        key={dir}
                        type="button"
                        onClick={() => target && assign(dir, target)}
                        disabled={!target}
                        title={
                          target
                            ? `Put ${dir} in "${target}"`
                            : "Type a layer name first"
                        }
                        className="relative flex items-baseline gap-2 px-2 h-7 rounded-[3px] text-left disabled:cursor-not-allowed"
                        style={{
                          background: layer ? "var(--accent-soft)" : "var(--surface)",
                          border: `1px solid ${layer ? "var(--accent)" : "var(--border)"}`,
                        }}
                      >
                        <span
                          className="mono text-[11px] truncate"
                          style={{ color: layer ? "var(--accent)" : "var(--text-2)" }}
                        >
                          {dir}
                        </span>
                        <span className="mono text-[10px] ml-auto" style={{ color: "var(--text-3)" }}>
                          {layer ?? count}
                        </span>
                        {layer && <CornerTicks inset={2} />}
                      </button>
                    );
                  })}
                </div>
              </Section>

              {layers.length >= 2 && (
                <Section label="Allowed directions">
                  <RuleGrid layers={layers} allowed={allowed} onToggle={toggleRule} />
                  <p className="text-[11px] mt-2 leading-relaxed" style={{ color: "var(--text-3)" }}>
                    Tick a cell to allow that row's layer to depend on that
                    column's. Anything left unticked between two declared layers
                    becomes a violation — so a layer that may depend on nothing
                    is a perfectly good answer.
                  </p>
                </Section>
              )}
            </>
          )}
        </div>

        <div
          className="flex items-center gap-2 px-3 h-11 shrink-0"
          style={{ borderTop: "1px solid var(--border)" }}
        >


          {layers.length > 0 && allowed.size === 0 ? (
            <span className="text-[11px]" style={{ color: severityCss(0.7) }}>
              {layers.length < 2
                ? "One layer on its own reports nothing — add a second, then say what may depend on what."
                : "No directions ticked yet, so every dependency between these layers becomes a violation."}
            </span>
          ) : (
            <span className="text-[11px]" style={{ color: "var(--text-3)" }}>
              Writes <span className="mono">zatlas.toml</span> at the repository root, then re-scans.
            </span>
          )}
          <div className="ml-auto">
            <Button
              variant="accent"
              onClick={() => void save()}
              disabled={saving || layers.length === 0}
            >
              <span className="flex items-center gap-1.5">
                <Check size={12} />
                {saving ? "Writing…" : "Write zatlas.toml"}
              </span>
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mb-5">
      <div className="flex items-center gap-2 mb-2">
        <span
          className="text-[10px] font-semibold tracking-[0.12em] uppercase"
          style={{ color: "var(--text-3)" }}
        >
          {label}
        </span>
        <span aria-hidden className="flex-1 h-px" style={{ background: "var(--border)" }} />
      </div>
      {children}
    </div>
  );
}


function RuleGrid({
  layers,
  allowed,
  onToggle,
}: {
  layers: string[];
  allowed: Set<string>;
  onToggle: (from: string, to: string) => void;
}) {
  return (
    <div className="inline-block">
      <div className="flex">
        <div className="w-[120px]" />
        {layers.map((to) => (
          <div
            key={to}
            className="mono w-[76px] text-[10px] truncate text-center pb-1"
            style={{ color: "var(--text-3)" }}
            title={to}
          >
            {to}
          </div>
        ))}
      </div>
      {layers.map((from) => (
        <div key={from} className="flex items-center">
          <div
            className="mono w-[120px] text-[11px] truncate pr-2 text-right"
            style={{ color: "var(--text-2)" }}
            title={from}
          >
            {from}
          </div>
          {layers.map((to) => {
            const self = from === to;
            const on = allowed.has(`${from}→${to}`);
            return (
              <div key={to} className="w-[76px] grid place-items-center py-0.5">
                <button
                  type="button"
                  onClick={() => !self && onToggle(from, to)}
                  disabled={self}
                  aria-label={`${from} may depend on ${to}`}
                  aria-pressed={on}
                  title={
                    self
                      ? "A layer always depends on itself"
                      : `${from} → ${to}`
                  }
                  className="grid place-items-center w-5 h-5 rounded-[2px]"
                  style={{


                    background: self
                      ? "var(--control)"
                      : on
                        ? "var(--accent-soft)"
                        : "transparent",
                    border: `1px solid ${
                      self ? "var(--border)" : on ? "var(--accent)" : "var(--border-strong)"
                    }`,
                    color: "var(--accent)",
                    cursor: self ? "default" : "pointer",
                  }}
                >
                  {self ? (
                    <span style={{ color: "var(--text-3)" }}>·</span>
                  ) : on ? (
                    <Check size={11} strokeWidth={3} />
                  ) : null}
                </button>
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}
