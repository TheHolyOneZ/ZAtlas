import { useEffect, useState } from "react";
import { ArrowRight, GitCompare, X } from "lucide-react";

import { Button, CornerTicks, Empty, Spinner } from "./ui";
import { severityCss } from "../lib/severity";
import { api, describeError, type RefComparison } from "../lib/tauri";
import { useRepoStore } from "../store/useRepoStore";


export default function CompareDialog({ onClose }: { onClose: () => void }) {
  const branch = useRepoStore((s) => s.summary?.branch ?? "HEAD");
  const [refs, setRefs] = useState<string[]>([]);
  const [base, setBase] = useState("");
  const [head, setHead] = useState("HEAD");
  const [result, setResult] = useState<RefComparison | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api
      .listRefs()
      .then((list) => {
        setRefs(list);


        setBase(list.find((r) => r !== branch) ?? list[0] ?? "");
      })
      .catch(() => {});
  }, [branch]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  function run() {
    if (!base) return;
    setRunning(true);
    setError(null);
    setResult(null);
    void api
      .compareRefs(base, head)
      .then(setResult)
      .catch((e) => setError(describeError(e)))
      .finally(() => setRunning(false));
  }

  return (
    <div
      className="fixed inset-0 z-40 grid place-items-center"
      style={{ background: "rgba(0,0,0,0.5)" }}
      onClick={onClose}
    >
      <div
        className="w-[760px] max-w-[92vw] max-h-[82vh] flex flex-col rounded-lg overflow-hidden"
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
          <GitCompare size={13} style={{ color: "var(--text-3)" }} />
          <span
            className="text-[10px] font-semibold tracking-[0.12em] uppercase"
            style={{ color: "var(--text-3)" }}
          >
            Compare
          </span>

          <div className="flex items-center gap-2 ml-3">
            <RefPicker value={base} onChange={setBase} options={refs} label="base" />
            <ArrowRight size={12} style={{ color: "var(--text-3)" }} />
            <RefPicker
              value={head}
              onChange={setHead}
              options={["HEAD", ...refs]}
              label="head"
            />
            <Button variant="accent" onClick={run} disabled={!base || running}>
              Compare
            </Button>
          </div>

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

        <div className="flex-1 overflow-auto" style={{ background: "var(--surface-2)" }}>
          {running ? (
            <div className="p-6">
              <Spinner label="analysing both trees" />
              <p className="text-[11px] mt-2" style={{ color: "var(--text-3)" }}>
                Each ref is analysed in full, against its own tsconfig and
                Cargo.toml. Your repository is not touched.
              </p>
            </div>
          ) : error ? (
            <div className="p-6">
              <Empty title="Could not compare" hint={error} />
            </div>
          ) : result ? (
            <Result result={result} />
          ) : (
            <div className="p-6">
              <Empty
                title="Pick two refs"
                hint="See what a branch did to the shape of the codebase — which cycles it introduced, what grew, what it added."
              />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function Result({ result }: { result: RefComparison }) {
  const { base, head } = result;
  return (
    <div className="p-4 space-y-4">


      {result.cyclesIntroduced.length > 0 && (
        <div
          className="rounded-md overflow-hidden"
          style={{ border: `1px solid ${severityCss(0.9)}` }}
        >
          <div
            className="px-3 py-1.5 text-[12px]"
            style={{ background: "var(--surface)", color: severityCss(0.9) }}
          >
            {result.cyclesIntroduced.length === 1
              ? "This branch introduces a cycle"
              : `This branch introduces ${result.cyclesIntroduced.length} cycles`}
          </div>
          {result.cyclesIntroduced.map((cycle, i) => (
            <div key={i} className="mono px-3 py-1.5 text-[11px]" style={{ color: "var(--text)" }}>
              {cycle.join(" → ")}
            </div>
          ))}
        </div>
      )}

      {result.cyclesResolved.length > 0 && (
        <div className="text-[12px]" style={{ color: "var(--accent)" }}>
          {result.cyclesResolved.length === 1
            ? "It also resolves one:"
            : `It also resolves ${result.cyclesResolved.length}:`}
          {result.cyclesResolved.map((cycle, i) => (
            <div key={i} className="mono text-[11px] mt-0.5" style={{ color: "var(--text-3)" }}>
              {cycle.join(" → ")}
            </div>
          ))}
        </div>
      )}

      {result.skipped.length > 0 && (
        <div
          className="rounded-md px-3 py-2"
          style={{ border: `1px solid ${severityCss(0.7)}`, color: severityCss(0.7) }}
        >
          <div className="text-[12px]">
            {result.skipped.length} file(s) could not be written on this platform
            and were left out of the comparison.
          </div>
          <div className="text-[11px] mt-1" style={{ color: "var(--text-3)" }}>
            Windows cannot create every name a repository may contain — a device
            name like <span className="mono">aux.ts</span>, or two files
            differing only in case. The numbers below are missing these.
          </div>
          {result.skipped.slice(0, 8).map((p) => (
            <div key={p} className="mono text-[11px] mt-0.5" style={{ color: "var(--text-2)" }}>
              {p}
            </div>
          ))}
        </div>
      )}

      <div className="grid grid-cols-5 gap-2">
        <Delta label="files" before={base.files} after={head.files} />
        <Delta label="lines" before={Number(base.loc)} after={Number(head.loc)} />
        <Delta label="deps" before={base.edges} after={head.edges} />
        <Delta label="cycles" before={base.cycles} after={head.cycles} worseUp />
        <Delta label="unresolved" before={base.unresolved} after={head.unresolved} worseUp />
      </div>

      {result.findingsNew.length > 0 && (
        <List label="New findings" items={result.findingsNew} tone="warn" />
      )}
      {result.findingsFixed.length > 0 && (
        <List label="No longer reported" items={result.findingsFixed} tone="good" />
      )}
      <List label="Added" items={result.added} />
      <List label="Removed" items={result.removed} />
      <List
        label="Grew"
        items={result.grown.slice(0, 12).map(([p, d]) => `${p}  +${d}`)}
      />
      <List
        label="Shrank"
        items={result.shrunk.slice(0, 12).map(([p, d]) => `${p}  ${d}`)}
      />
    </div>
  );
}


function Delta({
  label,
  before,
  after,
  worseUp = false,
}: {
  label: string;
  before: number;
  after: number;
  worseUp?: boolean;
}) {
  const change = after - before;
  const colour =
    change === 0
      ? "var(--text-3)"
      : worseUp
        ? change > 0
          ? severityCss(0.9)
          : "var(--accent)"
        : "var(--text-2)";
  return (
    <div
      className="relative px-2.5 py-2 rounded-md"
      style={{ background: "var(--surface)", border: "1px solid var(--border)" }}
    >
      <div
        className="text-[10px] uppercase tracking-[0.1em]"
        style={{ color: "var(--text-3)" }}
      >
        {label}
      </div>
      <div className="mono text-[15px] mt-0.5" style={{ color: "var(--text)" }}>
        {after}
      </div>
      <div className="mono text-[11px]" style={{ color: colour }}>
        {change === 0 ? "unchanged" : change > 0 ? `+${change}` : String(change)}
      </div>
      {worseUp && change > 0 && <CornerTicks inset={2} />}
    </div>
  );
}

function List({
  label,
  items,
  tone,
}: {
  label: string;
  items: string[];
  tone?: "warn" | "good";
}) {
  if (items.length === 0) return null;
  const colour =
    tone === "warn" ? severityCss(0.7) : tone === "good" ? "var(--accent)" : "var(--text-2)";
  return (
    <div>
      <div className="flex items-center gap-2 mb-1">
        <span
          className="text-[10px] font-semibold tracking-[0.12em] uppercase"
          style={{ color: "var(--text-3)" }}
        >
          {label}
        </span>
        <span className="mono text-[11px]" style={{ color: "var(--text-3)" }}>
          {items.length}
        </span>
        <span aria-hidden className="flex-1 h-px" style={{ background: "var(--border)" }} />
      </div>
      {items.slice(0, 30).map((item) => (
        <div key={item} className="mono text-[11px] leading-relaxed" style={{ color: colour }}>
          {item}
        </div>
      ))}
      {items.length > 30 && (
        <div className="text-[11px] mt-0.5" style={{ color: "var(--text-3)" }}>
          … and {items.length - 30} more
        </div>
      )}
    </div>
  );
}


function RefPicker({
  value,
  onChange,
  options,
  label,
}: {
  value: string;
  onChange: (value: string) => void;
  options: string[];
  label: string;
}) {
  return (
    <div className="flex items-center gap-1" role="radiogroup" aria-label={label}>
      {options.slice(0, 5).map((option) => {
        const on = option === value;
        return (
          <button
            key={option}
            type="button"
            role="radio"
            aria-checked={on}
            onClick={() => onChange(option)}
            className="relative mono px-2 h-6 rounded-[3px] text-[11px] whitespace-nowrap transition-colors duration-100"
            style={{
              background: on ? "var(--accent-soft)" : "transparent",
              color: on ? "var(--accent)" : "var(--text-3)",
              border: `1px solid ${on ? "var(--accent)" : "var(--border)"}`,
            }}
          >
            {option}
            {on && <CornerTicks inset={2} />}
          </button>
        );
      })}
    </div>
  );
}
