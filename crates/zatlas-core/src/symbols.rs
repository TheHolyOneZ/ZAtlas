use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use ts_rs::TS;

use crate::model::{FileId, Language};
use crate::parse::{node_line, node_text, DefKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SymbolNode {
    pub id: u32,
    pub name: String,
    pub kind: DefKind,
    pub file: FileId,
    pub path: String,
    pub line: u32,
    pub exported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SymbolEdge {
    pub from: u32,
    pub to: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SymbolGraph {
    pub nodes: Vec<SymbolNode>,
    pub edges: Vec<SymbolEdge>,

    pub external_refs: Vec<String>,
}

pub fn build(files: &[(FileId, String, Language, String)]) -> SymbolGraph {
    let mut out = SymbolGraph::default();

    let mut by_name: HashMap<String, Vec<u32>> = HashMap::new();
    for (file_id, path, language, source) in files {
        let parsed = crate::parse::parse(*language, source);
        for def in &parsed.definitions {
            if def.kind == DefKind::Impl || def.kind == DefKind::Module {
                continue;
            }
            let id = out.nodes.len() as u32;
            by_name.entry(def.name.clone()).or_default().push(id);
            out.nodes.push(SymbolNode {
                id,
                name: def.name.clone(),
                kind: def.kind,
                file: *file_id,
                path: path.clone(),
                line: def.line,
                exported: def.exported,
            });
        }
    }

    let ambiguous: HashSet<&str> = by_name
        .iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(name, _)| name.as_str())
        .collect();

    let mut edges: HashSet<SymbolEdge> = HashSet::new();
    let mut external: HashSet<String> = HashSet::new();
    let mut node_cursor = 0usize;

    for (_, _, language, source) in files {
        let parsed = crate::parse::parse(*language, source);
        let defs_here: Vec<(u32, u32, DefKind)> = parsed
            .definitions
            .iter()
            .filter(|d| d.kind != DefKind::Impl && d.kind != DefKind::Module)
            .map(|d| {
                let id = out.nodes[node_cursor].id;
                node_cursor += 1;
                (id, d.line, d.kind)
            })
            .collect();

        for (name, line) in references(*language, source) {
            let Some(&(owner, _, _)) = defs_here.iter().rfind(|(_, def_line, _)| *def_line <= line)
            else {
                continue;
            };

            if ambiguous.contains(name.as_str()) {
                continue;
            }
            match by_name.get(&name) {
                Some(ids) if ids.len() == 1 => {
                    let target = ids[0];
                    if target != owner {
                        edges.insert(SymbolEdge {
                            from: owner,
                            to: target,
                        });
                    }
                }
                _ => {
                    external.insert(name);
                }
            }
        }
    }

    out.edges = edges.into_iter().collect();
    out.edges.sort_by_key(|e| (e.from, e.to));

    out.external_refs = external.into_iter().collect();
    out.external_refs.sort();

    out.external_refs.truncate(50);

    out
}

fn references(language: Language, source: &str) -> Vec<(String, u32)> {
    let Some(mut parser) = crate::parse::grammar::parser_for(language) else {
        return Vec::new();
    };
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    let mut cursor = tree.walk();

    while let Some(node) = stack.pop() {
        let interesting = match node.kind() {
            "call_expression" | "call" => node.child_by_field_name("function"),

            "type_identifier" => Some(node),

            "new_expression" => node.child_by_field_name("constructor"),
            _ => None,
        };

        if let Some(target) = interesting {
            let text = node_text(target, source);
            if let Some(name) = last_segment(text) {
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    out.push((name.to_string(), node_line(node)));
                }
            }
        }

        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    out.sort_by_key(|(_, line)| *line);
    out
}

fn last_segment(text: &str) -> Option<&str> {
    text.rsplit(["::", "."].as_slice().first().copied().unwrap_or("::"))
        .next()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(files: &[(&str, &str)]) -> SymbolGraph {
        let owned: Vec<(FileId, String, Language, String)> = files
            .iter()
            .enumerate()
            .map(|(i, (path, src))| {
                (
                    FileId(i as u32),
                    (*path).to_string(),
                    Language::TypeScript,
                    (*src).to_string(),
                )
            })
            .collect();
        build(&owned)
    }

    fn rs(files: &[(&str, &str)]) -> SymbolGraph {
        let owned: Vec<(FileId, String, Language, String)> = files
            .iter()
            .enumerate()
            .map(|(i, (path, src))| {
                (
                    FileId(i as u32),
                    (*path).to_string(),
                    Language::Rust,
                    (*src).to_string(),
                )
            })
            .collect();
        build(&owned)
    }

    fn names(g: &SymbolGraph) -> Vec<&str> {
        g.nodes.iter().map(|n| n.name.as_str()).collect()
    }

    fn edge_names(g: &SymbolGraph) -> Vec<String> {
        let mut out: Vec<String> = g
            .edges
            .iter()
            .map(|e| {
                format!(
                    "{} -> {}",
                    g.nodes[e.from as usize].name, g.nodes[e.to as usize].name
                )
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn an_empty_selection_yields_an_empty_graph() {
        assert_eq!(build(&[]), SymbolGraph::default());
    }

    #[test]
    fn definitions_become_nodes() {
        let g = ts(&[(
            "a.ts",
            "export function alpha() {}\nexport class Beta {}\nconst gamma = () => {};",
        )]);
        let mut got = names(&g);
        got.sort_unstable();
        assert_eq!(got, ["Beta", "alpha", "gamma"]);
    }

    #[test]
    fn a_call_becomes_an_edge() {
        let g = ts(&[(
            "a.ts",
            "function helper() { return 1; }\nfunction main() { return helper(); }",
        )]);
        assert_eq!(edge_names(&g), ["main -> helper"]);
    }

    #[test]
    fn calls_across_files_in_the_selection_are_linked() {
        let g = ts(&[
            ("a.ts", "export function helper() { return 1; }"),
            (
                "b.ts",
                "import { helper } from './a';\nexport function main() { return helper(); }",
            ),
        ]);
        assert_eq!(edge_names(&g), ["main -> helper"]);
    }

    #[test]
    fn a_function_calling_itself_is_not_an_edge() {
        let g = ts(&[("a.ts", "function loop() { return loop(); }")]);
        assert!(g.edges.is_empty(), "{:?}", edge_names(&g));
    }

    #[test]
    fn an_ambiguous_name_is_dropped_rather_than_guessed_at() {
        let g = ts(&[
            ("a.ts", "export function render() {}"),
            ("b.ts", "export function render() {}"),
            ("c.ts", "export function draw() { render(); }"),
        ]);
        assert!(
            edge_names(&g).is_empty(),
            "guessing between duplicates draws wrong edges: {:?}",
            edge_names(&g)
        );
    }

    #[test]
    fn references_to_things_outside_the_selection_are_reported_not_dropped() {
        let g = ts(&[("a.ts", "function main() { console.log(externalThing()); }")]);
        assert!(
            g.external_refs.iter().any(|r| r == "externalThing"),
            "got {:?}",
            g.external_refs
        );
    }

    #[test]
    fn rust_calls_resolve_through_their_path() {
        let g = rs(&[(
            "a.rs",
            "pub fn compute() -> u32 { 1 }\npub fn run() -> u32 { helpers::compute() }",
        )]);
        assert_eq!(edge_names(&g), ["run -> compute"]);
    }

    #[test]
    fn rust_type_references_are_edges() {
        let g = rs(&[(
            "a.rs",
            "pub struct Config;\npub fn load() -> Config { Config }",
        )]);
        assert!(
            edge_names(&g).contains(&"load -> Config".to_string()),
            "got {:?}",
            edge_names(&g)
        );
    }

    #[test]
    fn impl_blocks_are_not_call_targets() {
        let g = rs(&[("a.rs", "pub struct S;\nimpl S { pub fn go() {} }")]);

        assert_eq!(
            names(&g).iter().filter(|n| **n == "S").count(),
            1,
            "got {:?}",
            names(&g)
        );
        assert_eq!(
            g.nodes.iter().find(|n| n.name == "S").map(|n| n.kind),
            Some(DefKind::Struct)
        );
        assert!(names(&g).contains(&"go"));
    }

    #[test]
    fn a_reference_before_any_definition_is_ignored() {
        let g = ts(&[("a.ts", "setup();\nfunction setup() {}")]);
        assert!(g.edges.is_empty(), "{:?}", edge_names(&g));
    }

    #[test]
    fn the_graph_is_deterministic() {
        let files = [
            ("a.ts", "export function a() { b(); }"),
            ("b.ts", "export function b() { c(); }"),
            ("c.ts", "export function c() {}"),
        ];
        assert_eq!(ts(&files), ts(&files));
    }

    #[test]
    fn external_references_are_capped() {
        let mut src = String::from("function main() {\n");
        for i in 0..200 {
            src.push_str(&format!("  ext{i}();\n"));
        }
        src.push('}');
        let g = ts(&[("a.ts", &src)]);
        assert!(g.external_refs.len() <= 50, "got {}", g.external_refs.len());
    }
}
