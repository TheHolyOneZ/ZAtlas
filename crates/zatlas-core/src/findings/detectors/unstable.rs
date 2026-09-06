use crate::findings::{percentile, severity_for, Evidence, Finding, FindingKind, FindingsInput};

pub fn detect(input: &FindingsInput) -> Vec<Finding> {
    if input.history.is_empty() || input.graph.files.len() < 5 {
        return Vec::new();
    }

    let files = &input.graph.files;
    let mut fan_ins: Vec<f64> = files
        .iter()
        .map(|f| input.deps.fan_in(f.id) as f64)
        .collect();
    let mut churns: Vec<f64> = files
        .iter()
        .map(|f| input.history.get(&f.path).map(|h| h.commits).unwrap_or(0) as f64)
        .collect();
    fan_ins.sort_by(f64::total_cmp);
    churns.sort_by(f64::total_cmp);

    let mut out = Vec::new();
    for f in files {
        let fan_in = input.deps.fan_in(f.id);
        let Some(h) = input.history.get(&f.path) else {
            continue;
        };
        if fan_in < 3 || h.commits < 3 {
            continue;
        }

        let score = percentile(&fan_ins, fan_in as f64) * percentile(&churns, h.commits as f64);
        if score < 0.6 {
            continue;
        }

        out.push(Finding {
            kind: FindingKind::UnstableInterface,
            severity: severity_for(score),
            score,
            headline: format!(
                "{} has {fan_in} dependents and changed {} times",
                f.path, h.commits
            ),
            files: vec![f.id],
            evidence: vec![
                Evidence::new("Files depending on it", fan_in),
                Evidence::new("Commits in window", h.commits),
                Evidence::new("Lines of code", f.loc),
            ],
            why: "Many files depend on this one, and it keeps changing. Every \
                  change has a wide blast radius, which is why this file is \
                  likely to be behind unrelated breakages."
                .into(),
            how_to_fix: "Separate the stable part from the volatile part. Publish \
                         a narrow interface that dependents import, and let the \
                         churn happen behind it where it affects nobody."
                .into(),
        });
    }
    out
}
