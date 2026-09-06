use crate::findings::{Evidence, Finding, FindingKind, FindingsInput, Severity};
use crate::graph::tangles;

pub fn detect(input: &FindingsInput) -> Vec<Finding> {
    tangles(input.deps)
        .into_iter()
        .map(|t| {
            let path = |id| input.graph.file(id).map(|f| f.path.as_str()).unwrap_or("?");
            let chain: Vec<&str> = t.example.iter().map(|&id| path(id)).collect();
            let n = t.members.len();

            let score = ((n as f32 - 2.0) / 10.0).clamp(0.0, 1.0) * 0.5 + 0.5;

            Finding {
                kind: FindingKind::Cycle,
                severity: if n >= 6 {
                    Severity::High
                } else if n >= 3 {
                    Severity::Medium
                } else {
                    Severity::Low
                },
                score,
                headline: format!("Cycle across {n} files: {}", chain.join(" → ")),
                files: t.members.clone(),
                evidence: vec![
                    Evidence::new("Files in cycle", n),
                    Evidence::new("Shortest chain", chain.join(" → ")),
                ],
                why: "These files depend on each other in a loop, so none of them \
                      can be understood, tested or reused without the others. \
                      Cycles also make build and initialisation order fragile."
                    .into(),
                how_to_fix: "Find the one edge that feels most accidental and \
                             invert it: move the shared type into a module both \
                             sides can import, or pass a value in rather than \
                             importing it back. Breaking a single edge is enough \
                             to dissolve the whole cycle."
                    .into(),
            }
        })
        .collect()
}
