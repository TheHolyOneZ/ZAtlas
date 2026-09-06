use ignore::{WalkBuilder, WalkState};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::config::ZatlasConfig;
use crate::error::{CoreError, Result};
use crate::model::Language;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedFile {
    pub rel_path: String,
    pub abs_path: PathBuf,
    pub language: Language,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedFile {
    pub rel_path: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    TooLarge { bytes: u64, limit: u64 },

    Binary,

    Excluded,
    Unreadable(String),
}

pub struct WalkOptions {
    pub max_file_bytes: u64,

    pub respect_gitignore: bool,

    pub hidden: bool,
    pub threads: usize,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            max_file_bytes: 2 * 1024 * 1024,
            respect_gitignore: true,
            hidden: false,
            threads: num_cpus::get(),
        }
    }
}

#[derive(Debug, Default)]
pub struct WalkOutcome {
    pub files: Vec<WalkedFile>,
    pub skipped: Vec<SkippedFile>,

    pub manifests: Vec<String>,
}

pub fn is_manifest(file_name: &str) -> bool {
    matches!(
        file_name,
        "package.json"
            | "Cargo.toml"
            | "go.mod"
            | "go.work"
            | "jsconfig.json"
            | "pnpm-workspace.yaml"
            | "pyproject.toml"
            | "setup.cfg"
    ) || (file_name.starts_with("tsconfig") && file_name.ends_with(".json"))
}

const ALWAYS_SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".zatlas",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".turbo",
    ".svelte-kit",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".tox",
    "vendor",
    "Pods",
    ".gradle",
    ".idea",
    ".vscode",
    "coverage",
    ".cache",
];

pub fn to_rel_string(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut s = String::with_capacity(rel.as_os_str().len());
    for (i, part) in rel.components().enumerate() {
        if i > 0 {
            s.push('/');
        }
        s.push_str(&part.as_os_str().to_string_lossy());
    }
    Some(s)
}

fn build_globset(patterns: &[String]) -> Result<Option<globset::GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = globset::GlobSetBuilder::new();
    for p in patterns {
        let glob = globset::Glob::new(p).map_err(|source| CoreError::Glob {
            pattern: p.clone(),
            source,
        })?;
        builder.add(glob);
    }
    let set = builder.build().map_err(|source| CoreError::Glob {
        pattern: patterns.join(", "),
        source,
    })?;
    Ok(Some(set))
}

fn looks_binary(path: &Path) -> bool {
    use std::io::Read;
    let mut buf = [0u8; 8192];
    match std::fs::File::open(path).and_then(|mut f| f.read(&mut buf)) {
        Ok(n) => buf[..n].contains(&0),
        Err(_) => false,
    }
}

pub fn walk(
    root: &Path,
    config: &ZatlasConfig,
    opts: &WalkOptions,
    cancel: &Arc<AtomicBool>,
) -> Result<WalkOutcome> {
    let include = build_globset(&config.include)?;
    let exclude = build_globset(&config.exclude)?;

    let files = Arc::new(Mutex::new(Vec::<WalkedFile>::new()));
    let skipped = Arc::new(Mutex::new(Vec::<SkippedFile>::new()));
    let manifests = Arc::new(Mutex::new(Vec::<String>::new()));

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(!opts.hidden)
        .git_ignore(opts.respect_gitignore)
        .git_global(opts.respect_gitignore)
        .git_exclude(opts.respect_gitignore)
        .require_git(false)
        .parents(opts.respect_gitignore)
        .follow_links(false)
        .threads(opts.threads.max(1));

    builder.filter_entry(|entry| {
        if entry.file_type().is_some_and(|t| t.is_dir()) {
            if let Some(name) = entry.file_name().to_str() {
                return !ALWAYS_SKIP_DIRS.contains(&name);
            }
        }
        true
    });

    builder.build_parallel().run(|| {
        let files = Arc::clone(&files);
        let skipped = Arc::clone(&skipped);
        let manifests = Arc::clone(&manifests);
        let cancel = Arc::clone(cancel);
        let include = include.clone();
        let exclude = exclude.clone();
        let root = root.to_path_buf();

        Box::new(move |result| {
            if cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            let entry = match result {
                Ok(e) => e,

                Err(e) => {
                    skipped.lock().unwrap().push(SkippedFile {
                        rel_path: String::new(),
                        reason: SkipReason::Unreadable(e.to_string()),
                    });
                    return WalkState::Continue;
                }
            };

            if !entry.file_type().is_some_and(|t| t.is_file()) {
                return WalkState::Continue;
            }

            let path = entry.path();
            let Some(rel) = to_rel_string(&root, path) else {
                return WalkState::Continue;
            };

            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if is_manifest(name) {
                    manifests.lock().unwrap().push(rel.clone());
                    return WalkState::Continue;
                }
            }

            let Some(language) = path
                .extension()
                .and_then(|e| e.to_str())
                .and_then(Language::from_extension)
            else {
                return WalkState::Continue;
            };

            if let Some(set) = &include {
                if !set.is_match(&rel) {
                    return WalkState::Continue;
                }
            }
            if let Some(set) = &exclude {
                if set.is_match(&rel) {
                    skipped.lock().unwrap().push(SkippedFile {
                        rel_path: rel,
                        reason: SkipReason::Excluded,
                    });
                    return WalkState::Continue;
                }
            }

            let bytes = match entry.metadata() {
                Ok(m) => m.len(),
                Err(e) => {
                    skipped.lock().unwrap().push(SkippedFile {
                        rel_path: rel,
                        reason: SkipReason::Unreadable(e.to_string()),
                    });
                    return WalkState::Continue;
                }
            };

            if bytes > opts.max_file_bytes {
                skipped.lock().unwrap().push(SkippedFile {
                    rel_path: rel,
                    reason: SkipReason::TooLarge {
                        bytes,
                        limit: opts.max_file_bytes,
                    },
                });
                return WalkState::Continue;
            }

            if looks_binary(path) {
                skipped.lock().unwrap().push(SkippedFile {
                    rel_path: rel,
                    reason: SkipReason::Binary,
                });
                return WalkState::Continue;
            }

            files.lock().unwrap().push(WalkedFile {
                rel_path: rel,
                abs_path: path.to_path_buf(),
                language,
                bytes,
            });
            WalkState::Continue
        })
    });

    let mut files = Arc::try_unwrap(files).unwrap().into_inner().unwrap();
    let mut skipped = Arc::try_unwrap(skipped).unwrap().into_inner().unwrap();
    let mut manifests = Arc::try_unwrap(manifests).unwrap().into_inner().unwrap();

    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    skipped.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    manifests.sort();

    Ok(WalkOutcome {
        files,
        skipped,
        manifests,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::rel_to_path;

    fn touch(root: &Path, rel: &str, body: &str) {
        let p = rel_to_path(root, rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn walk_default(root: &Path, config: &ZatlasConfig) -> WalkOutcome {
        let opts = WalkOptions {
            threads: 2,
            ..Default::default()
        };
        walk(root, config, &opts, &Arc::new(AtomicBool::new(false))).unwrap()
    }

    fn paths(out: &WalkOutcome) -> Vec<&str> {
        out.files.iter().map(|f| f.rel_path.as_str()).collect()
    }

    #[test]
    fn finds_source_files_and_ignores_everything_else() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "src/a.ts", "export const a = 1;");
        touch(p, "src/b.rs", "pub fn b() {}");
        touch(p, "README.md", "# hi");
        touch(p, "logo.png", "not source");

        let out = walk_default(p, &ZatlasConfig::default());
        assert_eq!(paths(&out), ["src/a.ts", "src/b.rs"]);
    }

    #[test]
    fn results_are_sorted_so_file_ids_are_stable_across_scans() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        for name in ["z", "m", "a", "q", "b"] {
            touch(p, &format!("src/{name}.ts"), "export {};");
        }
        let first = paths(&walk_default(p, &ZatlasConfig::default()))
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let second = paths(&walk_default(p, &ZatlasConfig::default()))
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        assert_eq!(first, second);
        assert_eq!(
            first,
            ["src/a.ts", "src/b.ts", "src/m.ts", "src/q.ts", "src/z.ts"]
        );
    }

    #[test]
    fn never_descends_into_node_modules_or_target_even_without_a_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "src/a.ts", "x");
        touch(p, "node_modules/lodash/index.js", "x");
        touch(p, "target/debug/build.rs", "x");
        touch(p, ".git/hooks/pre-commit.py", "x");

        let out = walk_default(p, &ZatlasConfig::default());
        assert_eq!(paths(&out), ["src/a.ts"]);
    }

    #[test]
    fn honours_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, ".gitignore", "generated/\n*.gen.ts\n");
        touch(p, "src/a.ts", "x");
        touch(p, "generated/b.ts", "x");
        touch(p, "src/c.gen.ts", "x");

        let out = walk_default(p, &ZatlasConfig::default());
        assert_eq!(paths(&out), ["src/a.ts"]);
    }

    #[test]
    fn include_acts_as_a_whitelist_and_exclude_beats_it() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "src/a.ts", "x");
        touch(p, "src/a.test.ts", "x");
        touch(p, "scripts/tool.ts", "x");

        let config = ZatlasConfig {
            include: vec!["src/**".into()],
            exclude: vec!["**/*.test.ts".into()],
            ..Default::default()
        };
        let out = walk_default(p, &config);
        assert_eq!(paths(&out), ["src/a.ts"]);
        assert!(out
            .skipped
            .iter()
            .any(|s| s.rel_path == "src/a.test.ts" && s.reason == SkipReason::Excluded));
    }

    #[test]
    fn a_bad_glob_names_the_pattern_rather_than_failing_opaquely() {
        let dir = tempfile::tempdir().unwrap();
        let config = ZatlasConfig {
            include: vec!["src/[".into()],
            ..Default::default()
        };
        let err = walk(
            dir.path(),
            &config,
            &WalkOptions::default(),
            &Arc::new(AtomicBool::new(false)),
        )
        .unwrap_err();
        assert!(err.to_string().contains("src/["), "got: {err}");
    }

    #[test]
    fn oversized_files_are_skipped_with_their_size_reported() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "src/small.ts", "x");
        touch(p, "src/huge.ts", &"y".repeat(5000));

        let opts = WalkOptions {
            max_file_bytes: 1000,
            threads: 2,
            ..Default::default()
        };
        let out = walk(
            p,
            &ZatlasConfig::default(),
            &opts,
            &Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(paths(&out), ["src/small.ts"]);
        assert!(matches!(
            out.skipped[0].reason,
            SkipReason::TooLarge {
                bytes: 5000,
                limit: 1000
            }
        ));
    }

    #[test]
    fn a_file_with_a_source_extension_but_binary_content_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join("src")).unwrap();
        std::fs::write(p.join("src/blob.ts"), [0x00, 0x01, 0x02, 0x00]).unwrap();
        touch(p, "src/real.ts", "export const x = 1;");

        let out = walk_default(p, &ZatlasConfig::default());
        assert_eq!(paths(&out), ["src/real.ts"]);
        assert_eq!(out.skipped[0].reason, SkipReason::Binary);
    }

    #[test]
    fn cancelling_stops_the_walk() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        for i in 0..200 {
            touch(p, &format!("src/f{i}.ts"), "x");
        }
        let cancel = Arc::new(AtomicBool::new(true));
        let out = walk(
            p,
            &ZatlasConfig::default(),
            &WalkOptions::default(),
            &cancel,
        )
        .unwrap();
        assert!(
            out.files.len() < 200,
            "expected an early stop, got {}",
            out.files.len()
        );
    }

    #[test]
    fn manifests_are_collected_separately_from_source() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "package.json", "{}");
        touch(p, "tsconfig.json", "{}");
        touch(p, "Cargo.toml", "");
        touch(p, "packages/ui/package.json", "{}");
        touch(p, "src/a.ts", "x");

        let out = walk_default(p, &ZatlasConfig::default());
        assert_eq!(paths(&out), ["src/a.ts"]);
        assert_eq!(
            out.manifests,
            [
                "Cargo.toml",
                "package.json",
                "packages/ui/package.json",
                "tsconfig.json"
            ]
        );
    }

    #[test]
    fn a_config_excluded_by_a_glob_is_still_read_for_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "tsconfig.json", "{}");
        touch(p, "src/a.ts", "x");
        let config = ZatlasConfig {
            include: vec!["src/**".into()],
            ..Default::default()
        };
        let out = walk_default(p, &config);
        assert_eq!(out.manifests, ["tsconfig.json"]);
    }

    #[test]
    fn rel_to_path_round_trips_through_the_platform_separator() {
        let root = Path::new("/repo");
        let joined = rel_to_path(root, "src/deep/file.ts");
        assert_eq!(joined.file_name().unwrap(), "file.ts");
        assert_eq!(joined.parent().unwrap().file_name().unwrap(), "deep");
        assert_eq!(
            to_rel_string(root, &joined).as_deref(),
            Some("src/deep/file.ts")
        );
    }

    #[test]
    fn paths_use_forward_slashes_so_the_graph_is_identical_on_every_platform() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "a/b/c/deep.ts", "x");
        let out = walk_default(dir.path(), &ZatlasConfig::default());
        assert_eq!(paths(&out), ["a/b/c/deep.ts"]);
    }
}
