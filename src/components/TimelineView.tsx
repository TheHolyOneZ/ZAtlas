import {useEffect, useMemo, useRef, useState} from "react";
import { Pause, Play, RotateCcw } from "lucide-react";

import { Button, Empty, Spinner, compact } from "./ui";
import { serialiseSvgElement } from "../lib/graphSvg";
import { useCaptureStore } from "../store/useCaptureStore";
import { useElementSize } from "../hooks/useElementSize";
import { api, describeError, type Timeline } from "../lib/tauri";
import { useRepoStore } from "../store/useRepoStore";


const FRAME_MS = 650;

export default function TimelineView() {
  const svgRef = useRef<SVGSVGElement | null>(null);


  const setCapture = useCaptureStore((s) => s.setCapture);
  useEffect(() => {
    setCapture({
      view: "timeline",


      canvas: () => null,
      toSvg: () => {
        const svg = svgRef.current;
        if (!svg) return "";
        const css = getComputedStyle(document.documentElement);
        return serialiseSvgElement(
          svg,
          css.getPropertyValue("--surface").trim() || "#111a19",
          (name) => css.getPropertyValue(name),
        );
      },
    });
    return () => setCapture(null);
  }, [setCapture]);

  const { ref: wrapRef, size } = useElementSize<HTMLDivElement>();
  const version = useRepoStore((s) => s.version);
  const summary = useRepoStore((s) => s.summary);

  const [timeline, setTimeline] = useState<Timeline | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [at, setAt] = useState(0);
  const [playing, setPlaying] = useState(false);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    if (!summary?.isGit) {
      setTimeline(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    void api
      .getTimeline(12)
      .then((t) => {
        if (cancelled) return;
        setTimeline(t);
        setAt(Math.max(0, t.snapshots.length - 1));
      })
      .catch((e) => {
        if (!cancelled) setError(describeError(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [version, summary?.isGit]);


  useEffect(() => {
    if (!playing || !timeline) return;
    timer.current = window.setInterval(() => {
      setAt((i) => {
        if (i >= timeline.snapshots.length - 1) {
          setPlaying(false);
          return i;
        }
        return i + 1;
      });
    }, FRAME_MS);
    return () => {
      if (timer.current) window.clearInterval(timer.current);
      timer.current = null;
    };
  }, [playing, timeline]);

  const snaps = timeline?.snapshots ?? [];
  const current = snaps[at];
  const change = at > 0 ? timeline?.diffs[at - 1] : undefined;

  const maxLoc = useMemo(
    () => Math.max(1, ...snaps.map((s) => s.totalLoc)),
    [snaps],
  );

  const chartH = Math.max(80, size.h - 260);
  const chartW = Math.max(1, size.w - 32);

  return (
    <div ref={wrapRef} className="relative h-full w-full overflow-auto">
      {loading && (
        <div className="absolute inset-0 grid place-items-center z-10">
          <Spinner label="reading history" />
        </div>
      )}
      {!loading && error && (
        <div className="absolute inset-0 z-10">
          <Empty title="No timeline" hint={error} />
        </div>
      )}
      {!loading && !error && snaps.length === 0 && (
        <div className="absolute inset-0 z-10">
          <Empty
            title="No history to travel through"
            hint="This folder has no commits in the last year, so there is nothing to play back."
          />
        </div>
      )}

      {snaps.length > 0 && (
        <div className="p-4">

          <svg
            ref={svgRef}
            width={chartW}
            height={chartH}
            role="img"
            aria-label="Lines of code over time"
          >
            <polyline
              fill="none"
              stroke="var(--accent)"
              strokeWidth={1.5}
              points={snaps
                .map((s, i) => {
                  const x = (i / Math.max(1, snaps.length - 1)) * (chartW - 8) + 4;
                  const y = chartH - 8 - (s.totalLoc / maxLoc) * (chartH - 24);
                  return `${x},${y}`;
                })
                .join(" ")}
            />
            {snaps.map((s, i) => {
              const x = (i / Math.max(1, snaps.length - 1)) * (chartW - 8) + 4;
              const y = chartH - 8 - (s.totalLoc / maxLoc) * (chartH - 24);
              const on = i === at;
              return (
                <g key={s.commit}>
                  <circle
                    cx={x}
                    cy={y}
                    r={on ? 5 : 3}
                    fill={on ? "var(--accent)" : "var(--surface)"}
                    stroke="var(--accent)"
                    strokeWidth={1.5}
                    style={{ cursor: "pointer" }}
                    onClick={() => {
                      setPlaying(false);
                      setAt(i);
                    }}
                  />
                  {(on || snaps.length <= 12) && (


                    <text
                      x={x}
                      y={chartH - 1}
                      textAnchor={i === 0 ? "start" : i === snaps.length - 1 ? "end" : "middle"}
                      fontSize={9}
                      fill={on ? "var(--accent)" : "var(--text-3)"}
                      style={{ fontFamily: "var(--font-mono)" }}
                    >
                      {s.label}
                    </text>
                  )}
                </g>
              );
            })}
          </svg>

          <div className="flex items-center gap-3 mt-3">
            <Button
              variant="accent"
              onClick={() => {
                if (at >= snaps.length - 1) setAt(0);
                setPlaying(!playing);
              }}
              title="Play the last twelve months"
            >
              <span className="flex items-center gap-1.5">
                {playing ? <Pause size={12} /> : <Play size={12} />}
                {playing ? "Pause" : "Play"}
              </span>
            </Button>
            <Button
              variant="ghost"
              onClick={() => {
                setPlaying(false);
                setAt(0);
              }}
              title="Back to the start"
            >
              <RotateCcw size={12} />
            </Button>
            <input
              type="range"
              min={0}
              max={snaps.length - 1}
              value={at}
              onChange={(e) => {
                setPlaying(false);
                setAt(Number(e.target.value));
              }}
              className="flex-1 accent-[var(--accent)]"
            />
            <span className="mono text-[11px]" style={{ color: "var(--text-3)" }}>
              {at + 1}/{snaps.length}
            </span>
          </div>

          {current && (
            <div className="mt-4 flex flex-wrap gap-x-8 gap-y-1">
              <Metric label="at" value={current.label} />
              <Metric label="commit" value={current.commit} />
              <Metric label="files" value={compact(current.fileCount)} />
              <Metric label="lines" value={compact(current.totalLoc)} />
              <Metric label="imports" value={compact(current.edgeCount)} />
            </div>
          )}

          {change && (
            <div className="mt-4 grid grid-cols-2 gap-4 max-w-[900px]">
              <ChangeList
                title={`Added (${change.added.length})`}
                colour="var(--ok)"
                rows={change.added}
              />
              <ChangeList
                title={`Removed (${change.removed.length})`}
                colour="var(--cycle)"
                rows={change.removed}
              />
              <ChangeList
                title={`Grew (${change.grown.length})`}
                colour="var(--warn)"
                rows={change.grown.map(([p, d]) => `${p}  +${d}`)}
              />
              <ChangeList
                title={`Shrank (${change.shrunk.length})`}
                colour="var(--text-2)"
                rows={change.shrunk.map(([p, d]) => `${p}  ${d}`)}
              />
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-[10px] uppercase tracking-[0.1em]" style={{ color: "var(--text-3)" }}>
        {label}
      </div>
      <div className="mono" style={{ color: "var(--text)" }}>
        {value}
      </div>
    </div>
  );
}

function ChangeList({
  title,
  colour,
  rows,
}: {
  title: string;
  colour: string;
  rows: string[];
}) {
  return (
    <div>
      <div className="text-[11px] mb-1" style={{ color: colour }}>
        {title}
      </div>
      {rows.length === 0 ? (
        <div className="text-[11px]" style={{ color: "var(--text-3)" }}>
          nothing
        </div>
      ) : (
        <div className="max-h-[160px] overflow-y-auto">
          {rows.slice(0, 40).map((r) => (
            <div
              key={r}
              className="mono truncate text-[11px]"
              style={{ color: "var(--text-2)" }}
              title={r}
            >
              {r}
            </div>
          ))}
          {rows.length > 40 && (
            <div className="text-[11px]" style={{ color: "var(--text-3)" }}>
              and {rows.length - 40} more
            </div>
          )}
        </div>
      )}
    </div>
  );
}
