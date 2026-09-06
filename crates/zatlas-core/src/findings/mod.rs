mod detectors;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

use crate::config::ZatlasConfig;
use crate::git::history::FileHistory;
use crate::git::CoupledPair;
use crate::graph::{Adjacency, Relation};
use crate::model::{FileId, Graph};

pub use detectors::layers::{layer_of, LayerMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum FindingKind {
    Cycle,
    GodFile,
    Orphan,
    UnstableInterface,
    LayeringViolation,
    BusFactor,
    DistantCoupling,
    BarrelHub,
    CaseMismatch,
    LspDisagreement,
    PathAlias,
}

impl FindingKind {
    pub fn label(self) -> &'static str {
        match self {
            FindingKind::Cycle => "Import cycle",
            FindingKind::GodFile => "God file",
            FindingKind::Orphan => "Orphan",
            FindingKind::UnstableInterface => "Unstable interface",
            FindingKind::LayeringViolation => "Layering violation",
            FindingKind::BusFactor => "Bus factor",
            FindingKind::DistantCoupling => "Distant coupling",
            FindingKind::BarrelHub => "Barrel hub",
            FindingKind::CaseMismatch => "Case mismatch",
            FindingKind::LspDisagreement => "Resolver disagreement",
            FindingKind::PathAlias => "Path alias",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub label: String,
    pub value: String,
}

impl Evidence {
    pub fn new(label: impl Into<String>, value: impl std::fmt::Display) -> Self {
        Self {
            label: label.into(),
            value: value.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub kind: FindingKind,
    pub severity: Severity,

    pub score: f32,

    pub headline: String,
    pub files: Vec<FileId>,
    pub evidence: Vec<Evidence>,
    pub why: String,
    pub how_to_fix: String,
}

pub struct FindingsInput<'a> {
    pub graph: &'a Graph,
    pub deps: &'a Adjacency,
    pub structural: &'a Adjacency,
    pub config: &'a ZatlasConfig,
    pub history: &'a HashMap<String, FileHistory>,
    pub coupling: &'a [CoupledPair],

    pub entrypoints: Vec<FileId>,
}

pub struct FindingsOptions {
    pub max_per_kind: usize,
}

impl Default for FindingsOptions {
    fn default() -> Self {
        Self { max_per_kind: 100 }
    }
}

pub fn detect(input: &FindingsInput, opts: &FindingsOptions) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(detectors::alias::detect(input));
    out.extend(detectors::case_mismatch::detect(input));
    out.extend(detectors::cycles::detect(input));
    out.extend(detectors::god_file::detect(input));
    out.extend(detectors::orphans::detect(input));
    out.extend(detectors::unstable::detect(input));
    out.extend(detectors::layers::detect(input));
    out.extend(detectors::lsp_disagreement::detect(input));
    out.extend(detectors::bus_factor::detect(input));
    out.extend(detectors::distant_coupling::detect(input));

    let mut per_kind: HashMap<FindingKind, usize> = HashMap::new();
    out.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| a.headline.cmp(&b.headline))
    });
    out.retain(|f| {
        let n = per_kind.entry(f.kind).or_insert(0);
        *n += 1;
        *n <= opts.max_per_kind
    });
    out
}

pub fn percentile(sorted_ascending: &[f64], value: f64) -> f32 {
    if sorted_ascending.is_empty() {
        return 0.0;
    }
    let below = sorted_ascending.partition_point(|&x| x < value);
    below as f32 / sorted_ascending.len() as f32
}

pub fn severity_for(score: f32) -> Severity {
    match score {
        s if s >= 0.95 => Severity::Critical,
        s if s >= 0.85 => Severity::High,
        s if s >= 0.65 => Severity::Medium,
        s if s >= 0.4 => Severity::Low,
        _ => Severity::Info,
    }
}

pub fn prepare<'a>(
    graph: &'a Graph,
    deps: &'a Adjacency,
    structural: &'a Adjacency,
    config: &'a ZatlasConfig,
    history: &'a HashMap<String, FileHistory>,
    coupling: &'a [CoupledPair],
) -> FindingsInput<'a> {
    let entrypoints = resolve_entrypoints(graph, config);
    FindingsInput {
        graph,
        deps,
        structural,
        config,
        history,
        coupling,
        entrypoints,
    }
}

fn is_root_index(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if !matches!(name, "index.ts" | "index.tsx" | "index.js" | "index.jsx") {
        return false;
    }
    let dir = match path.rfind('/') {
        Some(i) => &path[..i],
        None => return true,
    };
    dir == "src" || dir.ends_with("/src")
}

pub fn entrypoints_of(graph: &Graph, config: &ZatlasConfig) -> Vec<FileId> {
    resolve_entrypoints(graph, config)
}

fn resolve_entrypoints(graph: &Graph, config: &ZatlasConfig) -> Vec<FileId> {
    let mut out: Vec<FileId> = Vec::new();

    if !config.entrypoints.is_empty() {
        let mut builder = globset::GlobSetBuilder::new();
        for p in &config.entrypoints {
            if let Ok(g) = globset::Glob::new(p) {
                builder.add(g);
            }
        }
        if let Ok(set) = builder.build() {
            for f in &graph.files {
                if set.is_match(&f.path) {
                    out.push(f.id);
                }
            }
        }
    }

    if out.is_empty() {
        const ROOT_NAMES: &[&str] = &[
            "main.rs", "lib.rs", "build.rs", "main.tsx", "main.ts", "App.tsx",
        ];
        for f in &graph.files {
            let name = f.path.rsplit('/').next().unwrap_or(&f.path);
            if ROOT_NAMES.contains(&name) || is_root_index(&f.path) {
                out.push(f.id);
            }
        }
    }

    out.sort_unstable_by_key(|f| f.0);
    out.dedup();
    out
}

pub fn detect_all(
    graph: &Graph,
    config: &ZatlasConfig,
    history: &HashMap<String, FileHistory>,
    coupling: &[CoupledPair],
    opts: &FindingsOptions,
) -> Vec<Finding> {
    let deps = Adjacency::build(graph.files.len(), &graph.edges);
    let structural = Adjacency::build_for(graph.files.len(), &graph.edges, Relation::Structural);
    let input = prepare(graph, &deps, &structural, config, history, coupling);
    detect(&input, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_places_a_value_among_its_peers() {
        let sorted = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile(&sorted, 1.0), 0.0);
        assert_eq!(percentile(&sorted, 3.0), 0.5);
        assert_eq!(percentile(&sorted, 5.0), 1.0);
    }

    #[test]
    fn percentile_of_an_empty_population_is_zero_rather_than_a_panic() {
        assert_eq!(percentile(&[], 5.0), 0.0);
    }

    #[test]
    fn severity_rises_with_score() {
        assert_eq!(severity_for(0.0), Severity::Info);
        assert_eq!(severity_for(0.5), Severity::Low);
        assert_eq!(severity_for(0.7), Severity::Medium);
        assert_eq!(severity_for(0.9), Severity::High);
        assert_eq!(severity_for(1.0), Severity::Critical);
    }

    #[test]
    fn a_barrel_is_not_mistaken_for_an_entry_point() {
        assert!(is_root_index("src/index.ts"));
        assert!(is_root_index("apps/web/src/index.tsx"));
        assert!(is_root_index("index.ts"));
        assert!(!is_root_index("src/store/index.ts"));
        assert!(!is_root_index("src/components/ui/index.ts"));
        assert!(!is_root_index("src/main.tsx"));
    }

    #[test]
    fn severity_orders_correctly_for_ranking() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Medium);
        assert!(Severity::Info < Severity::Low);
    }
}
