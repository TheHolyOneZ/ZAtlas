use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FileId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModuleId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Rust,
    Python,
    Go,
}

impl Language {
    pub const RESOLVED: [Language; 7] = [
        Language::TypeScript,
        Language::Tsx,
        Language::JavaScript,
        Language::Jsx,
        Language::Rust,
        Language::Python,
        Language::Go,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Language::TypeScript => "TypeScript",
            Language::Tsx => "TSX",
            Language::JavaScript => "JavaScript",
            Language::Jsx => "JSX",
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::Go => "Go",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        Some(match ext {
            "ts" | "mts" | "cts" => Language::TypeScript,
            "tsx" => Language::Tsx,
            "js" | "mjs" | "cjs" => Language::JavaScript,
            "jsx" => Language::Jsx,
            "rs" => Language::Rust,
            "py" | "pyi" => Language::Python,
            "go" => Language::Go,
            _ => return None,
        })
    }

    pub fn has_resolver(self) -> bool {
        Self::RESOLVED.contains(&self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FileNode {
    pub id: FileId,

    pub path: String,
    pub module: ModuleId,
    pub language: Language,
    pub loc: u32,
    pub bytes: u64,

    pub content_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum EdgeKind {
    Import,

    ViaBarrel,

    CoChange,

    Contains,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub from: FileId,
    pub to: FileId,
    pub kind: EdgeKind,

    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
pub enum UnresolvedReason {
    External(String),

    NoSuchFile(String),

    UnmatchedAlias(String),

    DynamicSpecifier,

    ReExportChainTooDeep(String),

    NoResolverForLanguage(Language),

    ExcludedFromScan(String),

    FileNotInModuleTree,

    Asset(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedImport {
    pub from: FileId,
    pub specifier: String,
    pub line: u32,
    pub reason: UnresolvedReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CaseMismatch {
    pub from: FileId,
    pub specifier: String,
    pub line: u32,

    pub actual: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LspRepair {
    pub from: FileId,
    pub specifier: String,
    pub line: u32,
    pub to: FileId,
    pub server: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LspDisagreement {
    pub from: FileId,
    pub specifier: String,
    pub line: u32,
    pub lsp_target: FileId,
    pub server: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct EdgeSite {
    pub from: FileId,
    pub to: FileId,
    pub import: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AliasAudit {
    pub pattern: String,
    pub targets: Vec<String>,

    pub source: String,

    pub used_by: u32,

    pub resolves: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ModuleNode {
    pub id: ModuleId,
    pub path: String,
    pub files: Vec<FileId>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Graph {
    pub root: PathBuf,
    pub files: Vec<FileNode>,
    pub modules: Vec<ModuleNode>,
    pub edges: Vec<Edge>,
    pub unresolved: Vec<UnresolvedImport>,

    pub case_mismatches: Vec<CaseMismatch>,

    pub lsp_repairs: Vec<LspRepair>,

    pub lsp_disagreements: Vec<LspDisagreement>,

    pub alias_audit: Vec<AliasAudit>,

    pub edge_sites: Vec<EdgeSite>,
}

impl Graph {
    pub fn file(&self, id: FileId) -> Option<&FileNode> {
        self.files.get(id.0 as usize)
    }

    pub fn module(&self, id: ModuleId) -> Option<&ModuleNode> {
        self.modules.get(id.0 as usize)
    }

    pub fn unresolved_failure_count(&self) -> usize {
        self.unresolved
            .iter()
            .filter(|u| {
                !matches!(
                    u.reason,
                    UnresolvedReason::External(_)
                        | UnresolvedReason::Asset(_)
                        | UnresolvedReason::ExcludedFromScan(_)
                )
            })
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_extension_we_claim_to_support_maps_to_a_language() {
        for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs", "rs", "py", "go"] {
            assert!(
                Language::from_extension(ext).is_some(),
                "{ext} should map to a language"
            );
        }
        assert_eq!(Language::from_extension("md"), None);
    }

    #[test]
    fn a_language_without_a_resolver_is_never_silently_empty() {
        for language in Language::RESOLVED {
            assert!(language.has_resolver(), "{language:?}");
        }
        assert!(Language::Rust.has_resolver());
        assert!(Language::Python.has_resolver());
        assert!(Language::Go.has_resolver());
    }

    #[test]
    fn external_imports_do_not_count_as_resolution_failures() {
        let mut g = Graph::default();
        g.unresolved.push(UnresolvedImport {
            from: FileId(0),
            specifier: "serde".into(),
            line: 1,
            reason: UnresolvedReason::External("serde".into()),
        });
        g.unresolved.push(UnresolvedImport {
            from: FileId(0),
            specifier: "./gone".into(),
            line: 2,
            reason: UnresolvedReason::NoSuchFile("./gone".into()),
        });
        g.unresolved.push(UnresolvedImport {
            from: FileId(0),
            specifier: "./styles.css".into(),
            line: 3,
            reason: UnresolvedReason::Asset("css".into()),
        });
        assert_eq!(g.unresolved_failure_count(), 1);
    }
}
