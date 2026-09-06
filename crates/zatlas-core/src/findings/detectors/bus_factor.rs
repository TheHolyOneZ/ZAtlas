use crate::findings::{Evidence, Finding, FindingKind, FindingsInput, Severity};

pub fn detect(input: &FindingsInput) -> Vec<Finding> {
    if input.history.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for f in &input.graph.files {
        let Some(h) = input.history.get(&f.path) else {
            continue;
        };
        let fan_in = input.deps.fan_in(f.id);

        if h.authors != 1 || h.commits < 5 || fan_in < 5 {
            continue;
        }

        let distinct: std::collections::HashSet<&str> = input
            .history
            .values()
            .map(|x| x.top_author.as_str())
            .collect();
        if distinct.len() < 2 {
            continue;
        }

        let score = ((fan_in as f32 / 20.0).min(1.0) * 0.5 + 0.4).min(1.0);
        out.push(Finding {
            kind: FindingKind::BusFactor,
            severity: Severity::Medium,
            score,
            headline: format!(
                "{} has {fan_in} dependents and one author ({})",
                f.path, h.top_author
            ),
            files: vec![f.id],
            evidence: vec![
                Evidence::new("Authors", h.authors),
                Evidence::new("Sole author", &h.top_author),
                Evidence::new("Commits in window", h.commits),
                Evidence::new("Files depending on it", fan_in),
            ],
            why: "Knowledge of this file sits with one person, and a lot of the \
                  codebase depends on it. If they are unavailable, changes here \
                  become slow and risky for everyone else."
                .into(),
            how_to_fix: "Pair on the next change here, or ask for a walkthrough \
                         recorded as a comment at the top of the file. Reviewing \
                         one non-trivial change is usually enough to spread the \
                         essentials."
                .into(),
        });
    }
    out
}
