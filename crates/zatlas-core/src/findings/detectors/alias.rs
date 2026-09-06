use crate::findings::{Evidence, Finding, FindingKind, FindingsInput, Severity};

pub fn detect(input: &FindingsInput) -> Vec<Finding> {
    input
        .graph
        .alias_audit
        .iter()
        .map(|a| {
            let broken = !a.resolves;
            Finding {
                kind: FindingKind::PathAlias,
                severity: if broken {
                    Severity::High
                } else {
                    Severity::Info
                },
                score: if broken { 0.87 } else { 0.2 },
                headline: if broken {
                    format!(
                        "{} maps \"{}\" to {}, which does not exist",
                        a.source,
                        a.pattern,
                        a.targets.first().map(String::as_str).unwrap_or("nothing")
                    )
                } else {
                    format!(
                        "{} declares \"{}\" and nothing imports through it",
                        a.source, a.pattern
                    )
                },

                files: Vec::new(),
                evidence: vec![
                    Evidence::new("Declared in", &a.source),
                    Evidence::new("Pattern", &a.pattern),
                    Evidence::new("Targets", a.targets.join(", ")),
                    Evidence::new("Import sites using it", a.used_by),
                ],
                why: if broken {
                    "This alias points at a path that is neither in the scan nor \
                     on disk. Any import written against it fails to resolve, and \
                     the error names the import rather than this line."
                        .into()
                } else {
                    "The alias resolves, but no import in the repository uses it. \
                     Every alias is a rule the next reader has to learn before \
                     they can follow an import, so one that buys nothing is a \
                     small ongoing tax."
                        .into()
                },
                how_to_fix: if broken {
                    format!(
                        "In {}, point \"{}\" at a path that exists, or delete it. If \
                         the target is generated and intentionally not committed, \
                         the alias is fine — add the directory to `include` in \
                         zatlas.toml so ZAtlas can see it.",
                        a.source, a.pattern
                    )
                } else {
                    format!("Delete \"{}\" from {}.", a.pattern, a.source)
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AliasAudit, Graph};

    fn run(audit: Vec<AliasAudit>) -> Vec<Finding> {
        let graph = Graph {
            alias_audit: audit,
            ..Default::default()
        };
        let deps = crate::graph::Adjacency::build(0, &[]);
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
        detect(&input)
    }

    #[test]
    fn an_alias_pointing_nowhere_is_a_high_severity_finding() {
        let found = run(vec![AliasAudit {
            pattern: "@app/*".into(),
            targets: vec!["src/app/*".into()],
            source: "tsconfig.json".into(),
            used_by: 3,
            resolves: false,
        }]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::High);
        assert!(found[0].headline.contains("@app/*"));
        assert!(found[0].headline.contains("tsconfig.json"));
    }

    #[test]
    fn an_unused_but_working_alias_is_only_noted() {
        let found = run(vec![AliasAudit {
            pattern: "@old/*".into(),
            targets: vec!["src/old/*".into()],
            source: "tsconfig.json".into(),
            used_by: 0,
            resolves: true,
        }]);
        assert_eq!(found[0].severity, Severity::Info);
        assert!(found[0].how_to_fix.starts_with("Delete"));
    }

    #[test]
    fn the_fix_for_a_broken_alias_mentions_the_generated_directory_case() {
        let found = run(vec![AliasAudit {
            pattern: "@gen/*".into(),
            targets: vec!["generated/*".into()],
            source: "tsconfig.json".into(),
            used_by: 1,
            resolves: false,
        }]);
        assert!(found[0].how_to_fix.contains("zatlas.toml"));
    }

    #[test]
    fn a_repository_with_no_aliases_produces_nothing() {
        assert!(run(Vec::new()).is_empty());
    }
}
