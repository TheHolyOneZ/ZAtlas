use crate::findings::{Evidence, Finding, FindingKind, FindingsInput, Severity};

pub fn detect(input: &FindingsInput) -> Vec<Finding> {
    input
        .graph
        .lsp_disagreements
        .iter()
        .filter_map(|d| {
            let from = input.graph.file(d.from)?;
            let target = input.graph.file(d.lsp_target)?;
            Some(Finding {
                kind: FindingKind::LspDisagreement,
                severity: Severity::High,
                score: 0.88,
                headline: format!(
                    "{}:{} — {} resolves \"{}\" to {}",
                    from.path, d.line, d.server, d.specifier, target.path
                ),
                files: vec![d.from, d.lsp_target],
                evidence: vec![
                    Evidence::new("Import", &d.specifier),
                    Evidence::new("Language server says", &target.path),
                    Evidence::new("Reported by", &d.server),
                    Evidence::new("Line", d.line),
                ],
                why: "The heuristic resolver and the language server disagree \
                      about where this import goes, so at least one edge on the \
                      map is wrong. The server sees the same build configuration \
                      the compiler does, which makes it the better authority for \
                      an ambiguous specifier."
                    .into(),
                how_to_fix: format!(
                    "Open {}:{} and check where \"{}\" really resolves. If the \
                     server is right, this is a ZAtlas resolver bug worth \
                     reporting with the specifier and the project layout. If the \
                     specifier is genuinely ambiguous — two path aliases that \
                     both match, say — tightening it removes the ambiguity for \
                     every tool that reads this repository, not just this one.",
                    from.path, d.line, d.specifier
                ),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FileId, LspDisagreement};

    fn input_with(disagreements: Vec<LspDisagreement>) -> crate::model::Graph {
        let mut g = crate::model::Graph::default();
        for (i, path) in ["a.ts", "b.ts"].iter().enumerate() {
            g.files.push(crate::model::FileNode {
                id: FileId(i as u32),
                path: (*path).to_owned(),
                module: crate::model::ModuleId(0),
                language: crate::model::Language::TypeScript,
                loc: 10,
                bytes: 100,
                content_hash: String::new(),
            });
        }
        g.lsp_disagreements = disagreements;
        g
    }

    #[test]
    fn a_disagreement_names_both_files_and_the_server() {
        let graph = input_with(vec![LspDisagreement {
            from: FileId(0),
            specifier: "./thing".into(),
            line: 3,
            lsp_target: FileId(1),
            server: "tsserver".into(),
        }]);
        let deps = crate::graph::Adjacency::build(graph.files.len(), &graph.edges);
        let config = crate::config::ZatlasConfig::default();
        let history = std::collections::HashMap::new();
        let input = FindingsInput {
            graph: &graph,
            deps: &deps,
            structural: &deps,
            config: &config,
            history: &history,
            coupling: &[],
            entrypoints: Vec::new(),
        };
        let found = detect(&input);
        assert_eq!(found.len(), 1);
        assert!(found[0].headline.contains("a.ts:3"));
        assert!(found[0].headline.contains("b.ts"));
        assert!(found[0].headline.contains("tsserver"));
        assert_eq!(found[0].files, vec![FileId(0), FileId(1)]);
    }

    #[test]
    fn a_scan_without_the_lsp_pass_produces_no_rows() {
        let graph = input_with(Vec::new());
        let deps = crate::graph::Adjacency::build(graph.files.len(), &graph.edges);
        let config = crate::config::ZatlasConfig::default();
        let history = std::collections::HashMap::new();
        let input = FindingsInput {
            graph: &graph,
            deps: &deps,
            structural: &deps,
            config: &config,
            history: &history,
            coupling: &[],
            entrypoints: Vec::new(),
        };
        assert!(detect(&input).is_empty());
    }
}
