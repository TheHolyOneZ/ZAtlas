pub mod analyse;
pub mod baseline;
pub mod cache;
pub mod config;
pub mod error;
pub mod export;
pub mod findings;
pub mod git;
pub mod graph;
pub mod layout;
pub mod lsp;
pub mod model;
pub mod parse;
pub mod paths;
pub mod repo;
pub mod resolve;
pub mod symbols;
pub mod tour;
pub mod walk;
pub mod watch;
pub mod workspace;

pub use baseline::{Baseline, BaselineEntry};
pub use config::{Layer, LayerRule, ZatlasConfig};
pub use error::{CoreError, Result};
pub use lsp::{LspMode, LspReport, LspSettings};
pub use model::{
    AliasAudit, Edge, EdgeKind, EdgeSite, FileId, FileNode, Graph, Language, LspDisagreement,
    LspRepair, ModuleId, ModuleNode, UnresolvedImport, UnresolvedReason,
};
pub use repo::RepoInfo;

pub const CACHE_SCHEMA_VERSION: u32 = 1;
