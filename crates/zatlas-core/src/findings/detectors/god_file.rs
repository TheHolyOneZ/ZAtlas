use crate::findings::{percentile, severity_for, Evidence, Finding, FindingKind, FindingsInput};

pub fn detect(input: &FindingsInput) -> Vec<Finding> {
    let files = &input.graph.files;
    if files.len() < 5 {
        return Vec::new();
    }

    let mut locs: Vec<f64> = files.iter().map(|f| f.loc as f64).collect();
    let mut fan_ins: Vec<f64> = files
        .iter()
        .map(|f| input.deps.fan_in(f.id) as f64)
        .collect();
    locs.sort_by(f64::total_cmp);
    fan_ins.sort_by(f64::total_cmp);

    let mut out = Vec::new();
    for f in files {
        let loc_p = percentile(&locs, f.loc as f64);
        let fan_in = input.deps.fan_in(f.id);
        let fan_p = percentile(&fan_ins, fan_in as f64);
        let churn = input.history.get(&f.path).map(|h| h.commits).unwrap_or(0);

        let churn_term = if input.history.is_empty() {
            0.6
        } else {
            let mut churns: Vec<f64> = files
                .iter()
                .map(|x| input.history.get(&x.path).map(|h| h.commits).unwrap_or(0) as f64)
                .collect();
            churns.sort_by(f64::total_cmp);
            percentile(&churns, churn as f64)
        };

        let score = loc_p * fan_p * churn_term;
        if score < 0.4 || f.loc < 200 {
            continue;
        }

        let mut evidence = vec![
            Evidence::new("Lines of code", f.loc),
            Evidence::new("Files depending on it", fan_in),
        ];
        if churn > 0 {
            evidence.push(Evidence::new("Commits in window", churn));
        }

        out.push(Finding {
            kind: FindingKind::GodFile,
            severity: severity_for(score),
            score,
            headline: format!("{} is {} lines with {} dependents", f.path, f.loc, fan_in),
            files: vec![f.id],
            evidence,
            why: "This file is simultaneously large, widely depended upon and \
                  frequently changed. Every change to it risks breaking many \
                  other files, and its size means changes are hard to review."
                .into(),
            how_to_fix: "Split it along the seams its dependents already imply: \
                         group the exports by which files use them, and move each \
                         group into its own module. Start with the group that has \
                         the fewest dependents."
                .into(),
        });
    }
    out
}
