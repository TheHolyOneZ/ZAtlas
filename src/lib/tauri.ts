import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { Finding } from "./bindings/Finding";
import type { FindingKind } from "./bindings/FindingKind";
import type { FindingPage } from "./bindings/FindingPage";
import type { EdgeDetail } from "./bindings/EdgeDetail";
import type { GraphPayload } from "./bindings/GraphPayload";
import type { ImpactResult } from "./bindings/ImpactResult";
import type { LayoutPayload } from "./bindings/LayoutPayload";
import type { LspMode } from "./bindings/LspMode";
import type { LspServerDto } from "./bindings/LspServerDto";
import type { NodeDetail } from "./bindings/NodeDetail";
import type { RepoInfo } from "./bindings/RepoInfo";
import type { ScanProgress } from "./bindings/ScanProgress";
import type { ScanSummary } from "./bindings/ScanSummary";
import type { StartupArgs } from "./bindings/StartupArgs";
import type { RefComparison } from "./bindings/RefComparison";
import type { Timeline } from "./bindings/Timeline";
import type { ZatlasConfig } from "./bindings/ZatlasConfig";
import type { SymbolGraph } from "./bindings/SymbolGraph";
import type { TourStop } from "./bindings/TourStop";

export type {
  EdgeDetail,
  Finding,
  FindingKind,
  FindingPage,
  GraphPayload,
  ImpactResult,
  LayoutPayload,
  LspMode,
  LspServerDto,
  NodeDetail,
  RepoInfo,
  ScanProgress,
  RefComparison,
  ScanSummary,
  StartupArgs,
  Timeline,
  SymbolGraph,
  TourStop,
  ZatlasConfig,
};


export interface CommandError {
  code: string;
  message: string;
  remedy: string;
  retryable: boolean;
}

function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value &&
    "remedy" in value
  );
}

export function toCommandError(error: unknown): CommandError {
  if (isCommandError(error)) return error;
  return {
    code: "unknown",
    message: typeof error === "string" ? error : String(error),
    remedy: "Please report this.",
    retryable: false,
  };
}

export function describeError(error: unknown): string {
  return toCommandError(error).message;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toCommandError(error);
  }
}


export const api = {
  startupArgs: () => call<StartupArgs>("startup_args"),
  pickRepo: () => call<string | null>("pick_repo"),
  openRepo: (path: string) => call<RepoInfo>("open_repo", { path }),
  hasAnalysis: () => call<boolean>("has_analysis"),
  openInEditor: (path: string) => call<void>("open_in_editor", { path }),


  startScan: (path: string) => call<string>("start_scan", { path }),

  startWorkspaceScan: (paths: string[]) => call<string>("start_workspace_scan", { paths }),
  cancelScan: (scanId: string) => call<boolean>("cancel_scan", { scanId }),
  setWatching: (enabled: boolean) => call<boolean>("set_watching", { enabled }),
  getSummary: () => call<ScanSummary | null>("get_summary"),


  setLspMode: (mode: LspMode | null) => call<void>("set_lsp_mode", { mode }),

  lspServers: () => call<LspServerDto[]>("lsp_servers"),


  getGraph: () => call<GraphPayload>("get_graph"),

  getLayout: () => call<LayoutPayload>("get_layout"),
  getNode: (id: number) => call<NodeDetail>("get_node", { id }),

  edgeDetail: (from: number, to: number) =>
    call<EdgeDetail>("edge_detail", { from, to }),
  impact: (ids: number[]) => call<ImpactResult>("impact", { ids }),
  dsmOrder: () => call<number[]>("dsm_order"),
  getTour: () => call<TourStop[]>("get_tour"),
  getSymbols: (module: number) => call<SymbolGraph>("get_symbols", { module }),
  getTimeline: (steps?: number) => call<Timeline>("get_timeline", { steps: steps ?? null }),

  getConfig: () => call<ZatlasConfig>("get_config"),

  layerCandidates: () => call<[string, number][]>("layer_candidates"),

  writeLayers: (layers: { name: string; paths: string[] }[], rules: string[]) =>
    call<string>("write_layers", { layers, rules }),


  listRefs: () => call<string[]>("list_refs"),


  compareRefs: (base: string, head: string) =>
    call<RefComparison>("compare_refs", { base, head }),
  cycleMembers: () => call<number[][]>("cycle_members"),

  getFindings: (
    offset: number,
    limit: number,
    kinds?: FindingKind[],
    query?: string,
    includeAccepted?: boolean,
  ) =>
    call<FindingPage>("get_findings", {
      offset,
      limit,
      kinds: kinds ?? null,
      query: query ?? null,
      includeAccepted: includeAccepted ?? null,
    }),


  acceptBaseline: () => call<string>("accept_baseline"),


  acceptFinding: (index: number, note?: string) =>
    call<void>("accept_finding", { index, note: note ?? null }),

  unacceptFinding: (index: number) => call<void>("unaccept_finding", { index }),
  findingCounts: () => call<[FindingKind, number][]>("finding_counts"),


  exportText: (format: ExportFormat, scope?: "modules" | "files", only?: number[]) =>
    call<string>("export_text", {
      request: { format, scope: scope ?? null, only: only ?? null },
    }),


  saveImage: (format: ImageFormat, suggested: string, data: string) =>
    call<string | null>("save_image", { format, suggested, data }),


  exportToFile: (format: ExportFormat, scope?: "modules" | "files", only?: number[]) =>
    call<string | null>("export_to_file", {
      request: { format, scope: scope ?? null, only: only ?? null },
    }),
} as const;

export type ExportFormat = "d2" | "mermaid" | "markdown";

export type ImageFormat = "svg" | "png";


export const EVENTS = {
  scanProgress: "zatlas://scan-progress",
  scanNodes: "zatlas://scan-nodes",
  scanRollup: "zatlas://scan-rollup",
  scanDone: "zatlas://scan-done",
  scanFailed: "zatlas://scan-failed",
  scanCancelled: "zatlas://scan-cancelled",
  gitProgress: "zatlas://git-progress",
  layoutDone: "zatlas://layout-done",
  repoChanged: "zatlas://repo-changed",
} as const;

export function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (message) => handler(message.payload));
}
