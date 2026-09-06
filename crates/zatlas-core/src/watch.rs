use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebouncedEvent, Debouncer};

use crate::error::{CoreError, Result};
use crate::model::Language;

pub struct Watch {
    _debouncer: Debouncer<notify::RecommendedWatcher, notify_debouncer_full::RecommendedCache>,
}

pub const SETTLE: Duration = Duration::from_millis(400);

const SKIP: &[&str] = &[
    ".git",
    ".zatlas",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    "__pycache__",
    ".venv",
    "venv",
    "coverage",
];

pub fn is_interesting(path: &Path) -> bool {
    let text = path.to_string_lossy();
    for part in text.split(['/', '\\']) {
        if SKIP.contains(&part) {
            return false;
        }
    }

    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.ends_with('~') || name.starts_with(".#") || name.ends_with(".tmp") {
            return false;
        }
        if crate::walk::is_manifest(name) {
            return true;
        }
    }
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(Language::from_extension)
        .is_some()
}

pub fn watch<F>(root: &Path, on_change: F) -> Result<Watch>
where
    F: Fn(Vec<PathBuf>) + Send + 'static,
{
    let (tx, rx) = mpsc::channel();

    let mut debouncer =
        new_debouncer(SETTLE, None, tx).map_err(|e| CoreError::other(format!("watch: {e}")))?;

    debouncer
        .watch(root, RecursiveMode::Recursive)
        .map_err(|e| CoreError::other(format!("watch {}: {e}", root.display())))?;

    std::thread::spawn(move || {
        for result in rx {
            let Ok(events) = result else { continue };
            let paths = interesting_paths(&events);
            if !paths.is_empty() {
                on_change(paths);
            }
        }
    });

    Ok(Watch {
        _debouncer: debouncer,
    })
}

pub fn interesting_paths(events: &[DebouncedEvent]) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = events
        .iter()
        .flat_map(|e| e.paths.iter().cloned())
        .filter(|p| is_interesting(p))
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn source_files_are_interesting() {
        for path in [
            "src/a.ts",
            "src/b.rs",
            "app/views.py",
            "main.go",
            "src/c.tsx",
        ] {
            assert!(is_interesting(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn manifests_are_interesting_because_they_change_resolution() {
        for path in ["tsconfig.json", "Cargo.toml", "package.json", "go.mod"] {
            assert!(is_interesting(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn build_output_is_never_interesting() {
        for path in [
            "target/debug/thing.rs",
            "node_modules/lib/index.js",
            "dist/bundle.js",
            ".git/COMMIT_EDITMSG",
            ".zatlas/cache.sqlite",
            "__pycache__/mod.py",
        ] {
            assert!(!is_interesting(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn editor_scratch_files_are_ignored() {
        for path in ["src/a.ts~", "src/.#a.ts", "src/a.ts.tmp"] {
            assert!(!is_interesting(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn non_source_files_are_ignored() {
        for path in ["README.md", "logo.png", "LICENSE"] {
            assert!(!is_interesting(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn a_nested_skip_directory_is_still_skipped() {
        assert!(!is_interesting(Path::new("crates/foo/target/x.rs")));
        assert!(!is_interesting(Path::new("apps/web/node_modules/a/b.ts")));
    }

    #[test]
    fn watching_a_real_directory_reports_a_source_change() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.ts"), "export const a = 1;").unwrap();

        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        let _watch = watch(dir.path(), move |paths| {
            if paths
                .iter()
                .any(|p| p.extension().is_some_and(|e| e == "ts"))
            {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        })
        .unwrap();

        std::fs::write(dir.path().join("a.ts"), "export const a = 2;").unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        while hits.load(Ordering::Relaxed) == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(hits.load(Ordering::Relaxed) > 0, "no change was reported");
    }

    #[test]
    fn dropping_the_watch_stops_it() {
        let dir = tempfile::tempdir().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        {
            let _watch = watch(dir.path(), move |_| {
                counter.fetch_add(1, Ordering::Relaxed);
            })
            .unwrap();
        }
        std::fs::write(dir.path().join("a.ts"), "x").unwrap();
        std::thread::sleep(SETTLE + Duration::from_millis(400));
        assert_eq!(
            hits.load(Ordering::Relaxed),
            0,
            "a dropped watch must be silent"
        );
    }

    #[test]
    fn watching_a_path_that_does_not_exist_is_an_error_not_a_panic() {
        let err = match watch(Path::new("/definitely/not/here"), |_| {}) {
            Err(e) => e,
            Ok(_) => panic!("watching a missing path should fail"),
        };
        assert!(err.to_string().contains("watch"), "got {err}");
    }
}
