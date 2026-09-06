import { useEffect, useState } from "react";
import { Check, Copy, Download, X } from "lucide-react";

import { Button, Spinner } from "./ui";
import { copyText } from "../lib/clipboard";
import { api, describeError, type ExportFormat } from "../lib/tauri";
import { canvasToPng, useCaptureStore } from "../store/useCaptureStore";
import { useRepoStore } from "../store/useRepoStore";
import { toast } from "../store/useToastStore";


type Choice = ExportFormat | "svg" | "png";

const FORMATS: { id: Choice; label: string; hint: string }[] = [
  { id: "d2", label: "D2", hint: "Diagram source you can commit and render" },
  { id: "mermaid", label: "Mermaid", hint: "Renders inline on GitHub" },
  { id: "markdown", label: "Report", hint: "Findings, hotspots and what to do" },
  { id: "svg", label: "SVG", hint: "The view as vector — stays sharp at any size" },
  { id: "png", label: "PNG", hint: "The view as a picture, at twice screen resolution" },
];


function isImage(choice: Choice): choice is "svg" | "png" {
  return choice === "svg" || choice === "png";
}

export default function ExportDialog({ onClose }: { onClose: () => void }) {
  const [format, setFormat] = useState<Choice>("d2");
  const capture = useCaptureStore((s) => s.capture);
  const repoName = useRepoStore((s) => s.summary?.name ?? "zatlas");
  const [scope, setScope] = useState<"modules" | "files">("modules");
  const [text, setText] = useState("");
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (isImage(format)) {
      setText("");
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    void api
      .exportText(format, scope)
      .then((t) => {
        if (!cancelled) setText(t);
      })
      .catch((e) => {
        if (!cancelled) setText(`# ${describeError(e)}`);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [format, scope]);

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
          className="flex items-center gap-2 px-3 h-9 shrink-0"
          style={{ borderBottom: "1px solid var(--border)" }}
        >
          <span
            className="text-[10px] font-semibold tracking-[0.12em] uppercase"
            style={{ color: "var(--text-3)" }}
          >
            Export
          </span>
          <div className="flex items-center gap-1 ml-2">
            {FORMATS.filter((f) => {


              if (f.id === "svg") return capture?.toSvg != null;
              if (f.id === "png") return capture?.canvas() != null;
              return true;
            }).map((f) => (
              <button
                key={f.id}
                type="button"
                onClick={() => setFormat(f.id)}
                title={f.hint}
                className="px-2 h-6 rounded text-[12px] transition-colors duration-150"
                style={{
                  background: format === f.id ? "var(--accent)" : "transparent",
                  color: format === f.id ? "var(--accent-text)" : "var(--text-2)",
                }}
              >
                {f.label}
              </button>
            ))}
          </div>

          {format !== "markdown" && !isImage(format) && (
            <div className="flex items-center gap-1 ml-3">
              {(["modules", "files"] as const).map((s) => (
                <button
                  key={s}
                  type="button"
                  onClick={() => setScope(s)}
                  title={
                    s === "modules"
                      ? "One node per module — readable at any repository size"
                      : "One node per file — best for a small subgraph"
                  }
                  className="px-2 h-5 rounded-full text-[11px]"
                  style={{
                    background: scope === s ? "var(--accent-soft)" : "transparent",
                    color: scope === s ? "var(--accent)" : "var(--text-3)",
                    border: `1px solid ${scope === s ? "var(--accent)" : "var(--border)"}`,
                  }}
                >
                  {s}
                </button>
              ))}
            </div>
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

        <div className="flex-1 overflow-auto p-3" style={{ background: "var(--surface-2)" }}>
          {isImage(format) ? (
            <ImageNote format={format} view={capture?.view ?? "view"} />
          ) : loading ? (
            <Spinner label="rendering" />
          ) : (
            <pre
              className="mono whitespace-pre text-[11px] leading-relaxed"
              style={{ color: "var(--text-2)" }}
            >
              {text}
            </pre>
          )}
        </div>

        <div
          className="flex items-center gap-2 px-3 h-11 shrink-0"
          style={{ borderTop: "1px solid var(--border)" }}
        >
          <span className="mono text-[11px]" style={{ color: "var(--text-3)" }}>
            {isImage(format)
              ? `${capture?.view ?? "view"} · as shown`
              : `${text.split("\n").length} lines`}
          </span>
          <div className="ml-auto flex items-center gap-2">
            {!isImage(format) && (
              <Button
                onClick={() => {
                  void copyText(text).then((ok) => {
                    if (!ok) {
                      toast.error("Could not copy", "This webview refused clipboard access.");
                      return;
                    }
                    setCopied(true);
                    setTimeout(() => setCopied(false), 1600);
                  });
                }}
              >
                <span className="flex items-center gap-1.5">
                  {copied ? <Check size={12} /> : <Copy size={12} />}
                  {copied ? "Copied" : "Copy"}
                </span>
              </Button>
            )}
            <Button
              variant="accent"
              onClick={() => {
                const done = (path: string | null) => {
                  if (path) {
                    toast.success("Exported", path);
                    onClose();
                  }
                };
                const failed = (e: unknown) =>
                  toast.error("Export failed", describeError(e));

                if (isImage(format)) {
                  const name = `${repoName}-${capture?.view ?? "view"}.${format}`;
                  let data: string | null = null;
                  if (format === "svg") {
                    data = capture?.toSvg?.() ?? null;
                  } else {
                    const canvas = capture?.canvas() ?? null;
                    data = canvas ? canvasToPng(canvas) : null;
                  }
                  if (!data) {
                    failed(new Error("This view has nothing to export yet."));
                    return;
                  }
                  void api.saveImage(format, name, data).then(done).catch(failed);
                  return;
                }
                void api.exportToFile(format, scope).then(done).catch(failed);
              }}
            >
              <span className="flex items-center gap-1.5">
                <Download size={12} />
                Save
              </span>
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}


function ImageNote({ format, view }: { format: "svg" | "png"; view: string }) {
  return (
    <div className="max-w-[46ch] text-[12px] leading-relaxed" style={{ color: "var(--text-2)" }}>
      <p>
        Saves the <span className="mono">{view}</span> exactly as it is on screen
        — the same pan, zoom and filters, so what you framed is what you get.
      </p>
      <p className="mt-2" style={{ color: "var(--text-3)" }}>
        {format === "svg"
          ? "Vector, drawn from the graph data rather than captured from the screen. It stays sharp at any size and the text stays selectable."
          : "A bitmap at twice screen resolution. Use SVG where you can; PNG is for places that will not render one."}
      </p>
    </div>
  );
}
