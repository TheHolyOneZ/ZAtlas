use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use ts_rs::TS;

use crate::error::{CoreError, Result};

#[derive(Debug, Clone)]
pub struct CommitRecord {
    pub id: String,

    pub time: i64,
    pub author: String,

    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FileHistory {
    pub commits: u32,

    pub last_touched: i64,

    pub first_touched: i64,

    pub authors: u32,

    pub top_author_share: f32,
    pub top_author: String,
}

#[derive(Debug, Clone, Default)]
pub struct RepoHistory {
    pub files: HashMap<String, FileHistory>,
    pub commits: Vec<CommitRecord>,

    pub total_commits: usize,

    pub head_time: i64,
}

pub struct HistoryOptions {
    pub window_days: Option<i64>,

    pub max_commits: usize,
}

impl Default for HistoryOptions {
    fn default() -> Self {
        Self {
            window_days: Some(365),
            max_commits: 20_000,
        }
    }
}

pub fn history(
    root: &Path,
    opts: &HistoryOptions,
    cancel: &Arc<AtomicBool>,
) -> Result<RepoHistory> {
    let repo = gix::open(root).map_err(|e| match e {
        gix::open::Error::NotARepository { .. } => CoreError::NotAGitRepo(root.to_path_buf()),
        other => CoreError::Git(other.to_string()),
    })?;

    let Ok(head) = repo.head_id() else {
        return Ok(RepoHistory::default());
    };

    let walk = head
        .ancestors()
        .all()
        .map_err(|e| CoreError::Git(e.to_string()))?;

    let mut out = RepoHistory::default();
    let mut cutoff: Option<i64> = None;

    for (n, info) in walk.enumerate() {
        if n >= opts.max_commits || cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(info) = info else { continue };
        let Ok(commit) = repo.find_commit(info.id) else {
            continue;
        };
        let Ok(time) = commit.time() else { continue };
        let seconds = time.seconds;

        if out.head_time == 0 {
            out.head_time = seconds;
            if let Some(days) = opts.window_days {
                cutoff = Some(seconds - days * 86_400);
            }
        }
        if cutoff.is_some_and(|c| seconds < c) {
            break;
        }

        out.total_commits += 1;

        let author = commit
            .author()
            .map(|a| a.name.to_string())
            .unwrap_or_default();

        let files = changed_files(&repo, &commit);
        if files.is_empty() {
            continue;
        }

        out.commits.push(CommitRecord {
            id: info.id.to_hex_with_len(7).to_string(),
            time: seconds,
            author,
            files,
        });
    }

    out.files = summarise(&out.commits);
    Ok(out)
}

fn changed_files(repo: &gix::Repository, commit: &gix::Commit) -> Vec<String> {
    let Ok(tree) = commit.tree() else {
        return Vec::new();
    };

    let parent_tree = commit
        .parent_ids()
        .next()
        .and_then(|id| repo.find_commit(id).ok())
        .and_then(|p| p.tree().ok());

    let mut paths = Vec::new();

    let Some(parent_tree) = parent_tree else {
        collect_tree_paths(&tree, &mut paths);
        return paths;
    };

    let Ok(mut changes) = parent_tree.changes() else {
        return paths;
    };
    let _ = changes.for_each_to_obtain_tree(&tree, |change| {
        if change.entry_mode().is_blob() {
            let path = change.location().to_string();
            if !path.is_empty() {
                paths.push(path);
            }
        }
        Ok::<_, std::convert::Infallible>(std::ops::ControlFlow::<()>::Continue(()))
    });

    paths.sort();
    paths.dedup();
    paths
}

fn collect_tree_paths(tree: &gix::Tree, out: &mut Vec<String>) {
    let mut recorder = gix::traverse::tree::Recorder::default();
    if tree.traverse().breadthfirst(&mut recorder).is_ok() {
        for entry in recorder.records {
            if entry.mode.is_blob() {
                out.push(entry.filepath.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
}

fn summarise(commits: &[CommitRecord]) -> HashMap<String, FileHistory> {
    let mut authors_by_file: HashMap<&str, HashMap<&str, u32>> = HashMap::new();
    let mut out: HashMap<String, FileHistory> = HashMap::new();

    for c in commits {
        for path in &c.files {
            let entry = out.entry(path.clone()).or_insert(FileHistory {
                first_touched: c.time,
                last_touched: c.time,
                ..Default::default()
            });
            entry.commits += 1;
            entry.last_touched = entry.last_touched.max(c.time);
            entry.first_touched = if entry.first_touched == 0 {
                c.time
            } else {
                entry.first_touched.min(c.time)
            };
            *authors_by_file
                .entry(path.as_str())
                .or_default()
                .entry(c.author.as_str())
                .or_insert(0) += 1;
        }
    }

    for (path, authors) in authors_by_file {
        let Some(entry) = out.get_mut(path) else {
            continue;
        };
        entry.authors = authors.len() as u32;

        if let Some((name, count)) = authors
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        {
            entry.top_author = (*name).to_string();
            entry.top_author_share = *count as f32 / entry.commits.max(1) as f32;
        }
    }

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
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }

        fn commit(&self, author: &str, files: &[(&str, &str)]) {
            for (name, body) in files {
                let p = self.path().join(name);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, body).unwrap();
            }
            self.git(&["add", "-A"]);
            self.git(&[
                "-c",
                &format!("user.name={author}"),
                "commit",
                "-q",
                "-m",
                "change",
            ]);
        }

        fn read(&self) -> RepoHistory {
            history(
                self.path(),
                &HistoryOptions {
                    window_days: None,
                    max_commits: 1000,
                },
                &Arc::new(AtomicBool::new(false)),
            )
            .unwrap()
        }
    }

    #[test]
    fn a_folder_without_git_is_reported_as_such_not_as_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let err = history(
            dir.path(),
            &HistoryOptions::default(),
            &Arc::new(AtomicBool::new(false)),
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::NotAGitRepo(_)), "got {err}");
    }

    #[test]
    fn a_repository_with_no_commits_has_empty_history_rather_than_an_error() {
        let f = Fixture::new();
        let h = f.read();
        assert_eq!(h.total_commits, 0);
        assert!(h.files.is_empty());
    }

    #[test]
    fn the_root_commit_counts_every_file_it_introduced() {
        let f = Fixture::new();
        f.commit("Ada", &[("a.txt", "1"), ("b.txt", "1")]);
        let h = f.read();
        assert_eq!(h.total_commits, 1);
        assert_eq!(h.files["a.txt"].commits, 1);
        assert_eq!(h.files["b.txt"].commits, 1);
    }

    #[test]
    fn churn_counts_commits_per_file() {
        let f = Fixture::new();
        f.commit("Ada", &[("hot.txt", "1"), ("cold.txt", "1")]);
        f.commit("Ada", &[("hot.txt", "2")]);
        f.commit("Ada", &[("hot.txt", "3")]);
        let h = f.read();
        assert_eq!(h.files["hot.txt"].commits, 3);
        assert_eq!(h.files["cold.txt"].commits, 1);
    }

    #[test]
    fn only_files_a_commit_actually_touched_are_counted() {
        let f = Fixture::new();
        f.commit("Ada", &[("a.txt", "1"), ("b.txt", "1"), ("c.txt", "1")]);
        f.commit("Ada", &[("a.txt", "2")]);
        let h = f.read();
        assert_eq!(
            h.commits[0].files,
            ["a.txt"],
            "newest commit touched only a"
        );
    }

    #[test]
    fn directories_are_never_reported_as_changed_files() {
        let f = Fixture::new();
        f.commit("Ada", &[("src/a.txt", "1")]);
        f.commit("Ada", &[("src/components/b.txt", "1")]);
        let h = f.read();
        for path in h.files.keys() {
            assert!(
                path.ends_with(".txt"),
                "{path} is a directory, not a changed file"
            );
        }
        assert!(h.files.contains_key("src/components/b.txt"));
    }

    #[test]
    fn ownership_records_the_dominant_author_and_their_share() {
        let f = Fixture::new();
        f.commit("Ada", &[("owned.txt", "1")]);
        f.commit("Ada", &[("owned.txt", "2")]);
        f.commit("Ada", &[("owned.txt", "3")]);
        f.commit("Grace", &[("owned.txt", "4")]);
        let h = f.read();
        let owned = &h.files["owned.txt"];
        assert_eq!(owned.authors, 2);
        assert_eq!(owned.top_author, "Ada");
        assert!(
            (owned.top_author_share - 0.75).abs() < 1e-6,
            "got {}",
            owned.top_author_share
        );
    }

    #[test]
    fn a_single_author_file_has_a_bus_factor_of_one() {
        let f = Fixture::new();
        f.commit("Ada", &[("solo.txt", "1")]);
        f.commit("Ada", &[("solo.txt", "2")]);
        let h = f.read();
        assert_eq!(h.files["solo.txt"].authors, 1);
        assert_eq!(h.files["solo.txt"].top_author_share, 1.0);
    }

    #[test]
    fn first_and_last_touched_bracket_the_files_life() {
        let f = Fixture::new();
        f.commit("Ada", &[("a.txt", "1")]);
        f.commit("Ada", &[("a.txt", "2")]);
        let h = f.read();
        let a = &h.files["a.txt"];
        assert!(a.last_touched >= a.first_touched);
        assert!(a.first_touched > 0);
    }

    #[test]
    fn max_commits_stops_the_walk_early() {
        let f = Fixture::new();
        for i in 0..10 {
            f.commit("Ada", &[("a.txt", &i.to_string())]);
        }
        let h = history(
            f.path(),
            &HistoryOptions {
                window_days: None,
                max_commits: 3,
            },
            &Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(h.total_commits, 3);
    }

    #[test]
    fn cancelling_stops_the_walk() {
        let f = Fixture::new();
        for i in 0..10 {
            f.commit("Ada", &[("a.txt", &i.to_string())]);
        }
        let h = history(
            f.path(),
            &HistoryOptions {
                window_days: None,
                max_commits: 1000,
            },
            &Arc::new(AtomicBool::new(true)),
        )
        .unwrap();
        assert_eq!(h.total_commits, 0);
    }

    #[test]
    fn a_merge_is_diffed_against_its_first_parent_only() {
        let f = Fixture::new();
        f.commit("Ada", &[("base.txt", "1")]);
        f.git(&["checkout", "-q", "-b", "side"]);
        f.commit("Ada", &[("side.txt", "1")]);
        f.git(&["checkout", "-q", "main"]);
        f.commit("Ada", &[("main.txt", "1")]);
        f.git(&["merge", "-q", "--no-ff", "-m", "merge", "side"]);

        let h = f.read();

        assert_eq!(
            h.files["side.txt"].commits, 2,
            "one creation plus one merge"
        );
        assert_eq!(h.files["base.txt"].commits, 1);
    }

    #[test]
    fn history_never_modifies_the_working_tree() {
        let f = Fixture::new();
        f.commit("Ada", &[("a.txt", "content")]);
        let before = std::fs::read_to_string(f.path().join("a.txt")).unwrap();
        let status_before = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(f.path())
            .output()
            .unwrap();

        let _ = f.read();

        let after = std::fs::read_to_string(f.path().join("a.txt")).unwrap();
        let status_after = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(f.path())
            .output()
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(status_before.stdout, status_after.stdout);
    }
}
