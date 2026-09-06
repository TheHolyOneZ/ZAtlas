use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

use crate::error::{CoreError, Result};
use crate::findings::{Finding, FindingKind, Severity};
use crate::model::Graph;

pub const FILE_NAME: &str = "zatlas-baseline.json";

pub const BASELINE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct BaselineEntry {
    pub id: String,
    pub kind: FindingKind,

    pub severity: Severity,

    pub headline: String,
    pub files: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Baseline {
    pub version: u32,

    pub created: String,
    pub entries: Vec<BaselineEntry>,
}

impl Baseline {
    pub fn contains(&self, id: &str) -> bool {
        self.entries.iter().any(|e| e.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub struct Comparison<'a> {
    pub fresh: Vec<&'a Finding>,

    pub accepted: Vec<&'a Finding>,

    pub fixed: Vec<&'a BaselineEntry>,
}

pub fn compare<'a>(
    findings: &'a [Finding],
    graph: &Graph,
    baseline: &'a Baseline,
) -> Comparison<'a> {
    let mut fresh = Vec::new();
    let mut accepted = Vec::new();
    let mut seen: Vec<String> = Vec::with_capacity(findings.len());

    for finding in findings {
        let id = fingerprint(finding, graph);
        if baseline.contains(&id) {
            accepted.push(finding);
        } else {
            fresh.push(finding);
        }
        seen.push(id);
    }

    let fixed = baseline
        .entries
        .iter()
        .filter(|e| !seen.contains(&e.id))
        .collect();

    Comparison {
        fresh,
        accepted,
        fixed,
    }
}

pub fn from_findings(findings: &[Finding], graph: &Graph, today: &str) -> Baseline {
    let mut entries: Vec<BaselineEntry> = findings
        .iter()
        .map(|f| BaselineEntry {
            id: fingerprint(f, graph),
            kind: f.kind,
            severity: f.severity,
            headline: f.headline.clone(),
            files: f
                .files
                .iter()
                .filter_map(|id| graph.file(*id).map(|n| n.path.clone()))
                .collect(),
            note: None,
        })
        .collect();

    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries.dedup_by(|a, b| a.id == b.id);

    Baseline {
        version: BASELINE_VERSION,
        created: today.to_owned(),
        entries,
    }
}

pub fn fingerprint(finding: &Finding, graph: &Graph) -> String {
    let mut paths: Vec<&str> = finding
        .files
        .iter()
        .filter_map(|id| graph.file(*id).map(|n| n.path.as_str()))
        .collect();

    paths.sort_unstable();

    let stripped: String = finding
        .headline
        .chars()
        .filter(|c| !c.is_ascii_digit())
        .collect();

    let mut hasher = blake3::Hasher::new();
    hasher.update(finding.kind.label().as_bytes());
    hasher.update(b"\0");
    hasher.update(paths.join("\0").as_bytes());
    hasher.update(b"\0");
    hasher.update(stripped.trim().as_bytes());

    hasher.finalize().to_hex()[..16].to_owned()
}

pub fn path_of(root: &Path) -> PathBuf {
    root.join(FILE_NAME)
}

pub fn load(root: &Path) -> Result<Option<Baseline>> {
    let path = path_of(root);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(CoreError::io(&path, source)),
    };
    let baseline: Baseline = serde_json::from_str(&text)
        .map_err(|e| CoreError::other(format!("{}: {e}", path.display())))?;

    if baseline.version > BASELINE_VERSION {
        return Err(CoreError::other(format!(
            "{} was written by a newer ZAtlas (version {}, this build understands {}). \
             Re-accept it rather than trusting ids this build cannot reproduce.",
            path.display(),
            baseline.version,
            BASELINE_VERSION
        )));
    }
    Ok(Some(baseline))
}

pub fn save(root: &Path, baseline: &Baseline) -> Result<()> {
    let path = path_of(root);

    let mut text = serde_json::to_string_pretty(baseline)
        .map_err(|e| CoreError::other(format!("encoding the baseline: {e}")))?;
    text.push('\n');
    std::fs::write(&path, text).map_err(|e| CoreError::io(&path, e))
}

pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::findings::Evidence;
    use crate::model::{FileId, FileNode, Language, ModuleId};

    fn graph_with(paths: &[&str]) -> Graph {
        let mut g = Graph::default();
        for (i, path) in paths.iter().enumerate() {
            g.files.push(FileNode {
                id: FileId(i as u32),
                path: (*path).to_owned(),
                module: ModuleId(0),
                language: Language::TypeScript,
                loc: 10,
                bytes: 100,
                content_hash: String::new(),
            });
        }
        g
    }

    fn finding(kind: FindingKind, headline: &str, files: Vec<u32>) -> Finding {
        Finding {
            kind,
            severity: Severity::Medium,
            score: 0.5,
            headline: headline.to_owned(),
            files: files.into_iter().map(FileId).collect(),
            evidence: vec![Evidence::new("Lines of code", 840)],
            why: "because".into(),
            how_to_fix: "do the thing".into(),
        }
    }

    #[test]
    fn the_same_finding_fingerprints_identically_twice() {
        let g = graph_with(&["a.ts"]);
        let f = finding(FindingKind::GodFile, "a.ts is 840 lines", vec![0]);
        assert_eq!(fingerprint(&f, &g), fingerprint(&f, &g));
    }

    #[test]
    fn a_finding_whose_numbers_moved_is_still_the_same_finding() {
        let g = graph_with(&["a.ts"]);
        let before = finding(
            FindingKind::GodFile,
            "a.ts is 840 lines with 8 users",
            vec![0],
        );
        let after = finding(
            FindingKind::GodFile,
            "a.ts is 1204 lines with 9 users",
            vec![0],
        );
        assert_eq!(fingerprint(&before, &g), fingerprint(&after, &g));
    }

    #[test]
    fn a_finding_about_a_different_file_is_a_different_finding() {
        let g = graph_with(&["a.ts", "b.ts"]);
        let a = finding(FindingKind::GodFile, "a.ts is 840 lines", vec![0]);
        let b = finding(FindingKind::GodFile, "b.ts is 840 lines", vec![1]);
        assert_ne!(fingerprint(&a, &g), fingerprint(&b, &g));
    }

    #[test]
    fn two_kinds_about_the_same_file_do_not_collide() {
        let g = graph_with(&["a.ts"]);
        let god = finding(FindingKind::GodFile, "a.ts is big", vec![0]);
        let orphan = finding(FindingKind::Orphan, "a.ts is big", vec![0]);
        assert_ne!(fingerprint(&god, &g), fingerprint(&orphan, &g));
    }

    #[test]
    fn a_cycle_reported_from_a_different_member_is_the_same_cycle() {
        let g = graph_with(&["a.ts", "b.ts"]);
        let one = Finding {
            files: vec![FileId(0), FileId(1)],
            ..finding(FindingKind::Cycle, "cycle between two files", vec![])
        };
        let other = Finding {
            files: vec![FileId(1), FileId(0)],
            ..finding(FindingKind::Cycle, "cycle between two files", vec![])
        };
        assert_eq!(fingerprint(&one, &g), fingerprint(&other, &g));
    }

    #[test]
    fn two_case_mismatches_in_one_file_are_told_apart_by_their_specifier() {
        let g = graph_with(&["a.ts"]);
        let one = finding(
            FindingKind::CaseMismatch,
            "a.ts:3 imports \"./Foo\"",
            vec![0],
        );
        let two = finding(
            FindingKind::CaseMismatch,
            "a.ts:9 imports \"./Bar\"",
            vec![0],
        );
        assert_ne!(fingerprint(&one, &g), fingerprint(&two, &g));
    }

    #[test]
    fn the_same_import_moving_down_the_file_stays_accepted() {
        let g = graph_with(&["a.ts"]);
        let before = finding(
            FindingKind::CaseMismatch,
            "a.ts:3 imports \"./Foo\"",
            vec![0],
        );
        let after = finding(
            FindingKind::CaseMismatch,
            "a.ts:41 imports \"./Foo\"",
            vec![0],
        );
        assert_eq!(fingerprint(&before, &g), fingerprint(&after, &g));
    }

    #[test]
    fn comparing_splits_new_from_accepted_and_names_what_was_fixed() {
        let g = graph_with(&["a.ts", "b.ts"]);
        let old = finding(FindingKind::GodFile, "a.ts is 840 lines", vec![0]);
        let baseline = from_findings(std::slice::from_ref(&old), &g, "2026-01-01");

        let fresh = finding(FindingKind::Orphan, "b.ts is never imported", vec![1]);
        let both = [old.clone(), fresh.clone()];
        let comparison = compare(&both, &g, &baseline);
        assert_eq!(comparison.accepted.len(), 1);
        assert_eq!(comparison.fresh.len(), 1);
        assert_eq!(comparison.fresh[0].headline, fresh.headline);
        assert!(comparison.fixed.is_empty());

        let only_fresh = [fresh];
        let comparison = compare(&only_fresh, &g, &baseline);
        assert_eq!(comparison.fixed.len(), 1);
        assert_eq!(comparison.fixed[0].headline, "a.ts is 840 lines");
    }

    #[test]
    fn a_baseline_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let g = graph_with(&["a.ts"]);
        let baseline = from_findings(
            &[finding(FindingKind::GodFile, "a.ts is 840 lines", vec![0])],
            &g,
            "2026-01-01",
        );
        assert!(load(dir.path()).unwrap().is_none(), "none before saving");
        save(dir.path(), &baseline).unwrap();
        assert_eq!(load(dir.path()).unwrap().as_ref(), Some(&baseline));
    }

    #[test]
    fn a_handwritten_note_survives_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let g = graph_with(&["a.ts"]);
        let mut baseline = from_findings(
            &[finding(FindingKind::GodFile, "a.ts is 840 lines", vec![0])],
            &g,
            "2026-01-01",
        );
        baseline.entries[0].note = Some("Split scheduled for Q2.".into());
        save(dir.path(), &baseline).unwrap();
        assert_eq!(
            load(dir.path()).unwrap().unwrap().entries[0]
                .note
                .as_deref(),
            Some("Split scheduled for Q2.")
        );
    }

    #[test]
    fn a_baseline_from_a_newer_zatlas_is_refused_rather_than_misread() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            path_of(dir.path()),
            r#"{"version": 99, "created": "2030-01-01", "entries": []}"#,
        )
        .unwrap();
        let err = load(dir.path()).unwrap_err().to_string();
        assert!(err.contains("newer ZAtlas"), "got: {err}");
    }

    #[test]
    fn a_corrupt_baseline_names_the_file_rather_than_just_the_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(path_of(dir.path()), "{ not json").unwrap();
        let err = load(dir.path()).unwrap_err().to_string();
        assert!(err.contains(FILE_NAME), "got: {err}");
    }

    #[test]
    fn duplicate_findings_are_stored_once() {
        let g = graph_with(&["a.ts"]);
        let f = finding(FindingKind::GodFile, "a.ts is 840 lines", vec![0]);
        let baseline = from_findings(&[f.clone(), f], &g, "2026-01-01");
        assert_eq!(baseline.entries.len(), 1);
    }

    #[test]
    fn entries_are_sorted_so_the_committed_file_diffs_cleanly() {
        let g = graph_with(&["a.ts", "b.ts", "c.ts"]);
        let baseline = from_findings(
            &[
                finding(FindingKind::Orphan, "c.ts unused", vec![2]),
                finding(FindingKind::Orphan, "a.ts unused", vec![0]),
                finding(FindingKind::Orphan, "b.ts unused", vec![1]),
            ],
            &g,
            "2026-01-01",
        );
        let ids: Vec<&str> = baseline.entries.iter().map(|e| e.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
    }
}
