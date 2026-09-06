import { ArrowRight, ExternalLink, Crosshair, Copy, Network } from "lucide-react";

import { Button, Disclosure, Empty, Spinner, Stat, ago, compact } from "./ui";
import { severityCss, severityHeat } from "../lib/severity";
import { copyText } from "../lib/clipboard";
import { api } from "../lib/tauri";
import { useSelectionStore } from "../store/useSelectionStore";
import { toast } from "../store/useToastStore";

export default function Inspector({
  onShowSymbols,
}: {
  onShowSymbols?: (module: string) => void;
}) {
  const { detail, loadingDetail, selected, runImpact, impact, select, edge } =
    useSelectionStore();


  if (edge) return <EdgeInspector />;

  if (selected === null) {
    return (
      <div className="pt-10">
        <Empty
          title="Nothing selected"
          hint="Click a node in any view to see what it imports, what depends on it, and what ZAtlas thinks is wrong with it. Click a line to see the import that made it."
        />
      </div>
    );
  }
  if (loadingDetail || !detail) {
    return (
      <div className="p-3">
        <Spinner label="loading" />
      </div>
    );
  }

  const failures = detail.unresolved.filter((u) => u.isFailure);
  const nonFailures = detail.unresolved.filter((u) => !u.isFailure);

  return (
    <div className="h-full overflow-y-auto">
      <div className="px-3 pt-3 pb-2" style={{ borderBottom: "1px solid var(--border)" }}>
        <div className="flex items-start gap-1.5">
          <div className="mono break-all flex-1" style={{ color: "var(--text)" }}>
            {detail.path}
          </div>


          <button
            type="button"
            onClick={() => {
              void copyText(detail.path).then((ok) =>
                ok
                  ? toast.success("Path copied")
                  : toast.error("Could not copy", "This webview refused clipboard access."),
              );
            }}
            title="Copy this path"
            aria-label="Copy this path"
            className="shrink-0 grid place-items-center w-6 h-6 rounded-[3px] mt-[-2px]"
            style={{ color: "var(--text-3)" }}
          >
            <Copy size={12} />
          </button>
        </div>
        <div className="mt-1 text-[11px]" style={{ color: "var(--text-3)" }}>
          {compact(detail.loc)} LOC · {detail.language} · {detail.module}
        </div>
      </div>

      <div className="py-1.5" style={{ borderBottom: "1px solid var(--border)" }}>
        <Stat label="imports" value={detail.fanOut} />
        <Stat
          label="imported by"
          value={detail.fanIn}
          warn={detail.fanIn > 20}
          title={detail.fanIn > 20 ? "A wide blast radius: changes here reach a lot of the codebase" : undefined}
        />
        <Stat label="definitions" value={detail.definitions} />
        <Stat label="exports" value={detail.exports.length} />
        {detail.churn > 0 && (
          <>
            <Stat label="churn" value={`${detail.churn} commits`} />
            <Stat
              label="authors"
              value={
                detail.authors === 1
                  ? `1 (${detail.topAuthor})`
                  : `${detail.authors} (${Math.round(detail.topAuthorShare * 100)}% ${detail.topAuthor})`
              }
              warn={detail.authors === 1 && detail.fanIn >= 5}
            />
            <Stat label="last touched" value={ago(Number(detail.lastTouched))} />
          </>
        )}
      </div>

      {detail.findings.length > 0 && (
        <div className="py-2" style={{ borderBottom: "1px solid var(--border)" }}>
          {detail.findings.map((f, i) => (
            <div key={i} className="px-3 py-1.5">
              <div className="flex items-start gap-2">
                <span
                  className="mt-[3px] w-1.5 h-1.5 rounded-full shrink-0"
                  style={{ background: severityCss(severityHeat(f.severity)) }}
                />
                <div className="min-w-0">
                  <div className="text-[12px]" style={{ color: "var(--text)" }}>
                    {f.headline}
                  </div>
                  <div
                    className="mt-1 text-[11px] leading-relaxed"
                    style={{ color: "var(--text-3)" }}
                  >
                    {f.howToFix}
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {failures.length > 0 && (
        <div className="py-2" style={{ borderBottom: "1px solid var(--border)" }}>
          <div className="px-3 pb-1 text-[11px]" style={{ color: "var(--warn)" }}>
            {failures.length} unresolved import{failures.length === 1 ? "" : "s"}
          </div>
          {failures.map((u, i) => (
            <div key={i} className="px-3 py-0.5 flex items-baseline gap-2 text-[11px]">
              <span className="mono truncate" style={{ color: "var(--text-2)" }}>
                {u.specifier}
              </span>
              <span className="ml-auto shrink-0" style={{ color: "var(--text-3)" }}>
                {u.reason}
              </span>
            </div>
          ))}
        </div>
      )}

      {nonFailures.length > 0 && (
        <Disclosure title={`${nonFailures.length} external and asset imports`}>
          {nonFailures.map((u, i) => (
            <div key={i} className="px-4 py-0.5 flex items-baseline gap-2 text-[11px]">
              <span className="mono truncate" style={{ color: "var(--text-3)" }}>
                {u.specifier}
              </span>
              <span className="ml-auto shrink-0" style={{ color: "var(--text-3)" }}>
                {u.reason}
              </span>
            </div>
          ))}
        </Disclosure>
      )}

      <NodeList
        title={`Dependents (${detail.dependents.length})`}
        rows={detail.dependents}
        onSelect={(id) => void select(id)}
      />
      <NodeList
        title={`Dependencies (${detail.dependencies.length})`}
        rows={detail.dependencies}
        onSelect={(id) => void select(id)}
      />

      <div className="p-3 flex flex-wrap gap-2">
        <Button onClick={() => void runImpact()} title="What would break (i)">
          <span className="flex items-center gap-1.5">
            <Crosshair size={12} />
            Impact
          </span>
        </Button>
        <Button
          onClick={() => {
            void api.openInEditor(detail.path).catch(() => {
              toast.error("Could not open the file", "No default application is set.");
            });
          }}
          title="Open in editor (e)"
        >
          <span className="flex items-center gap-1.5">
            <ExternalLink size={12} />
            Open
          </span>
        </Button>
        {onShowSymbols && (
          <Button
            onClick={() => onShowSymbols(detail.module)}
            title="What calls what inside this module"
          >
            <span className="flex items-center gap-1.5">
              <Network size={12} />
              Symbols
            </span>
          </Button>
        )}
      </div>

      {impact && (
        <div className="px-3 pb-4 text-[11px]" style={{ color: "var(--text-2)" }}>
          Changing this reaches <b>{impact.total}</b> file
          {impact.total === 1 ? "" : "s"} — {Math.round(impact.share * 100)}% of the
          repository.
        </div>
      )}
    </div>
  );
}

function NodeList({
  title,
  rows,
  onSelect,
}: {
  title: string;
  rows: { id: number; path: string }[];
  onSelect: (id: number) => void;
}) {
  if (rows.length === 0) return null;
  return (
    <Disclosure title={title}>
      <div className="max-h-[220px] overflow-y-auto">
        {rows.map((r) => (
          <button
            key={r.id}
            type="button"
            onClick={() => onSelect(r.id)}
            className="mono block w-full text-left px-4 py-0.5 truncate hover:bg-[var(--row-hover)]"
            style={{ color: "var(--text-2)" }}
            title={r.path}
          >
            {r.path}
          </button>
        ))}
      </div>
    </Disclosure>
  );
}


function EdgeInspector() {
  const edge = useSelectionStore((s) => s.edge);
  const clear = useSelectionStore((s) => s.clearHighlight);
  if (!edge) return null;

  return (
    <div className="h-full overflow-y-auto">
      <div className="px-3 pt-3 pb-2" style={{ borderBottom: "1px solid var(--border)" }}>
        <div
          className="text-[10px] font-semibold tracking-[0.12em] uppercase mb-1.5"
          style={{ color: "var(--text-3)" }}
        >
          Dependency
        </div>
        <div className="mono break-all text-[12px]" style={{ color: "var(--text)" }}>
          {edge.fromPath}
        </div>
        <div className="flex items-center gap-1.5 my-0.5" style={{ color: "var(--accent)" }}>
          <ArrowRight size={12} />
          <span className="text-[11px]">
            {edge.kind === "viaBarrel"
              ? "through a barrel"
              : edge.kind === "contains"
                ? "declares"
                : "imports"}
          </span>
        </div>
        <div className="mono break-all text-[12px]" style={{ color: "var(--text)" }}>
          {edge.toPath}
        </div>
      </div>

      {edge.kind === "viaBarrel" && (
        <p className="px-3 py-2 text-[11px] leading-relaxed" style={{ color: "var(--text-3)" }}>
          The import goes through a re-export and the defining file could not be
          pinned down, so this edge points at the barrel. Weaker evidence than a
          direct import, and drawn that way.
        </p>
      )}

      <div className="px-3 py-2">
        <div
          className="text-[10px] font-semibold tracking-[0.12em] uppercase mb-1.5"
          style={{ color: "var(--text-3)" }}
        >
          {edge.sites.length === 1 ? "Written here" : `Written in ${edge.sites.length} places`}
        </div>
        {edge.sites.length === 0 ? (
          <p className="text-[11px]" style={{ color: "var(--text-3)" }}>
            No import statement was recorded for this edge. That is a bug worth
            reporting — every edge should be traceable to a line.
          </p>
        ) : (
          edge.sites.map((site, i) => (
            <button
              key={i}
              type="button"
              onClick={() => {


                void api
                  .openInEditor(edge.fromPath)
                  .catch(() => toast.error("Could not open your editor"));
              }}
              title={`Open ${edge.fromPath} — the import is on line ${site.line}`}
              className="flex items-baseline gap-2 w-full py-1 text-left rounded-[3px] hover:bg-[var(--row-hover)]"
            >
              <span className="mono text-[11px] shrink-0" style={{ color: "var(--text-3)" }}>
                :{site.line}
              </span>
              <span className="mono text-[11px] truncate" style={{ color: "var(--text)" }}>
                {site.specifier}
              </span>
              <span
                className="ml-auto text-[10px] shrink-0"
                style={{ color: "var(--text-3)" }}
              >
                {site.typeOnly ? "type-only" : site.kind}
              </span>
            </button>
          ))
        )}
      </div>

      <div className="px-3 pb-3">
        <Button onClick={() => clear()}>Clear</Button>
      </div>
    </div>
  );
}
