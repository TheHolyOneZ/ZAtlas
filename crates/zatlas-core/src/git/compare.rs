use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::analyse::{analyse, AnalyseOptions};
use crate::config::ZatlasConfig;
use crate::error::{CoreError, Result};
use crate::findings::{detect_all, FindingsOptions};
use crate::graph::{tangles, Adjacency};
use crate::model::Language;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct RefSide {
    pub reference: String,

    pub commit: String,
    pub files: usize,
    pub loc: u64,
    pub edges: usize,
    pub cycles: usize,
    pub unresolved: usize,
    pub findings: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct RefComparison {
    pub base: RefSide,
    pub head: RefSide,
    pub added: Vec<String>,
    pub removed: Vec<String>,

    pub grown: Vec<(String, i64)>,
    pub shrunk: Vec<(String, i64)>,

    pub cycles_introduced: Vec<Vec<String>>,
    pub cycles_resolved: Vec<Vec<String>>,

    pub findings_new: Vec<String>,
    pub findings_fixed: Vec<String>,

    pub skipped: Vec<String>,
}

fn wanted(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if matches!(
        name,
        "Cargo.toml"
            | "package.json"
            | "go.mod"
            | "pyproject.toml"
            | "setup.py"
            | "setup.cfg"
            | ".gitignore"
            | "zatlas.toml"
    ) {
        return true;
    }
    if name.starts_with("tsconfig.") && name.ends_with(".json") {
        return true;
    }
    if name.starts_with("jsconfig.") && name.ends_with(".json") {
        return true;
    }
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .and_then(Language::from_extension)
        .is_some()
}

pub fn resolve_ref(root: &Path, reference: &str) -> Result<String> {
    let repo = gix::open(root).map_err(|_| CoreError::NotAGitRepo(root.to_path_buf()))?;
    let id = repo
        .rev_parse_single(reference)
        .map_err(|e| CoreError::Git(format!("{reference}: {e}")))?;
    Ok(id.detach().to_hex().to_string())
}

pub fn branches(root: &Path) -> Result<Vec<String>> {
    let repo = gix::open(root).map_err(|_| CoreError::NotAGitRepo(root.to_path_buf()))?;
    let refs = repo
        .references()
        .map_err(|e| CoreError::Git(e.to_string()))?;
    let branches = refs
        .local_branches()
        .map_err(|e| CoreError::Git(e.to_string()))?;
    let mut names: Vec<String> = branches
        .filter_map(std::result::Result::ok)
        .map(|r| r.name().shorten().to_string())
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub fn windows_safe(rel: &str) -> bool {
    rel.split('/').all(|segment| {
        if segment.is_empty() {
            return true;
        }
        if segment.ends_with('.') || segment.ends_with(' ') {
            return false;
        }
        if segment
            .chars()
            .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        {
            return false;
        }

        let stem = segment.split('.').next().unwrap_or(segment);
        !RESERVED.iter().any(|r| stem.eq_ignore_ascii_case(r))
    })
}

fn materialise(
    root: &Path,
    commit: &str,
    into: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<Vec<String>> {
    let files = super::timetravel::files_at(root, commit, &wanted, cancel)?;
    let mut skipped: Vec<String> = Vec::new();

    let mut folded: HashSet<String> = HashSet::new();

    for file in files {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        if file.path.split('/').any(|part| part == ".." || part == ".") {
            skipped.push(file.path);
            continue;
        }
        if cfg!(windows) {
            if !windows_safe(&file.path) {
                skipped.push(file.path);
                continue;
            }
            if !folded.insert(file.path.to_lowercase()) {
                skipped.push(file.path);
                continue;
            }
        }

        let target = crate::paths::rel_to_path(into, &file.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
        }

        if std::fs::write(&target, &file.bytes).is_err() {
            skipped.push(file.path);
        }
    }
    skipped.sort();
    Ok(skipped)
}

fn cycle_keys(graph: &crate::model::Graph, adj: &Adjacency) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = tangles(adj)
        .into_iter()
        .map(|t| {
            let mut members: Vec<String> = t
                .members
                .iter()
                .filter_map(|id| graph.file(*id).map(|f| f.path.clone()))
                .collect();
            members.sort();
            members
        })
        .collect();
    out.sort();
    out
}

struct Analysed {
    side: RefSide,
    loc: HashMap<String, u32>,
    cycles: Vec<Vec<String>>,
    findings: Vec<String>,
    skipped: Vec<String>,
}

fn analyse_ref(
    root: &Path,
    reference: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<(Analysed, tempfile::TempDir)> {
    let commit = resolve_ref(root, reference)?;
    let dir = tempfile::tempdir().map_err(|e| CoreError::io(root, e))?;
    let skipped = materialise(root, &commit, dir.path(), cancel)?;

    let config = ZatlasConfig::load(dir.path()).unwrap_or_default();
    let opts = AnalyseOptions {
        use_cache: false,
        ..Default::default()
    };
    let analysis = analyse(dir.path(), &config, &opts, cancel, &|_, _, _| {})?;

    let graph = &analysis.graph;
    let adj = Adjacency::build(graph.files.len(), &graph.edges);
    let cycles = cycle_keys(graph, &adj);
    let findings = detect_all(
        graph,
        &config,
        &HashMap::new(),
        &[],
        &FindingsOptions::default(),
    );

    let side = RefSide {
        reference: reference.to_owned(),
        commit: commit.chars().take(7).collect(),
        files: graph.files.len(),
        loc: graph.files.iter().map(|f| u64::from(f.loc)).sum(),
        edges: graph.edges.len(),
        cycles: cycles.len(),
        unresolved: graph.unresolved_failure_count(),
        findings: findings.len(),
    };

    Ok((
        Analysed {
            side,
            loc: graph
                .files
                .iter()
                .map(|f| (f.path.clone(), f.loc))
                .collect(),
            cycles,
            findings: findings.into_iter().map(|f| f.headline).collect(),
            skipped,
        },
        dir,
    ))
}

pub fn compare(
    root: &Path,
    base_ref: &str,
    head_ref: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<RefComparison> {
    let (base, _base_dir) = analyse_ref(root, base_ref, cancel)?;
    let (head, _head_dir) = analyse_ref(root, head_ref, cancel)?;

    let mut added: Vec<String> = head
        .loc
        .keys()
        .filter(|p| !base.loc.contains_key(*p))
        .cloned()
        .collect();
    let mut removed: Vec<String> = base
        .loc
        .keys()
        .filter(|p| !head.loc.contains_key(*p))
        .cloned()
        .collect();
    added.sort();
    removed.sort();

    let mut grown: Vec<(String, i64)> = Vec::new();
    let mut shrunk: Vec<(String, i64)> = Vec::new();
    for (path, after) in &head.loc {
        let Some(before) = base.loc.get(path) else {
            continue;
        };
        let delta = i64::from(*after) - i64::from(*before);
        match delta.cmp(&0) {
            std::cmp::Ordering::Greater => grown.push((path.clone(), delta)),
            std::cmp::Ordering::Less => shrunk.push((path.clone(), delta)),
            std::cmp::Ordering::Equal => {}
        }
    }

    grown.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    shrunk.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

    let base_cycles: HashSet<&Vec<String>> = base.cycles.iter().collect();
    let head_cycles: HashSet<&Vec<String>> = head.cycles.iter().collect();
    let cycles_introduced: Vec<Vec<String>> = head
        .cycles
        .iter()
        .filter(|c| !base_cycles.contains(c))
        .cloned()
        .collect();
    let cycles_resolved: Vec<Vec<String>> = base
        .cycles
        .iter()
        .filter(|c| !head_cycles.contains(c))
        .cloned()
        .collect();

    let base_findings: HashSet<&String> = base.findings.iter().collect();
    let head_findings: HashSet<&String> = head.findings.iter().collect();
    let mut findings_new: Vec<String> = head
        .findings
        .iter()
        .filter(|f| !base_findings.contains(f))
        .cloned()
        .collect();
    let mut findings_fixed: Vec<String> = base
        .findings
        .iter()
        .filter(|f| !head_findings.contains(f))
        .cloned()
        .collect();
    findings_new.sort();
    findings_fixed.sort();

    let mut skipped: Vec<String> = base
        .skipped
        .iter()
        .chain(head.skipped.iter())
        .cloned()
        .collect();
    skipped.sort();
    skipped.dedup();

    Ok(RefComparison {
        base: base.side,
        head: head.side,
        added,
        removed,
        grown,
        shrunk,
        cycles_introduced,
        cycles_resolved,
        findings_new,
        findings_fixed,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    struct Repo(tempfile::TempDir);

    impl Repo {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            for args in [
                vec!["init", "-q", "-b", "main"],
                vec!["config", "user.email", "a@example.com"],
                vec!["config", "user.name", "Ada"],
            ] {
                Command::new("git")
                    .args(&args)
                    .current_dir(dir.path())
                    .output()
                    .unwrap();
            }
            Repo(dir)
        }

        fn path(&self) -> &Path {
            self.0.path()
        }

        fn write(&self, rel: &str, body: &str) {
            let p = crate::paths::rel_to_path(self.path(), rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }

        fn commit(&self, message: &str) {
            Command::new("git")
                .args(["add", "-A"])
                .current_dir(self.path())
                .output()
                .unwrap();
            Command::new("git")
                .args(["commit", "-q", "-m", message])
                .current_dir(self.path())
                .output()
                .unwrap();
        }

        fn branch(&self, name: &str) {
            Command::new("git")
                .args(["checkout", "-q", "-b", name])
                .current_dir(self.path())
                .output()
                .unwrap();
        }
    }

    fn flag() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn a_branch_that_adds_a_cycle_is_reported_as_introducing_one() {
        let repo = Repo::new();
        repo.write("src/a.ts", "export const a = 1;\n");
        repo.write(
            "src/b.ts",
            "import { a } from './a';\nexport const b = a;\n",
        );
        repo.commit("base");
        repo.branch("feature");

        repo.write(
            "src/a.ts",
            "import { b } from './b';\nexport const a = b;\n",
        );
        repo.commit("cycle");

        let out = compare(repo.path(), "main", "feature", &flag()).unwrap();
        assert_eq!(out.cycles_introduced.len(), 1, "{out:?}");
        assert_eq!(
            out.cycles_introduced[0],
            vec!["src/a.ts".to_string(), "src/b.ts".to_string()]
        );
        assert!(out.cycles_resolved.is_empty());
        assert_eq!(out.base.cycles, 0);
        assert_eq!(out.head.cycles, 1);
    }

    #[test]
    fn a_branch_that_breaks_a_cycle_is_reported_as_resolving_one() {
        let repo = Repo::new();
        repo.write(
            "src/a.ts",
            "import { b } from './b';\nexport const a = b;\n",
        );
        repo.write(
            "src/b.ts",
            "import { a } from './a';\nexport const b = a;\n",
        );
        repo.commit("tangled");
        repo.branch("fix");
        repo.write("src/a.ts", "export const a = 1;\n");
        repo.commit("untangled");

        let out = compare(repo.path(), "main", "fix", &flag()).unwrap();
        assert_eq!(out.cycles_resolved.len(), 1);
        assert!(out.cycles_introduced.is_empty());
    }

    #[test]
    fn added_removed_and_resized_files_are_each_reported() {
        let repo = Repo::new();
        repo.write("src/keep.ts", "export const k = 1;\n");
        repo.write("src/gone.ts", "export const g = 1;\n");
        repo.commit("base");
        repo.branch("work");
        std::fs::remove_file(crate::paths::rel_to_path(repo.path(), "src/gone.ts")).unwrap();
        repo.write("src/new.ts", "export const n = 1;\n");
        repo.write("src/keep.ts", "export const k = 1;\nexport const k2 = 2;\n");
        repo.commit("work");

        let out = compare(repo.path(), "main", "work", &flag()).unwrap();
        assert_eq!(out.added, ["src/new.ts"]);
        assert_eq!(out.removed, ["src/gone.ts"]);
        assert_eq!(out.grown, [("src/keep.ts".to_string(), 1)]);
        assert!(out.shrunk.is_empty());
    }

    #[test]
    fn comparing_a_ref_with_itself_reports_no_change() {
        let repo = Repo::new();
        repo.write("src/a.ts", "export const a = 1;\n");
        repo.commit("only");

        let out = compare(repo.path(), "main", "main", &flag()).unwrap();
        assert!(out.added.is_empty());
        assert!(out.removed.is_empty());
        assert!(out.grown.is_empty());
        assert!(out.shrunk.is_empty());
        assert!(out.cycles_introduced.is_empty());
        assert!(out.findings_new.is_empty());
        assert_eq!(out.base.commit, out.head.commit);
    }

    #[test]
    fn a_branch_analysed_under_its_own_tsconfig_not_the_working_trees() {
        let repo = Repo::new();
        repo.write(
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@x/*":["src/*"]}}}"#,
        );
        repo.write("src/dep.ts", "export const d = 1;\n");
        repo.write(
            "src/main.ts",
            "import { d } from '@x/dep';\nexport const m = d;\n",
        );
        repo.commit("base");
        repo.branch("rename");

        repo.write(
            "tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@y/*":["src/*"]}}}"#,
        );
        repo.write(
            "src/main.ts",
            "import { d } from '@y/dep';\nexport const m = d;\n",
        );
        repo.commit("rename");

        let out = compare(repo.path(), "main", "rename", &flag()).unwrap();

        assert_eq!(out.base.unresolved, 0, "{out:?}");
        assert_eq!(out.head.unresolved, 0, "{out:?}");
        assert_eq!(out.base.edges, 1);
        assert_eq!(out.head.edges, 1);
    }

    #[test]
    fn branches_are_listed_for_the_picker() {
        let repo = Repo::new();
        repo.write("src/a.ts", "export const a = 1;\n");
        repo.commit("only");
        repo.branch("feature");
        let mut names = branches(repo.path()).unwrap();
        names.sort();
        assert_eq!(names, ["feature", "main"]);
    }

    #[test]
    fn listing_branches_outside_a_repository_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert!(branches(dir.path()).is_err());
    }

    #[test]
    fn an_unknown_ref_is_an_error_naming_it() {
        let repo = Repo::new();
        repo.write("src/a.ts", "export const a = 1;\n");
        repo.commit("only");
        let err = compare(repo.path(), "main", "no-such-branch", &flag())
            .unwrap_err()
            .to_string();
        assert!(err.contains("no-such-branch"), "got: {err}");
    }

    #[test]
    fn windows_reserved_device_names_are_not_writable() {
        for bad in ["aux.ts", "src/con.rs", "NUL", "lpt1.go", "src/Aux.py"] {
            assert!(!windows_safe(bad), "{bad} should be rejected");
        }
        for ok in [
            "src/auxiliary.ts",
            "console.rs",
            "src/nullable.go",
            "com.ts",
        ] {
            assert!(windows_safe(ok), "{ok} should be fine");
        }
    }

    #[test]
    fn characters_windows_forbids_in_a_name_are_rejected() {
        for bad in [
            "a:b.ts",
            "what?.rs",
            "a|b.go",
            "quo\"te.ts",
            "star*.py",
            "lt<gt>.ts",
        ] {
            assert!(!windows_safe(bad), "{bad} should be rejected");
        }
    }

    #[test]
    fn a_trailing_dot_or_space_is_rejected_because_windows_trims_it() {
        assert!(!windows_safe("src/trailing."));
        assert!(!windows_safe("src/trailing "));
        assert!(!windows_safe("trailing./a.ts"));
        assert!(windows_safe("src/fine.ts"));
    }

    #[test]
    fn ordinary_paths_are_writable() {
        for ok in [
            "src/main.rs",
            "a/b/c/d.ts",
            "crates/zatlas-core/src/lib.rs",
            "",
        ] {
            assert!(windows_safe(ok), "{ok} should be fine");
        }
    }

    #[test]
    fn a_comparison_reports_nothing_skipped_on_a_normal_repository() {
        let repo = Repo::new();
        repo.write("src/a.ts", "export const a = 1;\n");
        repo.commit("only");
        let out = compare(repo.path(), "main", "main", &flag()).unwrap();
        assert!(out.skipped.is_empty(), "{:?}", out.skipped);
    }

    #[test]
    fn a_tree_entry_that_tries_to_escape_the_directory_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir.path().join("escaped.ts").exists());

        for bad in ["../escaped.ts", "src/../../escaped.ts", "./a.ts"] {
            assert!(
                bad.split('/').any(|p| p == ".." || p == "."),
                "{bad} should be caught"
            );
        }
    }
}
