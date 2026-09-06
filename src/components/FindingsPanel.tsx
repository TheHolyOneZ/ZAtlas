import { useEffect, useRef, useState } from "react";
import { Check, CheckCheck, ChevronRight, ClipboardCopy, EyeOff, Undo2 } from "lucide-react";

import { CornerTicks, Empty, Spinner } from "./ui";
import { copyText } from "../lib/clipboard";
import { findingToMarkdown } from "../lib/findingMarkdown";
import { severityCss, severityHeat } from "../lib/severity";
import { api, type Finding, type FindingKind } from "../lib/tauri";
import { useGraphStore } from "../store/useGraphStore";
import { useFindingCursor } from "../store/useFindingCursor";
import { useRepoStore } from "../store/useRepoStore";
import { useSelectionStore } from "../store/useSelectionStore";
import { toast } from "../store/useToastStore";

const KIND_LABELS: Record<string, string> = {
  cycle: "Cycles",
  godFile: "God files",
  orphan: "Orphans",
  unstableInterface: "Unstable",
  layeringViolation: "Layering",
  busFactor: "Bus factor",
  distantCoupling: "Coupling",
  barrelHub: "Barrels",
  caseMismatch: "Case",
  lspDisagreement: "Resolver",
  pathAlias: "Aliases",
};


export default function FindingsPanel() {
  const version = useRepoStore((s) => s.version);
  const state = useRepoStore((s) => s.state);
  const { select, highlight } = useSelectionStore();
  const nodes = useGraphStore((s) => s.graph?.nodes);

  const [rows, setRows] = useState<Finding[]>([]);
  const [indices, setIndices] = useState<number[]>([]);
  const [rowAccepted, setRowAccepted] = useState<boolean[]>([]);
  const [total, setTotal] = useState(0);
  const [counts, setCounts] = useState<[FindingKind, number][]>([]);
  const [active, setActive] = useState<FindingKind | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [acceptedCount, setAcceptedCount] = useState(0);
  const [showAccepted, setShowAccepted] = useState(false);

  const [refresh, setRefresh] = useState(0);


  const cursor = useFindingCursor((s) => s.cursor);
  const setCount = useFindingCursor((s) => s.setCount);
  const resetCursor = useFindingCursor((s) => s.reset);
  const listRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (state !== "ready") {
      setRows([]);
      setCount(0);
      setIndices([]);
      setRowAccepted([]);
      setTotal(0);
      setCounts([]);
      setAcceptedCount(0);
      return;
    }
    let cancelled = false;
    setLoading(true);
    void (async () => {
      try {
        const [page, c] = await Promise.all([
          api.getFindings(0, 200, active ? [active] : undefined, undefined, showAccepted),
          api.findingCounts(),
        ]);
        if (cancelled) return;
        setRows(page.rows);
        setCount(page.rows.length);
        setIndices(page.indices);
        setRowAccepted(page.isAccepted);
        setTotal(page.total);
        setAcceptedCount(page.accepted);
        setCounts(c);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [version, state, active, showAccepted, refresh]);

  useEffect(() => {
    const finding = rows[cursor];
    if (!finding) return;
    setExpanded(cursor);
    highlight(finding.files);
    const first = finding.files[0];
    if (first !== undefined) void select(first);


    listRef.current
      ?.querySelector(`[data-row="${cursor}"]`)
      ?.scrollIntoView({ block: "nearest" });


  }, [cursor, rows]);

  useEffect(() => resetCursor, [resetCursor, version]);


  async function toggleAccept(row: number) {
    const index = indices[row];
    if (index === undefined) return;
    const wasAccepted = rowAccepted[row] === true;
    try {
      if (wasAccepted) {
        await api.unacceptFinding(index);
        toast.info("Back in the list", rows[row]?.headline);
      } else {
        await api.acceptFinding(index);
        toast.success("Accepted", rows[row]?.headline);
      }
      setRefresh((n) => n + 1);
    } catch (e) {
      toast.error("Could not update the baseline", String(e));
    }
  }

  async function accept() {
    const accepting = total;
    try {
      await api.acceptBaseline();


      setShowAccepted(true);
      setRefresh((n) => n + 1);
      toast.success(
        `Accepted ${accepting} finding(s)`,
        "Commit zatlas-baseline.json. Later scans report only what appeared since.",
      );
    } catch (e) {
      toast.error("Could not write the baseline", String(e));
    }
  }

  if (state !== "ready") {
    return null;
  }

  return (
    <div className="flex flex-col h-full min-h-0" style={{ background: "var(--surface-2)" }}>
      <div
        className="flex items-center gap-1.5 px-3 h-8 shrink-0 overflow-x-auto"
        style={{ borderBottom: "1px solid var(--border)" }}
      >
        <span
          className="text-[10px] font-semibold tracking-[0.16em] uppercase shrink-0"
          style={{ color: "var(--text-3)" }}
        >
          Findings
        </span>
        <span className="mono text-[11px] shrink-0" style={{ color: "var(--text-2)" }}>
          {total}
        </span>
        <div className="flex items-center gap-1 ml-2">
          <Chip label="All" on={active === null} onClick={() => setActive(null)} />
          {counts.map(([kind, n]) => (
            <Chip
              key={String(kind)}
              label={KIND_LABELS[String(kind)] ?? String(kind)}
              count={n}
              on={active === kind}
              onClick={() => setActive(active === kind ? null : kind)}
            />
          ))}
        </div>
        <div className="ml-auto flex items-center gap-1.5 shrink-0">
          {loading && <Spinner />}


          {acceptedCount > 0 && (
            <Chip
              label={showAccepted ? "hiding nothing" : "accepted"}
              count={acceptedCount}
              on={showAccepted}
              onClick={() => setShowAccepted(!showAccepted)}
              icon={<EyeOff size={11} />}
              title={
                showAccepted
                  ? "Accepted findings are shown. Click to hide them again."
                  : `${acceptedCount} finding(s) accepted by zatlas-baseline.json. Click to show them.`
              }
            />
          )}
          {total > 0 && (
            <Chip
              label="Accept all"
              on={false}
              onClick={() => void accept()}
              icon={<CheckCheck size={11} />}
              title="Write zatlas-baseline.json accepting everything listed now, so later scans show only what is new. Commit the file."
            />
          )}
        </div>
      </div>

      <div ref={listRef} className="flex-1 min-h-0 overflow-y-auto">
        {rows.length === 0 && !loading ? (
          <Empty
            title={acceptedCount > 0 ? "Nothing new" : "Nothing to report"}
            hint={
              acceptedCount > 0
                ? `Every finding is in zatlas-baseline.json. Show the ${acceptedCount} accepted one(s) to review them.`
                : "No cycles, no god files, no orphans, no layering violations. That is a good result, not an empty screen."
            }
          />
        ) : (
          rows.map((f, i) => (
            <Row
              key={i}
              row={i}
              cursored={cursor === i}
              finding={f}
              paths={(f.files ?? []).map((id) => nodes?.[id]?.path ?? String(id))}
              accepted={rowAccepted[i] === true}
              onAccept={() => void toggleAccept(i)}
              open={expanded === i}
              onToggle={() => setExpanded(expanded === i ? null : i)}
              onFocus={() => {
                highlight(f.files);
                const first = f.files[0];
                if (first !== undefined) void select(first);
              }}
            />
          ))
        )}
      </div>
    </div>
  );
}


function Chip({
  label,
  count,
  on,
  onClick,
  icon,
  title,
}: {
  label: string;
  count?: number;
  on: boolean;
  onClick: () => void;
  icon?: React.ReactNode;
  title?: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      className="relative flex items-center gap-1.5 px-2 h-[22px] rounded-[3px] text-[11px] whitespace-nowrap transition-colors duration-100"
      style={{
        background: on ? "var(--accent-soft)" : "transparent",
        color: on ? "var(--accent)" : "var(--text-3)",
        border: `1px solid ${on ? "var(--accent)" : "var(--border)"}`,
      }}
    >
      {icon}
      {label}
      {count !== undefined && (
        <span className="mono text-[10px]" style={{ opacity: 0.75 }}>
          {count}
        </span>
      )}
      {on && <CornerTicks inset={2} />}
    </button>
  );
}

function Row({
  finding,
  paths,
  row,
  cursored,
  accepted,
  onAccept,
  open,
  onToggle,
  onFocus,
}: {
  finding: Finding;
  paths: string[];
  row: number;
  cursored: boolean;
  accepted: boolean;
  onAccept: () => void;
  open: boolean;
  onToggle: () => void;
  onFocus: () => void;
}) {
  const heat = severityHeat(String(finding.severity));
  return (
    <div
      data-row={row}
      className="relative"
      style={{
        borderBottom: "1px solid var(--border)",


        background: cursored ? "var(--row-hover)" : undefined,
      }}
    >


      <span
        aria-hidden
        className="absolute left-0 top-0 bottom-0 w-[3px]"
        style={{ background: severityCss(heat), opacity: accepted ? 0.35 : 1 }}
        title={String(finding.severity)}
      />
      <div className="group/row flex items-center gap-2 pl-4 pr-3 py-1.5 hover:bg-[var(--row-hover)]">
        <button
          type="button"
          onClick={onToggle}
          className="shrink-0 transition-transform duration-150"
          style={{
            color: "var(--text-3)",
            transform: open ? "rotate(90deg)" : "none",
          }}
          aria-label={open ? "Collapse" : "Expand"}
        >
          <ChevronRight size={13} />
        </button>
        <button
          type="button"
          onClick={onFocus}
          className="mono text-left truncate flex-1"
          style={{ color: accepted ? "var(--text-3)" : "var(--text)" }}
          title={finding.headline}
        >
          {finding.headline}
        </button>


        {heat >= 0.5 && (
          <span
            className="text-[10px] uppercase tracking-[0.1em] shrink-0"
            style={{ color: severityCss(heat) }}
          >
            {String(finding.severity)}
          </span>
        )}


        <button
          type="button"
          onClick={onAccept}
          title={
            accepted
              ? "Put this back in the list"
              : "Accept this one into zatlas-baseline.json"
          }
          aria-label={accepted ? "Un-accept this finding" : "Accept this finding"}
          className={`shrink-0 grid place-items-center w-5 h-5 rounded-[3px] transition-opacity duration-100 ${
            accepted ? "opacity-100" : "opacity-0 group-hover/row:opacity-100 focus:opacity-100"
          }`}
          style={{ color: accepted ? "var(--accent)" : "var(--text-3)" }}
        >
          {accepted ? <Undo2 size={12} /> : <CheckCheck size={12} />}
        </button>
      </div>

      {open && (
        <div className="pl-9 pr-4 pb-3 space-y-2">
          <div className="flex flex-wrap gap-1.5">
            {finding.evidence.map((e, i) => (
              <span
                key={i}
                className="flex items-baseline gap-1.5 px-2 py-0.5 rounded-[3px]"
                style={{ background: "var(--surface)", border: "1px solid var(--border)" }}
              >
                <span className="text-[10px] uppercase tracking-[0.08em]" style={{ color: "var(--text-3)" }}>
                  {e.label}
                </span>
                <span className="mono text-[11px]" style={{ color: "var(--text)" }}>
                  {e.value}
                </span>
              </span>
            ))}
          </div>
          <p className="text-[12px] leading-relaxed" style={{ color: "var(--text-2)" }}>
            {finding.why}
          </p>
          <div className="flex gap-2">
            <span
              aria-hidden
              className="mt-[7px] shrink-0 w-4 h-px"
              style={{ background: "var(--accent)" }}
            />
            <p className="text-[12px] leading-relaxed" style={{ color: "var(--text-2)" }}>
              {finding.howToFix}
            </p>
          </div>
          <CopyAsMarkdown finding={finding} paths={paths} />
        </div>
      )}
    </div>
  );
}


function CopyAsMarkdown({ finding, paths }: { finding: Finding; paths: string[] }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      onClick={() => {
        void copyText(findingToMarkdown(finding, paths)).then((ok) => {
          if (!ok) {
            toast.error("Could not copy", "This webview refused clipboard access.");
            return;
          }
          setCopied(true);
          setTimeout(() => setCopied(false), 1600);
        });
      }}
      title="Copy this finding as Markdown, with its evidence and fix"
      className="flex items-center gap-1.5 px-2 h-6 rounded-[3px] text-[11px] transition-colors duration-100"
      style={{
        border: "1px solid var(--border)",
        color: copied ? "var(--accent)" : "var(--text-3)",
      }}
    >
      {copied ? <Check size={11} /> : <ClipboardCopy size={11} />}
      {copied ? "Copied" : "Copy as Markdown"}
    </button>
  );
}
