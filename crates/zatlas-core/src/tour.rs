use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

use crate::git::history::FileHistory;
use crate::graph::{Adjacency, Relation};
use crate::model::{FileId, Graph};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct TourStop {
    pub file: FileId,
    pub path: String,
    pub loc: u32,

    pub reason: String,

    pub unlocks: usize,
}

pub struct TourOptions {
    pub max_stops: usize,

    pub min_loc: u32,
}

impl Default for TourOptions {
    fn default() -> Self {
        Self {
            max_stops: 10,
            min_loc: 25,
        }
    }
}

pub fn build(
    graph: &Graph,
    entrypoints: &[FileId],
    history: &HashMap<String, FileHistory>,
    opts: &TourOptions,
) -> Vec<TourStop> {
    if graph.files.is_empty() {
        return Vec::new();
    }

    let deps = Adjacency::build(graph.files.len(), &graph.edges);
    let structural = Adjacency::build_for(graph.files.len(), &graph.edges, Relation::Structural);

    let mut stops: Vec<TourStop> = Vec::new();
    let mut covered: Vec<bool> = vec![false; graph.files.len()];

    let rank = |path: &str| -> u8 {
        let name = path.rsplit('/').next().unwrap_or(path);
        match name {
            "main.rs" | "main.ts" | "main.tsx" => 0,
            "App.tsx" | "app.tsx" => 1,
            "lib.rs" => 2,
            _ => 3,
        }
    };
    let mut entries: Vec<&FileId> = entrypoints
        .iter()
        .filter(|id| graph.file(**id).is_some())
        .collect();
    entries.sort_by_key(|id| {
        let f = graph.file(**id);
        let path = f.map(|f| f.path.as_str()).unwrap_or("");
        (rank(path), std::cmp::Reverse(f.map(|f| f.loc).unwrap_or(0)))
    });

    for id in entries.iter().take(2) {
        let Some(f) = graph.file(**id) else { continue };

        if f.loc < 2 {
            continue;
        }

        let reach = crate::graph::dependencies_of(&structural, &[**id]).len();
        mark(&mut covered, &structural, **id);
        stops.push(TourStop {
            file: f.id,
            path: f.path.clone(),
            loc: f.loc,
            reason: format!(
                "The entry point. Reading it shows what the program does on startup, \
                 and it reaches {reach} of the {} files here.",
                graph.files.len()
            ),
            unlocks: reach,
        });
    }

    while stops.len() < opts.max_stops {
        let mut best: Option<(f64, FileId, usize)> = None;

        for f in &graph.files {
            if covered.get(f.id.0 as usize).copied().unwrap_or(true) || f.loc < opts.min_loc {
                continue;
            }
            let dependents = deps.dependents(f.id);
            let fresh = dependents
                .iter()
                .filter(|d| !covered.get(d.0 as usize).copied().unwrap_or(true))
                .count();
            if fresh == 0 {
                continue;
            }

            let churn = history.get(&f.path).map(|h| h.commits).unwrap_or(0);
            let score = fresh as f64 * 3.0 + (f.loc as f64).sqrt() + (churn as f64).sqrt() * 1.5;

            if best.is_none_or(|(b, _, _)| score > b) {
                best = Some((score, f.id, fresh));
            }
        }

        let Some((_, id, fresh)) = best else { break };
        let Some(f) = graph.file(id) else { break };

        let churn = history.get(&f.path).map(|h| h.commits).unwrap_or(0);
        let mut reason = if fresh == 1 {
            "One file still unexplained depends on this one".to_string()
        } else {
            format!("{fresh} files still unexplained depend on this one")
        };
        if churn >= 5 {
            reason.push_str(&format!(", and it has changed {churn} times recently"));
        }
        reason.push('.');

        mark(&mut covered, &structural, id);
        stops.push(TourStop {
            file: f.id,
            path: f.path.clone(),
            loc: f.loc,
            reason,
            unlocks: fresh,
        });
    }

    if stops.len() < 3 {
        let already: Vec<FileId> = stops.iter().map(|s| s.file).collect();
        let mut ranked: Vec<(u64, usize, &crate::model::FileNode)> = graph
            .files
            .iter()
            .filter(|f| !already.contains(&f.id) && f.loc > 0)
            .map(|f| {
                let fan_in = deps.fan_in(f.id);
                (f.loc as u64 * (1 + fan_in as u64), fan_in, f)
            })
            .filter(|(weight, _, _)| *weight > 0)
            .collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.path.cmp(&b.2.path)));

        for (_, fan_in, f) in ranked.into_iter().take(opts.max_stops - stops.len()) {
            stops.push(TourStop {
                file: f.id,
                path: f.path.clone(),
                loc: f.loc,
                reason: if fan_in > 0 {
                    format!(
                        "Nothing here stands out as an obvious next step, so this is \
                         where the substance is: {}, imported by {fan_in}.",
                        lines(f.loc)
                    )
                } else {
                    format!(
                        "Nothing here stands out as an obvious next step, so this is \
                         simply one of the larger files, at {}.",
                        lines(f.loc)
                    )
                },
                unlocks: fan_in,
            });
        }
    }

    stops
}

fn lines(n: u32) -> String {
    if n == 1 {
        "1 line".to_string()
    } else {
        format!("{n} lines")
    }
}

fn mark(covered: &mut [bool], structural: &Adjacency, id: FileId) {
    if let Some(slot) = covered.get_mut(id.0 as usize) {
        *slot = true;
    }
    for d in structural.dependencies(id) {
        if let Some(slot) = covered.get_mut(d.0 as usize) {
            *slot = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Edge, EdgeKind, FileNode, Language, ModuleId};

    fn file(id: u32, path: &str, loc: u32) -> FileNode {
        FileNode {
            id: FileId(id),
            path: path.into(),
            module: ModuleId(0),
            language: Language::TypeScript,
            loc,
            bytes: loc as u64 * 30,
            content_hash: String::new(),
        }
    }

    fn edge(from: u32, to: u32) -> Edge {
        Edge {
            from: FileId(from),
            to: FileId(to),
            kind: EdgeKind::Import,
            weight: 1.0,
        }
    }

    #[test]
    fn an_empty_repository_has_no_tour() {
        let stops = build(
            &Graph::default(),
            &[],
            &HashMap::new(),
            &TourOptions::default(),
        );
        assert!(stops.is_empty());
    }

    #[test]
    fn the_entry_point_comes_first() {
        let graph = Graph {
            files: vec![
                file(0, "src/main.ts", 120),
                file(1, "src/core.ts", 300),
                file(2, "src/util.ts", 80),
            ],
            edges: vec![edge(0, 1), edge(1, 2)],
            ..Default::default()
        };
        let stops = build(
            &graph,
            &[FileId(0)],
            &HashMap::new(),
            &TourOptions::default(),
        );
        assert_eq!(stops[0].path, "src/main.ts");
        assert!(
            stops[0].reason.contains("entry point"),
            "{}",
            stops[0].reason
        );
    }

    #[test]
    fn a_tiny_file_is_never_a_stop_however_popular() {
        let mut files = vec![file(0, "src/tiny.ts", 4)];
        let mut edges = Vec::new();
        for i in 1..12u32 {
            files.push(file(i, &format!("src/f{i}.ts"), 200));
            edges.push(edge(i, 0));
        }
        let graph = Graph {
            files,
            edges,
            ..Default::default()
        };
        let stops = build(&graph, &[], &HashMap::new(), &TourOptions::default());

        assert_ne!(stops.first().map(|s| s.path.as_str()), Some("src/tiny.ts"));
    }

    #[test]
    fn the_tour_prefers_files_that_explain_the_most() {
        let mut files = vec![file(0, "src/hub.ts", 300), file(1, "src/lonely.ts", 900)];
        let mut edges = Vec::new();
        for i in 2..12u32 {
            files.push(file(i, &format!("src/leaf{i}.ts"), 60));
            edges.push(edge(i, 0));
        }
        let graph = Graph {
            files,
            edges,
            ..Default::default()
        };
        let stops = build(&graph, &[], &HashMap::new(), &TourOptions::default());
        assert_eq!(stops[0].path, "src/hub.ts");
        assert!(stops[0].unlocks >= 10);
    }

    #[test]
    fn the_tour_spreads_out_instead_of_walking_one_chain() {
        let mut files = Vec::new();
        let mut edges = Vec::new();

        for cluster in 0..2u32 {
            let hub = cluster * 8;
            files.push(file(hub, &format!("src/c{cluster}/hub.ts"), 300));
            for i in 1..8u32 {
                let id = hub + i;
                files.push(file(id, &format!("src/c{cluster}/f{i}.ts"), 90));
                edges.push(edge(id, hub));
            }
        }
        let graph = Graph {
            files,
            edges,
            ..Default::default()
        };
        let stops = build(&graph, &[], &HashMap::new(), &TourOptions::default());
        let paths: Vec<&str> = stops.iter().map(|s| s.path.as_str()).collect();
        assert!(paths.iter().any(|p| p.contains("c0/hub")), "{paths:?}");
        assert!(paths.iter().any(|p| p.contains("c1/hub")), "{paths:?}");
    }

    #[test]
    fn no_file_is_recommended_twice() {
        let mut files = vec![file(0, "src/hub.ts", 400)];
        let mut edges = Vec::new();
        for i in 1..30u32 {
            files.push(file(i, &format!("src/f{i}.ts"), 100));
            edges.push(edge(i, 0));
        }
        let graph = Graph {
            files,
            edges,
            ..Default::default()
        };
        let stops = build(&graph, &[], &HashMap::new(), &TourOptions::default());
        let mut seen: Vec<u32> = stops.iter().map(|s| s.file.0).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), before);
    }

    #[test]
    fn the_tour_is_capped() {
        let mut files = Vec::new();
        let mut edges = Vec::new();
        for i in 0..60u32 {
            files.push(file(i, &format!("src/f{i}.ts"), 200));
            if i > 0 {
                edges.push(edge(i, 0));
            }
        }
        let graph = Graph {
            files,
            edges,
            ..Default::default()
        };
        let opts = TourOptions {
            max_stops: 4,
            ..Default::default()
        };
        let stops = build(&graph, &[], &HashMap::new(), &opts);
        assert!(stops.len() <= 4);
    }

    #[test]
    fn churn_lifts_a_file_a_newcomer_will_have_to_touch() {
        let graph = Graph {
            files: vec![
                file(0, "src/steady.ts", 200),
                file(1, "src/busy.ts", 200),
                file(2, "src/a.ts", 100),
                file(3, "src/b.ts", 100),
                file(4, "src/c.ts", 100),
                file(5, "src/d.ts", 100),
            ],
            edges: vec![edge(2, 0), edge(3, 0), edge(4, 1), edge(5, 1)],
            ..Default::default()
        };
        let mut history = HashMap::new();
        history.insert(
            "src/busy.ts".to_string(),
            FileHistory {
                commits: 80,
                ..Default::default()
            },
        );
        let stops = build(&graph, &[], &history, &TourOptions::default());
        assert_eq!(stops[0].path, "src/busy.ts", "got {stops:#?}");
    }

    #[test]
    fn a_conventional_entry_point_beats_a_bigger_one() {
        let graph = Graph {
            files: vec![
                file(0, "src/index.ts", 900),
                file(1, "src/main.tsx", 60),
                file(2, "src/core.ts", 400),
            ],
            edges: vec![edge(1, 2), edge(0, 2)],
            ..Default::default()
        };
        let stops = build(
            &graph,
            &[FileId(0), FileId(1)],
            &HashMap::new(),
            &TourOptions::default(),
        );
        assert_eq!(stops[0].path, "src/main.tsx", "got {stops:#?}");
    }

    #[test]
    fn the_reason_reads_correctly_for_a_single_dependent() {
        let graph = Graph {
            files: vec![file(0, "src/hub.ts", 300), file(1, "src/one.ts", 100)],
            edges: vec![edge(1, 0)],
            ..Default::default()
        };
        let stops = build(&graph, &[], &HashMap::new(), &TourOptions::default());
        assert!(
            stops.iter().all(|s| !s.reason.contains("1 files")),
            "got {stops:#?}"
        );
    }

    #[test]
    fn every_stop_explains_itself() {
        let mut files = vec![file(0, "src/hub.ts", 300)];
        let mut edges = Vec::new();
        for i in 1..10u32 {
            files.push(file(i, &format!("src/f{i}.ts"), 120));
            edges.push(edge(i, 0));
        }
        let graph = Graph {
            files,
            edges,
            ..Default::default()
        };
        let stops = build(
            &graph,
            &[FileId(1)],
            &HashMap::new(),
            &TourOptions::default(),
        );
        assert!(!stops.is_empty());
        for s in &stops {
            assert!(!s.reason.trim().is_empty(), "{s:?}");
            assert!(s.reason.ends_with('.'), "{}", s.reason);
        }
    }

    #[test]
    fn an_entry_point_does_not_explain_the_whole_repository_in_one_stop() {
        let mut files = vec![file(0, "src/main.tsx", 60)];
        let mut edges = Vec::new();
        for i in 1..12u32 {
            files.push(file(i, &format!("src/f{i}.ts"), 150));
            edges.push(edge(i - 1, i));
        }
        let graph = Graph {
            files,
            edges,
            ..Default::default()
        };
        let stops = build(
            &graph,
            &[FileId(0)],
            &HashMap::new(),
            &TourOptions::default(),
        );
        assert!(
            stops.len() >= 4,
            "expected a real reading order, got {stops:#?}"
        );
    }

    #[test]
    fn a_repository_of_small_files_still_gets_a_tour() {
        let mut files = vec![file(0, "src/hub.ts", 8)];
        let mut edges = Vec::new();
        for i in 1..15u32 {
            files.push(file(i, &format!("src/f{i}.ts"), 6));
            edges.push(edge(i, 0));
        }
        let graph = Graph {
            files,
            edges,
            ..Default::default()
        };
        let stops = build(&graph, &[], &HashMap::new(), &TourOptions::default());
        assert!(!stops.is_empty(), "expected a fallback tour");
        assert_eq!(stops[0].path, "src/hub.ts");
        assert!(
            stops[0].reason.contains("Nothing here stands out"),
            "the fallback should say plainly that it fell back: {}",
            stops[0].reason
        );
    }

    #[test]
    fn an_empty_repository_still_has_no_tour() {
        assert!(build(
            &Graph::default(),
            &[],
            &HashMap::new(),
            &TourOptions::default()
        )
        .is_empty());
    }

    #[test]
    fn line_counts_read_correctly_when_singular() {
        assert_eq!(lines(1), "1 line");
        assert_eq!(lines(0), "0 lines");
        assert_eq!(lines(42), "42 lines");
    }

    #[test]
    fn the_tour_is_deterministic() {
        let mut files = Vec::new();
        let mut edges = Vec::new();
        for i in 0..25u32 {
            files.push(file(i, &format!("src/f{i}.ts"), 100 + i));
            if i > 0 {
                edges.push(edge(i, i % 5));
            }
        }
        let graph = Graph {
            files,
            edges,
            ..Default::default()
        };
        let a = build(&graph, &[], &HashMap::new(), &TourOptions::default());
        let b = build(&graph, &[], &HashMap::new(), &TourOptions::default());
        assert_eq!(a, b);
    }
}
