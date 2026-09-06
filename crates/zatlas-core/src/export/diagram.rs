use std::collections::{HashMap, HashSet};

use crate::graph::module_edges;
use crate::model::{EdgeKind, FileId, Graph};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramScope {
    Modules,

    Files,
}

pub struct DiagramOptions {
    pub scope: DiagramScope,

    pub only: Vec<FileId>,

    pub highlight_cycles: bool,

    pub max_nodes: usize,
}

impl Default for DiagramOptions {
    fn default() -> Self {
        Self {
            scope: DiagramScope::Modules,
            only: Vec::new(),
            highlight_cycles: true,
            max_nodes: 120,
        }
    }
}

fn ident(raw: &str) -> String {
    if raw.is_empty() {
        return "root".to_string();
    }
    let mut out: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();

    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, 'n');
    }
    out
}

fn label(raw: &str) -> String {
    if raw.is_empty() {
        "(root)".to_string()
    } else {
        raw.replace('"', "'")
    }
}

struct Prepared {
    nodes: Vec<(String, String, usize, u32)>,
    edges: Vec<(String, String, u32)>,
    truncated: usize,
}

fn prepare(graph: &Graph, opts: &DiagramOptions, cycle_files: &HashSet<FileId>) -> Prepared {
    let filter: Option<HashSet<FileId>> = if opts.only.is_empty() {
        None
    } else {
        Some(opts.only.iter().copied().collect())
    };

    match opts.scope {
        DiagramScope::Modules => {
            let mut keep: HashSet<u32> = HashSet::new();
            for f in &graph.files {
                if filter.as_ref().is_none_or(|s| s.contains(&f.id)) {
                    keep.insert(f.module.0);
                }
            }

            let mut nodes: Vec<(String, String, usize, u32)> = graph
                .modules
                .iter()
                .filter(|m| keep.contains(&m.id.0))
                .map(|m| {
                    let cycles = m.files.iter().filter(|f| cycle_files.contains(f)).count() as u32;
                    (ident(&m.path), label(&m.path), m.files.len(), cycles)
                })
                .collect();
            let truncated = nodes.len().saturating_sub(opts.max_nodes);

            nodes.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
            nodes.truncate(opts.max_nodes);
            let present: HashSet<&str> = nodes.iter().map(|n| n.0.as_str()).collect();

            let by_id: HashMap<u32, &str> = graph
                .modules
                .iter()
                .map(|m| (m.id.0, m.path.as_str()))
                .collect();

            let mut edges: Vec<(String, String, u32)> = module_edges(graph)
                .into_iter()
                .filter_map(|(from, to, n)| {
                    let f = ident(by_id.get(&from.0)?);
                    let t = ident(by_id.get(&to.0)?);
                    (present.contains(f.as_str()) && present.contains(t.as_str()))
                        .then_some((f, t, n))
                })
                .collect();
            edges.sort();
            nodes.sort_by(|a, b| a.0.cmp(&b.0));

            Prepared {
                nodes,
                edges,
                truncated,
            }
        }
        DiagramScope::Files => {
            let mut nodes: Vec<(String, String, usize, u32)> = graph
                .files
                .iter()
                .filter(|f| filter.as_ref().is_none_or(|s| s.contains(&f.id)))
                .map(|f| {
                    (
                        ident(&f.path),
                        label(&f.path),
                        f.loc as usize,
                        u32::from(cycle_files.contains(&f.id)),
                    )
                })
                .collect();
            let truncated = nodes.len().saturating_sub(opts.max_nodes);
            nodes.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
            nodes.truncate(opts.max_nodes);
            let present: HashSet<&str> = nodes.iter().map(|n| n.0.as_str()).collect();

            let mut edges: Vec<(String, String, u32)> = graph
                .edges
                .iter()
                .filter(|e| e.kind != EdgeKind::CoChange)
                .filter_map(|e| {
                    let f = ident(&graph.file(e.from)?.path);
                    let t = ident(&graph.file(e.to)?.path);
                    (present.contains(f.as_str()) && present.contains(t.as_str()))
                        .then_some((f, t, 1))
                })
                .collect();
            edges.sort();
            edges.dedup();
            nodes.sort_by(|a, b| a.0.cmp(&b.0));

            Prepared {
                nodes,
                edges,
                truncated,
            }
        }
    }
}

pub fn to_d2(graph: &Graph, opts: &DiagramOptions, cycle_files: &HashSet<FileId>) -> String {
    let p = prepare(graph, opts, cycle_files);
    let mut out = String::new();

    out.push_str("# Generated by ZAtlas\n");
    out.push_str("direction: right\n\n");

    for (id, text, size, cycles) in &p.nodes {
        out.push_str(&format!("{id}: \"{text}\" {{\n"));
        if opts.highlight_cycles && *cycles > 0 {
            out.push_str("  style.stroke: \"#f87171\"\n  style.stroke-width: 2\n");
        }
        out.push_str(&format!("  # {size}\n}}\n"));
    }
    if !p.nodes.is_empty() {
        out.push('\n');
    }

    for (from, to, weight) in &p.edges {
        if *weight > 1 {
            out.push_str(&format!("{from} -> {to}: {weight}\n"));
        } else {
            out.push_str(&format!("{from} -> {to}\n"));
        }
    }

    if p.truncated > 0 {
        out.push_str(&format!(
            "\n# {} more nodes omitted to keep the diagram readable\n",
            p.truncated
        ));
    }
    out
}

pub fn to_mermaid(graph: &Graph, opts: &DiagramOptions, cycle_files: &HashSet<FileId>) -> String {
    let p = prepare(graph, opts, cycle_files);
    let mut out = String::new();

    out.push_str("%% Generated by ZAtlas\n");
    out.push_str("graph LR\n");

    for (id, text, _, _) in &p.nodes {
        out.push_str(&format!("  {id}[\"{text}\"]\n"));
    }
    for (from, to, weight) in &p.edges {
        if *weight > 1 {
            out.push_str(&format!("  {from} -->|{weight}| {to}\n"));
        } else {
            out.push_str(&format!("  {from} --> {to}\n"));
        }
    }

    if opts.highlight_cycles {
        let in_cycle: Vec<&str> = p
            .nodes
            .iter()
            .filter(|n| n.3 > 0)
            .map(|n| n.0.as_str())
            .collect();
        if !in_cycle.is_empty() {
            out.push_str("  classDef cycle stroke:#f87171,stroke-width:2px\n");
            out.push_str(&format!("  class {},cycle\n", in_cycle.join(",")));
        }
    }

    if p.truncated > 0 {
        out.push_str(&format!(
            "  %% {} more nodes omitted to keep the diagram readable\n",
            p.truncated
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Edge, FileNode, Language, ModuleId, ModuleNode};

    fn graph() -> Graph {
        let file = |id: u32, module: u32, path: &str, loc: u32| FileNode {
            id: FileId(id),
            path: path.into(),
            module: ModuleId(module),
            language: Language::TypeScript,
            loc,
            bytes: 100,
            content_hash: String::new(),
        };
        Graph {
            files: vec![
                file(0, 0, "src/ui/a.ts", 100),
                file(1, 0, "src/ui/b.ts", 50),
                file(2, 1, "src/data/c.ts", 200),
            ],
            modules: vec![
                ModuleNode {
                    id: ModuleId(0),
                    path: "src/ui".into(),
                    files: vec![FileId(0), FileId(1)],
                },
                ModuleNode {
                    id: ModuleId(1),
                    path: "src/data".into(),
                    files: vec![FileId(2)],
                },
            ],
            edges: vec![
                Edge {
                    from: FileId(0),
                    to: FileId(2),
                    kind: EdgeKind::Import,
                    weight: 1.0,
                },
                Edge {
                    from: FileId(1),
                    to: FileId(2),
                    kind: EdgeKind::Import,
                    weight: 1.0,
                },
            ],
            ..Default::default()
        }
    }

    fn empty_cycles() -> HashSet<FileId> {
        HashSet::new()
    }

    #[test]
    fn identifiers_are_safe_for_both_formats() {
        assert_eq!(ident("src/ui/a.ts"), "src_ui_a_ts");
        assert_eq!(ident("my-crate"), "my_crate");
        assert_eq!(ident(""), "root");
        assert!(!ident("2fast").starts_with(|c: char| c.is_ascii_digit()));
    }

    #[test]
    fn d2_emits_one_node_per_module_and_the_edges_between_them() {
        let d2 = to_d2(&graph(), &DiagramOptions::default(), &empty_cycles());
        assert!(d2.contains("src_ui: \"src/ui\""), "{d2}");
        assert!(d2.contains("src_data: \"src/data\""), "{d2}");

        assert!(d2.contains("src_ui -> src_data: 2"), "{d2}");
    }

    #[test]
    fn mermaid_emits_the_same_topology() {
        let m = to_mermaid(&graph(), &DiagramOptions::default(), &empty_cycles());
        assert!(m.starts_with("%% Generated by ZAtlas"));
        assert!(m.contains("graph LR"));
        assert!(m.contains("src_ui[\"src/ui\"]"), "{m}");
        assert!(m.contains("src_ui -->|2| src_data"), "{m}");
    }

    #[test]
    fn file_scope_emits_one_node_per_file() {
        let opts = DiagramOptions {
            scope: DiagramScope::Files,
            ..Default::default()
        };
        let d2 = to_d2(&graph(), &opts, &empty_cycles());
        assert!(d2.contains("src_ui_a_ts"), "{d2}");
        assert!(d2.contains("src_ui_a_ts -> src_data_c_ts"), "{d2}");
    }

    #[test]
    fn a_subgraph_export_includes_only_the_selected_files() {
        let opts = DiagramOptions {
            scope: DiagramScope::Files,
            only: vec![FileId(0), FileId(2)],
            ..Default::default()
        };
        let d2 = to_d2(&graph(), &opts, &empty_cycles());
        assert!(d2.contains("src_ui_a_ts"));
        assert!(!d2.contains("src_ui_b_ts"), "b was not selected: {d2}");
    }

    #[test]
    fn cycles_are_marked_in_both_formats() {
        let cycles: HashSet<FileId> = [FileId(0)].into_iter().collect();
        let d2 = to_d2(&graph(), &DiagramOptions::default(), &cycles);
        assert!(d2.contains("#f87171"), "{d2}");
        let m = to_mermaid(&graph(), &DiagramOptions::default(), &cycles);
        assert!(m.contains("classDef cycle"), "{m}");
    }

    #[test]
    fn a_huge_graph_is_truncated_rather_than_emitted_unreadable() {
        let mut g = Graph::default();
        for i in 0..500u32 {
            g.files.push(FileNode {
                id: FileId(i),
                path: format!("m{i}/f.ts"),
                module: ModuleId(i),
                language: Language::TypeScript,
                loc: 10,
                bytes: 10,
                content_hash: String::new(),
            });
            g.modules.push(ModuleNode {
                id: ModuleId(i),
                path: format!("m{i}"),
                files: vec![FileId(i)],
            });
        }
        let d2 = to_d2(&g, &DiagramOptions::default(), &empty_cycles());
        assert!(
            d2.contains("more nodes omitted"),
            "{}",
            &d2[..200.min(d2.len())]
        );
        assert!(d2.matches(" -> ").count() < 500);
    }

    #[test]
    fn output_is_byte_identical_across_runs() {
        let g = graph();
        let opts = DiagramOptions::default();
        assert_eq!(
            to_d2(&g, &opts, &empty_cycles()),
            to_d2(&g, &opts, &empty_cycles())
        );
        assert_eq!(
            to_mermaid(&g, &opts, &empty_cycles()),
            to_mermaid(&g, &opts, &empty_cycles())
        );
    }

    #[test]
    fn an_empty_graph_still_produces_valid_source() {
        let g = Graph::default();
        let d2 = to_d2(&g, &DiagramOptions::default(), &empty_cycles());
        assert!(d2.contains("direction: right"));
        let m = to_mermaid(&g, &DiagramOptions::default(), &empty_cycles());
        assert!(m.contains("graph LR"));
    }

    #[test]
    fn a_quote_in_a_path_does_not_break_the_label() {
        let mut g = graph();
        g.modules[0].path = "src/we\"ird".into();
        let d2 = to_d2(&g, &DiagramOptions::default(), &empty_cycles());
        assert!(
            !d2.contains("we\"ird"),
            "an unescaped quote breaks parsing: {d2}"
        );
    }
}
