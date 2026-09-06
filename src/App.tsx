import { useEffect, useState } from "react";

import CommandPalette from "./components/CommandPalette";
import CompareDialog from "./components/CompareDialog";
import LayersDialog from "./components/LayersDialog";
import ExportDialog from "./components/ExportDialog";
import FilterPanel from "./components/FilterPanel";
import PresetPanel from "./components/PresetPanel";
import FindingsPanel from "./components/FindingsPanel";
import GraphView from "./components/GraphView";
import Inspector from "./components/Inspector";
import RepoList from "./components/RepoList";
import SymbolPanel from "./components/SymbolPanel";
import MatrixView from "./components/MatrixView";
import Settings from "./components/Settings";
import TimelineView from "./components/TimelineView";
import Titlebar from "./components/Titlebar";
import Toasts from "./components/Toasts";
import TopBar from "./components/TopBar";
import TourPanel from "./components/TourPanel";
import TreemapView from "./components/TreemapView";
import ViewSwitcher from "./components/ViewSwitcher";
import { Button, Empty, RailLabel, Stat, ago, compact } from "./components/ui";
import { useKeyboard } from "./hooks/useKeyboard";
import { useGraphStore } from "./store/useGraphStore";
import { useFindingCursor } from "./store/useFindingCursor";
import { lastRepo } from "./store/useRecentStore";
import { useRepoStore } from "./store/useRepoStore";
import { useSelectionStore } from "./store/useSelectionStore";
import { useViewStore, type ViewId } from "./store/useViewStore";
import { api } from "./lib/tauri";

export default function App() {
  const { state, summary, version, subscribe, pick, rescan } = useRepoStore();
  const graphStore = useGraphStore();
  const selection = useSelectionStore();
  const { view, setView } = useViewStore();
  const [exporting, setExporting] = useState(false);
  const [comparing, setComparing] = useState(false);
  const [layers, setLayers] = useState(false);
  const [palette, setPalette] = useState(false);
  const [tour, setTour] = useState(false);
  const [symbols, setSymbols] = useState<{ id: number; label: string } | null>(null);
  const [settings, setSettings] = useState(false);


  useEffect(() => {
    let dispose: (() => void) | undefined;
    let cancelled = false;
    void subscribe().then((fn) => {
      if (cancelled) fn();
      else dispose = fn;
    });
    return () => {
      cancelled = true;
      dispose?.();
    };
  }, [subscribe]);


  useEffect(() => {
    void api.startupArgs().then((args) => {
      if (args.view) setView(args.view as ViewId);


      const path = args.path ?? lastRepo();
      if (path) void useRepoStore.getState().open(path);
    });

  }, []);


  useEffect(() => {
    if (state === "ready") {
      void graphStore.load();
      void selection.select(null);
    }
    if (state === "scanning") graphStore.clear();

  }, [version, state]);

  useKeyboard(
    {
    "ctrl+k": () => summary && setPalette(true),
    "1": () => setView("graph"),
    "2": () => setView("treemap"),
    "3": () => setView("matrix"),
    "4": () => setView("timeline"),
    f5: () => void rescan(),
    "ctrl+e": () => summary && setExporting(true),
    "?": () => summary && setTour(true),
    "ctrl+,": () => setSettings(true),
    escape: () => selection.clearHighlight(),
    "]": () => useFindingCursor.getState().step(1),
    "[": () => useFindingCursor.getState().step(-1),
    i: () => void selection.runImpact(),
    e: () => {
      const path = selection.detail?.path;
      if (path) void api.openInEditor(path);
    },
    },


    !(palette || exporting || comparing || layers || settings || tour),
  );

  return (
    <div className="h-full flex flex-col">
      <Titlebar subtitle={summary?.name} onSettings={() => setSettings(true)} />
      <TopBar />

      <div className="flex-1 flex min-h-0">
        <aside
          className="w-[240px] shrink-0 min-h-0 overflow-y-auto"
          style={{ background: "var(--surface)", borderRight: "1px solid var(--border)" }}
        >
          {summary ? (
            <>
              <RailLabel>Repository</RailLabel>
              <Stat label="files" value={compact(summary.fileCount)} />
              <Stat label="modules" value={compact(summary.moduleCount)} />
              <Stat label="edges" value={compact(summary.edgeCount)} />
              <Stat label="lines" value={compact(Number(summary.totalLoc))} />

              <RailLabel>Health</RailLabel>
              <Stat label="findings" value={summary.findingCount} />
              <Stat label="cycles" value={summary.cycleCount} warn={summary.cycleCount > 0} />
              <Stat
                label="unresolved"
                value={summary.unresolvedCount}
                warn={summary.unresolvedCount > 0}
                title="Imports ZAtlas could not resolve. Surfaced, never hidden."
              />
              <Stat label="external" value={compact(summary.externalCount)} />
              <Stat label="scanned" value={ago(Number(summary.scannedAt) / 1000)} />
              <RepoList />
              {summary.invalidRules.length > 0 && (
                <div className="px-3 py-2 text-[11px]" style={{ color: "var(--warn)" }}>
                  {summary.invalidRules.length} layer rule
                  {summary.invalidRules.length === 1 ? "" : "s"} name an undeclared layer
                </div>
              )}
              <FilterPanel />
              <PresetPanel />
            </>
          ) : (
            <div className="p-3">
              <RailLabel>No repository</RailLabel>
            </div>
          )}
        </aside>

        <main className="flex-1 flex flex-col min-w-0">
          <div
            className="flex items-stretch gap-2 h-10 shrink-0"
            style={{
              borderBottom: "1px solid var(--border)",
              background: "var(--surface-2)",
            }}
          >
            <ViewSwitcher />
            <div className="ml-auto flex items-center gap-2 pr-3">
              {summary && (
                <>
                  <Button
                    onClick={() => setTour(true)}
                    title="Where to start reading (?)"
                  >
                    Where to start
                  </Button>
                  {summary.isGit && (
                    <Button
                      onClick={() => setComparing(true)}
                      title="What a branch did to the shape of the codebase"
                    >
                      Compare
                    </Button>
                  )}
                  <Button
                    onClick={() => setLayers(true)}
                    title="Declare layers and the directions allowed between them"
                  >
                    Layers
                  </Button>
                  <Button onClick={() => setExporting(true)} title="Export (Ctrl+E)">
                    Export
                  </Button>
                </>
              )}
            </div>
          </div>

          <div className="contour-ground relative flex-1 min-h-0">
            {state === "idle" && !summary && (
              <Empty
                title="Point ZAtlas at a repository"
                hint="It reads the code and the git history, and never writes anything back."
                action={
                  <Button variant="accent" onClick={() => void pick()}>
                    Choose a folder
                  </Button>
                }
              />
            )}
            {state === "failed" && (
              <Empty
                title="That scan did not finish"
                hint={useRepoStore.getState().error ?? undefined}
                action={<Button onClick={() => void pick()}>Try another folder</Button>}
              />
            )}
            {summary && view === "graph" && <GraphView />}
            {summary && view === "treemap" && <TreemapView />}
            {summary && view === "matrix" && <MatrixView />}
            {summary && view === "timeline" && <TimelineView />}
          </div>


          <div
            className="shrink-0 max-h-[248px] min-h-[92px] overflow-hidden flex flex-col"
            style={{ borderTop: "1px solid var(--border)" }}
          >
            <FindingsPanel />
          </div>
        </main>

        <aside
          className="w-[300px] shrink-0 min-h-0"
          style={{ background: "var(--surface)", borderLeft: "1px solid var(--border)" }}
        >
          <Inspector
            onShowSymbols={(label) => {
              const mod = graphStore.graph?.modules.find((m) => m.path === label);
              if (mod) setSymbols({ id: mod.id, label });
            }}
          />
        </aside>
      </div>

      {symbols && (
        <SymbolPanel
          module={symbols.id}
          label={symbols.label}
          onClose={() => setSymbols(null)}
        />
      )}
      {settings && <Settings onClose={() => setSettings(false)} />}
      {tour && <TourPanel onClose={() => setTour(false)} />}
      {palette && <CommandPalette onClose={() => setPalette(false)} />}
      {exporting && <ExportDialog onClose={() => setExporting(false)} />}
      {comparing && <CompareDialog onClose={() => setComparing(false)} />}
      {layers && <LayersDialog onClose={() => setLayers(false)} />}
      <Toasts />
    </div>
  );
}
