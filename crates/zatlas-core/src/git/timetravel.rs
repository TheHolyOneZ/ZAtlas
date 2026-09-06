use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricFile {
    pub path: String,

    pub blob: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub commit: String,

    pub time: i64,

    pub label: String,
    pub file_count: usize,
    pub total_loc: u32,
    pub edge_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,

    pub grown: Vec<(String, i64)>,
    pub shrunk: Vec<(String, i64)>,
}

pub struct TimelineOptions {
    pub steps: usize,

    pub window_days: i64,
}

impl Default for TimelineOptions {
    fn default() -> Self {
        Self {
            steps: 12,
            window_days: 365,
        }
    }
}

pub fn pick_commits(
    root: &Path,
    opts: &TimelineOptions,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<(String, i64)>> {
    let repo = gix::open(root).map_err(|_| CoreError::NotAGitRepo(root.to_path_buf()))?;
    let Ok(head) = repo.head_id() else {
        return Ok(Vec::new());
    };

    let walk = head
        .ancestors()
        .all()
        .map_err(|e| CoreError::Git(e.to_string()))?;

    let mut commits: Vec<(String, i64, usize)> = Vec::new();
    let mut newest = 0i64;
    let mut cutoff = i64::MIN;

    for info in walk {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(info) = info else { continue };
        let Ok(commit) = repo.find_commit(info.id) else {
            continue;
        };
        let Ok(time) = commit.time() else { continue };
        if newest == 0 {
            newest = time.seconds;
            cutoff = newest - opts.window_days * 86_400;
        }
        if time.seconds < cutoff {
            break;
        }
        let position = commits.len();
        commits.push((info.id.to_hex().to_string(), time.seconds, position));
    }

    if commits.is_empty() {
        return Ok(Vec::new());
    }

    let oldest = commits.last().map(|c| c.1).unwrap_or(newest);
    let span = (newest - oldest).max(1);
    let steps = opts.steps.max(1);

    let mut chosen: Vec<(String, i64, usize)> = Vec::new();
    let mut seen_buckets: Vec<usize> = Vec::new();
    for entry in &commits {
        let bucket =
            (((newest - entry.1) as f64 / span as f64) * (steps - 1) as f64).round() as usize;
        if seen_buckets.contains(&bucket) {
            continue;
        }
        seen_buckets.push(bucket);
        chosen.push(entry.clone());
    }

    let want = steps.min(commits.len());
    if chosen.len() < want {
        let stride = (commits.len() as f64 / want as f64).max(1.0);
        for k in 0..want {
            let idx = ((k as f64) * stride).floor() as usize;
            let Some(candidate) = commits.get(idx) else {
                continue;
            };
            if chosen.iter().any(|c| c.0 == candidate.0) {
                continue;
            }
            chosen.push(candidate.clone());
            if chosen.len() >= want {
                break;
            }
        }
    }

    chosen.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| b.2.cmp(&a.2)));
    chosen.dedup_by(|a, b| a.0 == b.0);
    Ok(chosen.into_iter().map(|(id, time, _)| (id, time)).collect())
}

pub fn files_at(
    root: &Path,
    commit_id: &str,
    want: &dyn Fn(&str) -> bool,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<HistoricFile>> {
    let repo = gix::open(root).map_err(|_| CoreError::NotAGitRepo(root.to_path_buf()))?;
    let id = gix::ObjectId::from_hex(commit_id.as_bytes())
        .map_err(|e| CoreError::Git(format!("{commit_id}: {e}")))?;
    let commit = repo
        .find_commit(id)
        .map_err(|e| CoreError::Git(e.to_string()))?;
    let tree = commit.tree().map_err(|e| CoreError::Git(e.to_string()))?;

    let mut recorder = gix::traverse::tree::Recorder::default();
    tree.traverse()
        .breadthfirst(&mut recorder)
        .map_err(|e| CoreError::Git(e.to_string()))?;

    let mut out = Vec::new();
    for entry in recorder.records {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if !entry.mode.is_blob() {
            continue;
        }
        let path = entry.filepath.to_string();
        if !want(&path) {
            continue;
        }
        let Ok(object) = repo.find_object(entry.oid) else {
            continue;
        };
        out.push(HistoricFile {
            path,
            blob: entry.oid.to_hex().to_string(),
            bytes: object.data.clone(),
        });
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

pub fn month_label(unix_seconds: i64) -> String {
    label_for(unix_seconds, 365 * 86_400)
}

pub fn label_for(unix_seconds: i64, span_seconds: i64) -> String {
    use chrono::{TimeZone, Utc};
    let format = if span_seconds < 2 * 86_400 {
        "%H:%M"
    } else if span_seconds < 75 * 86_400 {
        "%m-%d"
    } else {
        "%Y-%m"
    };
    Utc.timestamp_opt(unix_seconds, 0)
        .single()
        .map(|dt| dt.format(format).to_string())
        .unwrap_or_else(|| "?".to_string())
}

pub fn diff(before: &HashMap<String, u32>, after: &HashMap<String, u32>) -> SnapshotDiff {
    let mut out = SnapshotDiff::default();

    for (path, loc) in after {
        match before.get(path) {
            None => out.added.push(path.clone()),
            Some(old) => {
                let delta = *loc as i64 - *old as i64;
                if delta > 0 {
                    out.grown.push((path.clone(), delta));
                } else if delta < 0 {
                    out.shrunk.push((path.clone(), delta));
                }
            }
        }
    }
    for path in before.keys() {
        if !after.contains_key(path) {
            out.removed.push(path.clone());
        }
    }

    out.added.sort();
    out.removed.sort();

    out.grown
        .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out.shrunk
        .sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    struct Fixture {
        dir: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let f = Fixture { dir };
            f.git(&["init", "-q", "-b", "main"]);
            f.git(&["config", "user.email", "a@example.com"]);
            f.git(&["config", "user.name", "Ada"]);
            f
        }
        fn path(&self) -> &Path {
            self.dir.path()
        }
        fn git(&self, args: &[&str]) {
            let out = Command::new("git")
                .args(args)
                .current_dir(self.path())
                .output()
                .expect("git available");
            assert!(out.status.success(), "git {args:?}");
        }
        fn commit(&self, files: &[(&str, &str)]) {
            for (name, body) in files {
                let p = self.path().join(name);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, body).unwrap();
            }
            self.git(&["add", "-A"]);
            self.git(&["commit", "-q", "-m", "change"]);
        }
    }

    fn no_cancel() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    fn all(_: &str) -> bool {
        true
    }

    #[test]
    fn a_folder_without_git_is_reported_as_such() {
        let dir = tempfile::tempdir().unwrap();
        let err = pick_commits(dir.path(), &TimelineOptions::default(), &no_cancel()).unwrap_err();
        assert!(matches!(err, CoreError::NotAGitRepo(_)));
    }

    #[test]
    fn a_repository_with_no_commits_yields_no_snapshots() {
        let f = Fixture::new();
        let picked = pick_commits(f.path(), &TimelineOptions::default(), &no_cancel()).unwrap();
        assert!(picked.is_empty());
    }

    #[test]
    fn snapshots_run_in_ancestry_order_even_when_timestamps_tie() {
        let f = Fixture::new();
        f.commit(&[("one.ts", "1")]);
        f.commit(&[("two.ts", "2")]);
        f.commit(&[("three.ts", "3")]);

        let picked = pick_commits(
            f.path(),
            &TimelineOptions {
                steps: 12,
                window_days: 3650,
            },
            &no_cancel(),
        )
        .unwrap();
        assert_eq!(picked.len(), 3);

        let mut previous = 0;
        for (commit, _) in &picked {
            let n = files_at(f.path(), commit, &all, &no_cancel())
                .unwrap()
                .len();
            assert!(n > previous, "expected growth, got {n} after {previous}");
            previous = n;
        }
    }

    #[test]
    fn commits_are_picked_oldest_first() {
        let f = Fixture::new();
        f.commit(&[("a.ts", "1")]);
        f.commit(&[("a.ts", "2")]);
        f.commit(&[("a.ts", "3")]);
        let picked = pick_commits(f.path(), &TimelineOptions::default(), &no_cancel()).unwrap();
        assert!(!picked.is_empty());
        for pair in picked.windows(2) {
            assert!(
                pair[0].1 <= pair[1].1,
                "snapshots must run forwards in time"
            );
        }
    }

    #[test]
    fn a_young_repository_still_gets_one_snapshot_per_commit() {
        let f = Fixture::new();
        f.commit(&[("a.ts", "1")]);
        f.commit(&[("a.ts", "2")]);
        f.commit(&[("a.ts", "3")]);
        let picked = pick_commits(
            f.path(),
            &TimelineOptions {
                steps: 12,
                window_days: 3650,
            },
            &no_cancel(),
        )
        .unwrap();
        assert_eq!(picked.len(), 3, "got {picked:?}");
    }

    #[test]
    fn no_more_snapshots_than_requested() {
        let f = Fixture::new();
        for i in 0..20 {
            f.commit(&[("a.ts", &i.to_string())]);
        }
        let opts = TimelineOptions {
            steps: 5,
            window_days: 3650,
        };
        let picked = pick_commits(f.path(), &opts, &no_cancel()).unwrap();
        assert!(picked.len() <= 5, "got {}", picked.len());
    }

    #[test]
    fn files_are_read_from_the_object_database_without_a_checkout() {
        let f = Fixture::new();
        f.commit(&[("src/a.ts", "export const a = 1;")]);
        let first = pick_commits(f.path(), &TimelineOptions::default(), &no_cancel()).unwrap();
        f.commit(&[
            ("src/a.ts", "export const a = 2;"),
            ("src/b.ts", "export const b = 1;"),
        ]);

        assert!(std::fs::read_to_string(f.path().join("src/a.ts"))
            .unwrap()
            .contains("= 2"));

        let files = files_at(f.path(), &first[0].0, &all, &no_cancel()).unwrap();
        let a = files.iter().find(|x| x.path == "src/a.ts").unwrap();
        assert!(String::from_utf8_lossy(&a.bytes).contains("= 1"));
        assert_eq!(files.len(), 1, "b.ts did not exist yet");

        assert!(std::fs::read_to_string(f.path().join("src/a.ts"))
            .unwrap()
            .contains("= 2"));
    }

    #[test]
    fn unchanged_files_keep_the_same_blob_across_snapshots() {
        let f = Fixture::new();
        f.commit(&[("stable.ts", "unchanged"), ("moving.ts", "1")]);
        f.commit(&[("moving.ts", "2")]);

        let picked = pick_commits(
            f.path(),
            &TimelineOptions {
                steps: 12,
                window_days: 3650,
            },
            &no_cancel(),
        )
        .unwrap();
        assert!(
            picked.len() >= 2,
            "need two snapshots, got {}",
            picked.len()
        );

        let first = files_at(f.path(), &picked[0].0, &all, &no_cancel()).unwrap();
        let last = files_at(f.path(), &picked[picked.len() - 1].0, &all, &no_cancel()).unwrap();

        let blob_of = |files: &[HistoricFile], name: &str| {
            files
                .iter()
                .find(|x| x.path == name)
                .map(|x| x.blob.clone())
        };
        assert_eq!(blob_of(&first, "stable.ts"), blob_of(&last, "stable.ts"));
        assert_ne!(blob_of(&first, "moving.ts"), blob_of(&last, "moving.ts"));
    }

    #[test]
    fn the_filter_keeps_out_files_we_do_not_analyse() {
        let f = Fixture::new();
        f.commit(&[("src/a.ts", "x"), ("README.md", "hi"), ("logo.png", "bin")]);
        let picked = pick_commits(f.path(), &TimelineOptions::default(), &no_cancel()).unwrap();
        let files = files_at(
            f.path(),
            &picked[0].0,
            &|p| p.ends_with(".ts"),
            &no_cancel(),
        )
        .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/a.ts");
    }

    #[test]
    fn nested_directories_are_read() {
        let f = Fixture::new();
        f.commit(&[("a/b/c/deep.ts", "x"), ("top.ts", "y")]);
        let picked = pick_commits(f.path(), &TimelineOptions::default(), &no_cancel()).unwrap();
        let files = files_at(f.path(), &picked[0].0, &all, &no_cancel()).unwrap();
        let paths: Vec<&str> = files.iter().map(|x| x.path.as_str()).collect();
        assert_eq!(paths, ["a/b/c/deep.ts", "top.ts"]);
    }

    #[test]
    fn an_unknown_commit_is_an_error_not_a_panic() {
        let f = Fixture::new();
        f.commit(&[("a.ts", "x")]);
        let err = files_at(f.path(), "not-a-commit", &all, &no_cancel()).unwrap_err();
        assert!(matches!(err, CoreError::Git(_)), "got {err}");
    }

    #[test]
    fn the_diff_reports_additions_removals_and_movement() {
        let before: HashMap<String, u32> = [
            ("kept.ts".to_string(), 100),
            ("gone.ts".to_string(), 20),
            ("grew.ts".to_string(), 10),
            ("shrank.ts".to_string(), 90),
        ]
        .into_iter()
        .collect();
        let after: HashMap<String, u32> = [
            ("kept.ts".to_string(), 100),
            ("new.ts".to_string(), 5),
            ("grew.ts".to_string(), 60),
            ("shrank.ts".to_string(), 40),
        ]
        .into_iter()
        .collect();

        let d = diff(&before, &after);
        assert_eq!(d.added, ["new.ts"]);
        assert_eq!(d.removed, ["gone.ts"]);
        assert_eq!(d.grown, [("grew.ts".to_string(), 50)]);
        assert_eq!(d.shrunk, [("shrank.ts".to_string(), -50)]);
    }

    #[test]
    fn an_unchanged_file_appears_in_no_diff_bucket() {
        let same: HashMap<String, u32> = [("a.ts".to_string(), 10)].into_iter().collect();
        let d = diff(&same, &same);
        assert_eq!(d, SnapshotDiff::default());
    }

    #[test]
    fn labels_match_the_span_being_shown() {
        let t = 1_773_576_000;

        assert_eq!(label_for(t, 300 * 86_400), "2026-03");

        assert_eq!(label_for(t, 20 * 86_400), "03-15");

        assert_eq!(label_for(t, 3_600), "12:00");
    }

    #[test]
    fn a_repository_whose_history_is_one_day_old_gets_distinguishable_labels() {
        let f = Fixture::new();
        f.commit(&[("a.ts", "1")]);
        f.commit(&[("b.ts", "2")]);
        let picked = pick_commits(
            f.path(),
            &TimelineOptions {
                steps: 12,
                window_days: 3650,
            },
            &no_cancel(),
        )
        .unwrap();
        assert_eq!(picked.len(), 2);

        let labels: Vec<String> = picked.iter().map(|(_, t)| label_for(*t, 0)).collect();
        assert!(labels[0].contains(':'), "expected a time, got {labels:?}");
    }

    #[test]
    fn cancelling_stops_a_historic_read() {
        let f = Fixture::new();
        f.commit(&[("a.ts", "x"), ("b.ts", "y")]);
        let picked = pick_commits(f.path(), &TimelineOptions::default(), &no_cancel()).unwrap();
        let files = files_at(
            f.path(),
            &picked[0].0,
            &all,
            &Arc::new(AtomicBool::new(true)),
        )
        .unwrap();
        assert!(files.is_empty());
    }
}

pub type LocByPath = HashMap<String, u32>;

pub type TimelineData = (Vec<Snapshot>, Vec<LocByPath>);

pub fn build_timeline(
    root: &Path,
    opts: &TimelineOptions,
    cache: Option<&crate::cache::Cache>,
    want: &dyn Fn(&str) -> bool,
    language_of: &dyn Fn(&str) -> Option<crate::model::Language>,
    cancel: &Arc<AtomicBool>,
) -> Result<TimelineData> {
    let picked = pick_commits(root, opts, cancel)?;
    let span = match (picked.first(), picked.last()) {
        (Some(first), Some(last)) => (last.1 - first.1).max(0),
        _ => 0,
    };
    let mut snapshots = Vec::with_capacity(picked.len());
    let mut per_snapshot_loc = Vec::with_capacity(picked.len());

    for (commit, time) in &picked {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let files = files_at(root, commit, want, cancel)?;
        let mut loc_by_path = LocByPath::new();
        let mut total_loc = 0u32;
        let mut edge_count = 0usize;

        for file in &files {
            let Some(language) = language_of(&file.path) else {
                continue;
            };

            let hash = crate::cache::content_hash(&file.bytes);
            let parsed = match cache.and_then(|c| c.get(&hash)) {
                Some(hit) => hit,
                None => {
                    let source = String::from_utf8_lossy(&file.bytes);
                    let p = crate::parse::parse(language, &source);
                    if let Some(c) = cache {
                        let _ = c.put(&hash, language as u8, &p);
                    }
                    p
                }
            };
            edge_count += parsed.imports.len();
            total_loc += parsed.loc;
            loc_by_path.insert(file.path.clone(), parsed.loc);
        }

        snapshots.push(Snapshot {
            commit: commit.chars().take(7).collect(),
            time: *time,
            label: label_for(*time, span),
            file_count: loc_by_path.len(),
            total_loc,
            edge_count,
        });
        per_snapshot_loc.push(loc_by_path);
    }

    Ok((snapshots, per_snapshot_loc))
}

#[cfg(test)]
mod timeline_tests {
    use super::*;
    use crate::cache::Cache;
    use crate::model::Language;
    use std::process::Command;

    fn ts_only(path: &str) -> bool {
        path.ends_with(".ts")
    }
    fn lang(path: &str) -> Option<Language> {
        path.ends_with(".ts").then_some(Language::TypeScript)
    }

    struct Repo {
        dir: tempfile::TempDir,
    }

    impl Repo {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let r = Repo { dir };
            for args in [
                vec!["init", "-q", "-b", "main"],
                vec!["config", "user.email", "a@example.com"],
                vec!["config", "user.name", "Ada"],
            ] {
                Command::new("git")
                    .args(&args)
                    .current_dir(r.dir.path())
                    .output()
                    .unwrap();
            }
            r
        }
        fn commit(&self, files: &[(&str, &str)]) {
            for (n, b) in files {
                let p = self.dir.path().join(n);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, b).unwrap();
            }
            for args in [vec!["add", "-A"], vec!["commit", "-q", "-m", "c"]] {
                Command::new("git")
                    .args(&args)
                    .current_dir(self.dir.path())
                    .output()
                    .unwrap();
            }
        }
    }

    fn opts() -> TimelineOptions {
        TimelineOptions {
            steps: 12,
            window_days: 3650,
        }
    }

    #[test]
    fn the_timeline_records_growth_over_time() {
        let r = Repo::new();
        r.commit(&[("a.ts", "export const a = 1;")]);
        r.commit(&[
            ("a.ts", "export const a = 1;"),
            ("b.ts", "export const b = 2;"),
        ]);
        r.commit(&[
            ("a.ts", "export const a = 1;"),
            ("b.ts", "export const b = 2;"),
            ("c.ts", "export const c = 3;"),
        ]);

        let (snaps, _) = build_timeline(
            r.dir.path(),
            &opts(),
            None,
            &ts_only,
            &lang,
            &Arc::new(AtomicBool::new(false)),
        )
        .unwrap();

        assert_eq!(snaps.len(), 3);
        assert_eq!(snaps[0].file_count, 1);
        assert_eq!(snaps[2].file_count, 3);
        assert!(snaps[2].total_loc > snaps[0].total_loc);
        for pair in snaps.windows(2) {
            assert!(pair[0].time <= pair[1].time);
        }
    }

    #[test]
    fn a_shared_blob_is_parsed_once_across_the_whole_timeline() {
        let r = Repo::new();
        let stable: Vec<(String, String)> = (0..9)
            .map(|i| (format!("s{i}.ts"), format!("export const s{i} = {i};")))
            .collect();
        let mut first: Vec<(&str, &str)> = stable
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        first.push(("moving.ts", "export const m = 1;"));
        r.commit(&first);

        let mut second = first.clone();
        second.pop();
        second.push(("moving.ts", "export const m = 2;"));
        r.commit(&second);

        let cache = Cache::in_memory().unwrap();
        let (snaps, _) = build_timeline(
            r.dir.path(),
            &opts(),
            Some(&cache),
            &ts_only,
            &lang,
            &Arc::new(AtomicBool::new(false)),
        )
        .unwrap();

        assert_eq!(snaps.len(), 2);

        assert_eq!(
            cache.len(),
            11,
            "nine stable blobs plus two versions of the one that moved"
        );
    }

    #[test]
    fn a_repository_without_history_yields_an_empty_timeline() {
        let r = Repo::new();
        let (snaps, locs) = build_timeline(
            r.dir.path(),
            &opts(),
            None,
            &ts_only,
            &lang,
            &Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert!(snaps.is_empty());
        assert!(locs.is_empty());
    }

    #[test]
    fn consecutive_snapshots_diff_into_readable_changes() {
        let r = Repo::new();
        r.commit(&[
            ("keep.ts", "export const k = 1;"),
            ("gone.ts", "export const g = 1;"),
        ]);

        std::fs::remove_file(r.dir.path().join("gone.ts")).unwrap();
        r.commit(&[
            ("keep.ts", "export const k = 1;\nexport const extra = 2;"),
            ("new.ts", "export const n = 1;"),
        ]);

        let (_, locs) = build_timeline(
            r.dir.path(),
            &opts(),
            None,
            &ts_only,
            &lang,
            &Arc::new(AtomicBool::new(false)),
        )
        .unwrap();

        let d = diff(&locs[0], &locs[1]);
        assert_eq!(d.added, ["new.ts"]);
        assert_eq!(d.removed, ["gone.ts"]);
        assert_eq!(d.grown, [("keep.ts".to_string(), 1)]);
    }
}
