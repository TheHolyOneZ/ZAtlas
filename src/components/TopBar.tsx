import { useEffect, useRef, useState } from "react";
import { ChevronDown, Clock, Eye, EyeOff, FolderOpen, FolderPlus, RotateCw, X } from "lucide-react";

import { Button, CornerTicks, Divider, Spinner, ago, compact, ellipsisMiddle } from "./ui";
import { useRecentStore } from "../store/useRecentStore";
import { useRepoStore } from "../store/useRepoStore";


export default function TopBar() {
  const {
    open,
    state,
    summary,
    progress,
    pick,
    addRepo,
    rescan,
    cancel,
    watching,
    stale,
    setWatching,
    roots,
  } = useRepoStore();

  return (
    <div
      className="flex items-center gap-3 h-9 px-3 shrink-0"
      style={{ background: "var(--surface)", borderBottom: "1px solid var(--border)" }}
    >
      <OpenControl onPick={() => void pick()} onOpen={(p) => void open(p)} />
      {summary && (
        <Button
          variant="ghost"
          square
          onClick={() => void addRepo()}
          title="Add another repository to compare side by side"
        >
          <FolderPlus size={13} />
        </Button>
      )}
      {summary && <Divider />}

      {summary && (
        <>
          <span className="mono truncate max-w-[280px]" style={{ color: "var(--text)" }}>
            {roots.length > 1
              ? `${roots.length} repositories`
              : ellipsisMiddle(summary.root, 46)}
          </span>
          {summary.isGit && (
            <span
              className="mono text-[11px] px-1.5 h-5 flex items-center rounded-[3px] shrink-0"
              style={{ background: "var(--control)", color: "var(--text-2)" }}
              title="Branch and commit"
            >
              {summary.branch ?? "detached"}
              {summary.head ? ` @ ${summary.head}` : ""}
            </span>
          )}
          <Divider />
          <span className="mono text-[11px] shrink-0" style={{ color: "var(--text-3)" }}>
            {compact(summary.fileCount)} files
            <span style={{ opacity: 0.4 }}> · </span>
            {compact(summary.edgeCount)} edges
            <span style={{ opacity: 0.4 }}> · </span>
            {compact(Number(summary.totalLoc))} LOC
          </span>
          {summary.lsp.servers.length > 0 && (
            <span
              className="mono text-[11px] px-1.5 h-5 flex items-center rounded-[3px] shrink-0"
              title={`${summary.lsp.servers.join(", ")} answered ${summary.lsp.queries} queries: ${summary.lsp.repaired} import(s) repaired, ${summary.lsp.disagreements} disagreement(s).${summary.lsp.truncated ? " The time budget ran out before every query was sent." : ""}`}
              style={{
                color: summary.lsp.disagreements > 0 ? "var(--warn)" : "var(--accent)",
                border: `1px solid ${summary.lsp.disagreements > 0 ? "var(--warn)" : "var(--accent)"}`,
              }}
            >


              {summary.lsp.disagreements > 0
                ? `${summary.lsp.disagreements} disagree`
                : summary.lsp.repaired > 0
                  ? `${summary.lsp.repaired} repaired`
                  : "lsp agrees"}
            </span>
          )}
          {summary.unresolvedCount > 0 && (
            <span
              className="mono text-[11px] px-1.5 h-5 flex items-center rounded-[3px] shrink-0"
              title="Imports ZAtlas could not resolve. Open a file to see why — they are never hidden."
              style={{
                color: "var(--warn)",
                border: "1px solid var(--warn)",
                background: "transparent",
              }}
            >
              {summary.unresolvedCount} unresolved
            </span>
          )}
        </>
      )}

      <div className="ml-auto flex items-center gap-2">
        {state === "scanning" && progress && (
          <>
            <Spinner
              label={
                progress.total > 0
                  ? `${progress.phase} ${compact(progress.done)}/${compact(progress.total)}`
                  : progress.phase
              }
            />
            <Button variant="ghost" onClick={() => void cancel()} title="Cancel scan">
              <X size={13} />
            </Button>
          </>
        )}
        {state === "scanning" && !progress && <Spinner label="starting" />}
        {summary && state !== "scanning" && (
          <>
            <Button
              variant="ghost"
              onClick={() => void setWatching(!watching)}
              title={
                watching
                  ? "Watching for changes on disk — click to stop"
                  : "Watch for changes on disk"
              }
            >
              {watching ? <Eye size={13} /> : <EyeOff size={13} />}
            </Button>
            <Button
              variant={stale ? "accent" : "default"}
              onClick={() => void rescan()}
              title={stale ? "The repository changed on disk" : "Re-scan (F5)"}
            >
              <span className="flex items-center gap-1.5">
                <RotateCw size={12} />
                {stale ? "Changed — re-scan" : "Re-scan"}
              </span>
            </Button>
          </>
        )}
      </div>
    </div>
  );
}


function OpenControl({
  onPick,
  onOpen,
}: {
  onPick: () => void;
  onOpen: (path: string) => void;
}) {
  const recents = useRecentStore((s) => s.recents);
  const forget = useRecentStore((s) => s.forget);
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!wrap.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={wrap} className="relative flex items-center">
      <Button variant="ghost" onClick={onPick} title="Open a repository">
        <span className="flex items-center gap-1.5">
          <FolderOpen size={13} />
          Open
        </span>
      </Button>
      {recents.length > 0 && (
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          aria-label="Recent repositories"
          aria-expanded={open}
          title="Recent repositories"
          className="grid place-items-center w-5 h-6 rounded-[3px]"
          style={{ color: open ? "var(--accent)" : "var(--text-3)" }}
        >
          <ChevronDown size={12} />
        </button>
      )}

      {open && (
        <div
          className="absolute left-0 top-8 z-50 w-[340px] rounded-md overflow-hidden"
          style={{
            background: "var(--raised)",
            border: "1px solid var(--border-strong)",
            boxShadow: "var(--shadow)",
          }}
        >
          <div
            className="px-3 py-1.5 text-[10px] font-semibold tracking-[0.12em] uppercase"
            style={{ color: "var(--text-3)", borderBottom: "1px solid var(--border)" }}
          >
            Recent
          </div>
          {recents.map((r) => (
            <div key={r.path} className="group/recent relative flex items-center">
              <button
                type="button"
                onClick={() => {
                  setOpen(false);
                  onOpen(r.path);
                }}
                className="relative flex items-baseline gap-2 flex-1 min-w-0 px-3 h-8 text-left hover:bg-[var(--row-hover)]"
              >
                <Clock size={10} className="shrink-0 self-center" style={{ color: "var(--text-3)" }} />
                <span className="text-[12px] truncate" style={{ color: "var(--text)" }}>
                  {r.name}
                </span>
                <span
                  className="mono text-[10px] ml-auto shrink-0 pr-5"
                  style={{ color: "var(--text-3)" }}
                  title={r.path}
                >
                  {ago(r.openedAt)}
                </span>
              </button>
              <button
                type="button"
                onClick={() => forget(r.path)}
                aria-label={`Forget ${r.name}`}
                title="Remove from this list"
                className="absolute right-1.5 grid place-items-center w-5 h-5 rounded-[3px] opacity-0 group-hover/recent:opacity-100 focus:opacity-100"
                style={{ color: "var(--text-3)", background: "var(--raised)" }}
              >
                <X size={11} />
              </button>
            </div>
          ))}
          <CornerTicks inset={3} />
        </div>
      )}
    </div>
  );
}
