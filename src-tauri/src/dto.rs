use serde::Serialize;
use ts_rs::TS;
use zatlas_core::findings::{Finding, Severity};
use zatlas_core::model::{EdgeKind, FileId, Language, UnresolvedReason};

use crate::state::Loaded;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScanSummary {
    pub root: String,
    pub name: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub is_git: bool,
    pub file_count: usize,
    pub module_count: usize,
    pub edge_count: usize,
    pub total_loc: u64,

    pub unresolved_count: usize,
    pub external_count: usize,
    pub finding_count: usize,
    pub cycle_count: usize,
    pub commit_count: usize,
    pub scanned_at: u64,
    pub invalid_rules: Vec<String>,

    pub lsp: zatlas_core::lsp::LspReport,
}

impl ScanSummary {
    pub fn of(loaded: &Loaded) -> Self {
        let g = &loaded.analysis.graph;
        Self {
            root: loaded.info.root.clone(),
            name: loaded.info.name.clone(),
            branch: loaded.info.branch.clone(),
            head: loaded.info.head.clone(),
            is_git: loaded.info.is_git,
            file_count: g.files.len(),
            module_count: g.modules.len(),
            edge_count: g.edges.len(),
            total_loc: g.files.iter().map(|f| f.loc as u64).sum(),
            unresolved_count: g.unresolved_failure_count(),
            external_count: g
                .unresolved
                .iter()
                .filter(|u| matches!(u.reason, UnresolvedReason::External(_)))
                .count(),
            finding_count: loaded.findings.len(),
            cycle_count: loaded
                .findings
                .iter()
                .filter(|f| f.kind == zatlas_core::findings::FindingKind::Cycle)
                .count(),
            commit_count: loaded.history.len(),
            scanned_at: loaded.scanned_at,
            invalid_rules: loaded.info.invalid_rules.clone(),
            lsp: loaded.lsp.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LspServerDto {
    pub id: String,
    pub label: String,
    pub command: String,

    pub installed: bool,

    pub applicable: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NodeDto {
    pub id: u32,
    pub path: String,

    pub name: String,
    pub module: u32,
    pub language: Language,
    pub loc: u32,
    pub fan_in: u32,
    pub fan_out: u32,
    pub churn: u32,
    pub authors: u32,

    pub heat: f32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EdgeDto {
    pub from: u32,
    pub to: u32,
    pub kind: EdgeKind,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModuleDto {
    pub id: u32,
    pub path: String,
    pub name: String,
    pub file_count: usize,
    pub loc: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GraphPayload {
    pub nodes: Vec<NodeDto>,
    pub edges: Vec<EdgeDto>,
    pub modules: Vec<ModuleDto>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportSite {
    pub line: u32,
    pub specifier: String,

    pub kind: String,

    pub type_only: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EdgeDetail {
    pub from: u32,
    pub to: u32,
    pub from_path: String,
    pub to_path: String,
    pub kind: EdgeKind,

    pub sites: Vec<ImportSite>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NodeDetail {
    pub id: u32,
    pub path: String,
    pub language: Language,
    pub loc: u32,
    pub bytes: u64,
    pub module: String,
    pub fan_in: u32,
    pub fan_out: u32,
    pub churn: u32,
    pub authors: u32,
    pub top_author: String,
    pub top_author_share: f32,

    pub last_touched: i64,
    pub exports: Vec<String>,
    pub definitions: u32,
    pub dependencies: Vec<NodeRef>,
    pub dependents: Vec<NodeRef>,
    pub unresolved: Vec<UnresolvedDto>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NodeRef {
    pub id: u32,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UnresolvedDto {
    pub specifier: String,
    pub line: u32,

    pub reason: String,
    pub detail: String,

    pub is_failure: bool,
}

impl UnresolvedDto {
    pub fn of(u: &zatlas_core::model::UnresolvedImport) -> Self {
        let (reason, detail, is_failure) = match &u.reason {
            UnresolvedReason::External(p) => ("external", p.clone(), false),
            UnresolvedReason::Asset(e) => ("asset", e.clone(), false),
            UnresolvedReason::ExcludedFromScan(p) => ("excluded from scan", p.clone(), false),
            UnresolvedReason::NoSuchFile(p) => ("no such file", p.clone(), true),
            UnresolvedReason::UnmatchedAlias(p) => ("alias points nowhere", p.clone(), true),
            UnresolvedReason::DynamicSpecifier => ("computed specifier", String::new(), true),
            UnresolvedReason::ReExportChainTooDeep(p) => {
                ("re-export chain too deep", p.clone(), true)
            }
            UnresolvedReason::NoResolverForLanguage(l) => {
                ("no resolver yet", l.label().to_string(), false)
            }

            UnresolvedReason::FileNotInModuleTree => {
                ("file is not part of any crate", String::new(), true)
            }
        };
        Self {
            specifier: u.specifier.clone(),
            line: u.line,
            reason: reason.to_string(),
            detail,
            is_failure,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FindingPage {
    pub total: usize,
    pub rows: Vec<Finding>,

    pub indices: Vec<usize>,

    pub is_accepted: Vec<bool>,

    pub accepted: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImpactResult {
    pub seeds: Vec<u32>,
    pub affected: Vec<u32>,
    pub total: usize,

    pub share: f32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LayoutPayload {
    pub xy: Vec<f32>,
    pub radius: Vec<f32>,
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScanProgress {
    pub scan_id: String,
    pub phase: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SortBy {
    Severity,
    Path,
    Loc,
    FanIn,
    Churn,
}

pub fn heat_of(severity: Severity) -> f32 {
    match severity {
        Severity::Info => 0.1,
        Severity::Low => 0.35,
        Severity::Medium => 0.6,
        Severity::High => 0.85,
        Severity::Critical => 1.0,
    }
}

pub fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

pub fn node_ref(loaded: &Loaded, id: FileId) -> Option<NodeRef> {
    loaded.analysis.graph.file(id).map(|f| NodeRef {
        id: f.id.0,
        path: f.path.clone(),
    })
}
