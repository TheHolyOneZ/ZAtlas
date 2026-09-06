use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::State;
use zatlas_core::cache::Cache;
use zatlas_core::git::timetravel::{build_timeline, diff, Snapshot, SnapshotDiff, TimelineOptions};
use zatlas_core::model::Language;

use crate::error::{CommandError, Response};
use crate::state::AppState;

fn not_loaded() -> CommandError {
    CommandError::new(
        "not_loaded",
        "No analysis is loaded",
        "Open a repository and let the scan finish first.",
        false,
    )
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Timeline {
    pub snapshots: Vec<Snapshot>,

    pub diffs: Vec<SnapshotDiff>,
}

#[tauri::command]
pub async fn get_timeline(state: State<'_, AppState>, steps: Option<usize>) -> Response<Timeline> {
    let (root, cache_path) = {
        let guard = state.loaded.read().expect("loaded lock poisoned");
        let loaded = guard.as_ref().ok_or_else(not_loaded)?;
        if !loaded.info.is_git {
            return Err(CommandError::new(
                "not_a_git_repo",
                "This folder is not a git repository",
                "Time travel needs history. The structural views work without it.",
                false,
            ));
        }
        let root = std::path::PathBuf::from(&loaded.info.root);
        let cache = zatlas_core::config::cache_db_path(&root);
        (root, cache)
    };

    let opts = TimelineOptions {
        steps: steps.unwrap_or(12).clamp(2, 60),
        window_days: 365,
    };

    let result = tauri::async_runtime::spawn_blocking(move || {
        let cache = Cache::open(&cache_path).ok();
        build_timeline(
            &root,
            &opts,
            cache.as_ref(),
            &|path: &str| {
                std::path::Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .and_then(Language::from_extension)
                    .is_some()
            },
            &|path: &str| {
                std::path::Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .and_then(Language::from_extension)
            },
            &Arc::new(AtomicBool::new(false)),
        )
    })
    .await
    .map_err(|e| {
        CommandError::new(
            "timeline_panicked",
            e,
            "Please report this with the repository size.",
            false,
        )
    })?;

    let (snapshots, per_snapshot) = result?;

    let diffs: Vec<SnapshotDiff> = per_snapshot
        .windows(2)
        .map(|w| diff(&w[0], &w[1]))
        .collect();

    let _: HashMap<String, u32> = HashMap::new();
    Ok(Timeline { snapshots, diffs })
}

#[tauri::command]
pub async fn compare_refs(
    state: State<'_, AppState>,
    base: String,
    head: String,
) -> Response<zatlas_core::git::compare::RefComparison> {
    let root = {
        let guard = state.loaded.read().expect("loaded lock poisoned");
        let loaded = guard.as_ref().ok_or_else(not_loaded)?;
        if !loaded.info.is_git {
            return Err(CommandError::new(
                "not_a_git_repo",
                "This folder is not a git repository",
                "Comparing branches needs history. The structural views work without it.",
                false,
            ));
        }
        std::path::PathBuf::from(&loaded.info.root)
    };

    let result = tauri::async_runtime::spawn_blocking(move || {
        zatlas_core::git::compare::compare(&root, &base, &head, &Arc::new(AtomicBool::new(false)))
    })
    .await
    .map_err(|e| {
        CommandError::new(
            "compare_panicked",
            e,
            "Please report this with the two refs you compared.",
            false,
        )
    })?;

    Ok(result?)
}

#[tauri::command]
pub async fn list_refs(state: State<'_, AppState>) -> Response<Vec<String>> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;
    if !loaded.info.is_git {
        return Ok(Vec::new());
    }
    let root = std::path::Path::new(&loaded.info.root);

    let mut names = zatlas_core::git::compare::branches(root).unwrap_or_default();

    if let Some(current) = &loaded.info.branch {
        names.sort_by_key(|n| n != current);
    }
    Ok(names)
}
