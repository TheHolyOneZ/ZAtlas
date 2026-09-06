use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

use super::history::CommitRecord;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct CoupledPair {
    pub a: String,
    pub b: String,

    pub together: u32,

    pub confidence: f32,
}

pub struct CouplingOptions {
    pub max_files_per_commit: usize,

    pub min_support: u32,

    pub min_confidence: f32,

    pub limit: usize,
}

impl Default for CouplingOptions {
    fn default() -> Self {
        Self {
            max_files_per_commit: 50,
            min_support: 5,
            min_confidence: 0.4,
            limit: 500,
        }
    }
}

pub fn coupling(commits: &[CommitRecord], opts: &CouplingOptions) -> Vec<CoupledPair> {
    let mut changes: HashMap<&str, u32> = HashMap::new();
    let mut together: HashMap<(&str, &str), u32> = HashMap::new();

    for commit in commits {
        if commit.files.len() > opts.max_files_per_commit {
            continue;
        }
        for f in &commit.files {
            *changes.entry(f.as_str()).or_insert(0) += 1;
        }

        for (i, a) in commit.files.iter().enumerate() {
            for b in commit.files.iter().skip(i + 1) {
                let key = if a < b {
                    (a.as_str(), b.as_str())
                } else {
                    (b.as_str(), a.as_str())
                };
                *together.entry(key).or_insert(0) += 1;
            }
        }
    }

    let mut out: Vec<CoupledPair> = together
        .into_iter()
        .filter(|(_, n)| *n >= opts.min_support)
        .filter_map(|((a, b), n)| {
            let ca = *changes.get(a)?;
            let cb = *changes.get(b)?;
            let denom = ca.min(cb).max(1);
            let confidence = n as f32 / denom as f32;
            (confidence >= opts.min_confidence).then(|| CoupledPair {
                a: a.to_string(),
                b: b.to_string(),
                together: n,
                confidence,
            })
        })
        .collect();

    out.sort_by(|x, y| {
        y.confidence
            .total_cmp(&x.confidence)
            .then_with(|| y.together.cmp(&x.together))
            .then_with(|| x.a.cmp(&y.a))
            .then_with(|| x.b.cmp(&y.b))
    });
    out.truncate(opts.limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(files: &[&str]) -> CommitRecord {
        CommitRecord {
            id: "abc1234".into(),
            time: 0,
            author: "Ada".into(),
            files: files.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn opts() -> CouplingOptions {
        CouplingOptions {
            min_support: 2,
            min_confidence: 0.4,
            ..Default::default()
        }
    }

    #[test]
    fn two_files_that_always_change_together_are_fully_coupled() {
        let commits = vec![commit(&["a.rs", "b.rs"]); 4];
        let got = coupling(&commits, &opts());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].a, "a.rs");
        assert_eq!(got[0].b, "b.rs");
        assert_eq!(got[0].together, 4);
        assert_eq!(got[0].confidence, 1.0);
    }

    #[test]
    fn files_that_never_share_a_commit_are_not_coupled() {
        let commits = vec![commit(&["a.rs"]), commit(&["b.rs"]), commit(&["a.rs"])];
        assert!(coupling(&commits, &opts()).is_empty());
    }

    #[test]
    fn confidence_uses_the_less_changed_file_as_the_denominator() {
        let mut commits = vec![commit(&["a.rs", "b.rs"]); 2];
        commits.extend(vec![commit(&["b.rs"]); 8]);
        let got = coupling(&commits, &opts());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].confidence, 1.0);
    }

    #[test]
    fn a_single_shared_commit_is_below_the_support_floor() {
        let commits = vec![commit(&["a.rs", "b.rs"])];
        assert!(coupling(&commits, &CouplingOptions::default()).is_empty());
    }

    #[test]
    fn a_sweeping_commit_is_skipped_entirely() {
        let wide: Vec<String> = (0..200).map(|i| format!("f{i}.rs")).collect();
        let refs: Vec<&str> = wide.iter().map(|s| s.as_str()).collect();
        let commits = vec![commit(&refs); 10];
        assert!(coupling(&commits, &opts()).is_empty());
    }

    #[test]
    fn a_wide_commit_does_not_blow_up_the_pair_count() {
        let wide: Vec<String> = (0..200).map(|i| format!("f{i}.rs")).collect();
        let refs: Vec<&str> = wide.iter().map(|s| s.as_str()).collect();
        let mut commits = vec![commit(&refs); 50];
        commits.extend(vec![commit(&["a.rs", "b.rs"]); 5]);
        let got = coupling(&commits, &opts());
        assert_eq!(got.len(), 1, "only the narrow pair survives");
    }

    #[test]
    fn weak_coupling_is_filtered_out() {
        let mut commits = vec![commit(&["a.rs", "b.rs"]); 2];
        commits.extend(vec![commit(&["a.rs"]); 20]);
        commits.extend(vec![commit(&["b.rs"]); 20]);
        let strict = CouplingOptions {
            min_support: 2,
            min_confidence: 0.5,
            ..Default::default()
        };
        assert!(coupling(&commits, &strict).is_empty());
    }

    #[test]
    fn pairs_are_reported_in_one_canonical_order() {
        let commits = vec![commit(&["z.rs", "a.rs"]); 3];
        let got = coupling(&commits, &opts());
        assert_eq!(got[0].a, "a.rs");
        assert_eq!(got[0].b, "z.rs");
    }

    #[test]
    fn output_is_ordered_strongest_first_and_stable() {
        let mut commits = vec![commit(&["a.rs", "b.rs"]); 10];
        commits.extend(vec![commit(&["c.rs", "d.rs"]); 3]);
        commits.extend(vec![commit(&["c.rs"]); 3]);
        let got = coupling(&commits, &opts());
        assert_eq!(got[0].a, "a.rs");
        assert_eq!(coupling(&commits, &opts()), got);
    }

    #[test]
    fn the_limit_caps_the_result() {
        let mut commits = Vec::new();
        for i in 0..40 {
            let a = format!("a{i}.rs");
            let b = format!("b{i}.rs");
            for _ in 0..3 {
                commits.push(commit(&[&a, &b]));
            }
        }
        let capped = CouplingOptions {
            min_support: 2,
            limit: 10,
            ..Default::default()
        };
        assert_eq!(coupling(&commits, &capped).len(), 10);
    }

    #[test]
    fn three_files_in_one_commit_produce_three_pairs() {
        let commits = vec![commit(&["a.rs", "b.rs", "c.rs"]); 3];
        assert_eq!(coupling(&commits, &opts()).len(), 3);
    }
}
