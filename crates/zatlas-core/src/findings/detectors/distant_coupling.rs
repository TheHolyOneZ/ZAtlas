use crate::findings::{Evidence, Finding, FindingKind, FindingsInput, Severity};

pub fn detect(input: &FindingsInput) -> Vec<Finding> {
    let id_of = |path: &str| input.graph.files.iter().find(|f| f.path == path);

    input
        .coupling
        .iter()
        .filter_map(|pair| {
            let a = id_of(&pair.a)?;
            let b = id_of(&pair.b)?;

            let linked = input.deps.dependencies(a.id).contains(&b.id)
                || input.deps.dependencies(b.id).contains(&a.id);
            if linked {
                return None;
            }

            if a.module == b.module {
                return None;
            }
            if pair.confidence < 0.6 {
                return None;
            }

            let score = pair.confidence.min(1.0) * 0.8;
            Some(Finding {
                kind: FindingKind::DistantCoupling,
                severity: Severity::Medium,
                score,
                headline: format!(
                    "{} and {} change together {:.0}% of the time but are unrelated in code",
                    pair.a,
                    pair.b,
                    pair.confidence * 100.0
                ),
                files: vec![a.id, b.id],
                evidence: vec![
                    Evidence::new("Commits together", pair.together),
                    Evidence::new("Confidence", format!("{:.0}%", pair.confidence * 100.0)),
                    Evidence::new(
                        "Modules",
                        format!("{} and {}", module_of(input, a.id), module_of(input, b.id)),
                    ),
                ],
                why: "These files are edited together almost every time, yet \
                      neither imports the other and they live in different \
                      modules. That is a real dependency the code does not \
                      express, so it is easy to change one and forget the other."
                    .into(),
                how_to_fix: "Find the thing they share — a format, a constant, an \
                             assumption about ordering — and give it one home that \
                             both import. If they genuinely belong together, move \
                             them into the same module."
                    .into(),
            })
        })
        .collect()
}

fn module_of(input: &FindingsInput, id: crate::model::FileId) -> String {
    input
        .graph
        .file(id)
        .and_then(|f| input.graph.module(f.module))
        .map(|m| {
            if m.path.is_empty() {
                "(root)".to_string()
            } else {
                m.path.clone()
            }
        })
        .unwrap_or_default()
}
