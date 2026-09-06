use tauri::State;
use tauri_plugin_dialog::DialogExt;
use zatlas_core::repo::{self, RepoInfo};

use crate::error::{CommandError, Response};
use crate::state::AppState;

#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StartupArgs {
    pub path: Option<String>,

    pub view: Option<String>,
}

#[tauri::command]
pub async fn startup_args() -> Response<StartupArgs> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let path = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .filter(|a| std::path::Path::new(a).is_dir())
        .cloned();

    let view = args
        .iter()
        .position(|a| a == "--view")
        .and_then(|i| args.get(i + 1))
        .filter(|v| matches!(v.as_str(), "graph" | "treemap" | "matrix" | "timeline"))
        .cloned();

    Ok(StartupArgs { path, view })
}

#[tauri::command]
pub async fn open_repo(path: String) -> Response<RepoInfo> {
    Ok(repo::open(std::path::Path::new(&path))?)
}

#[tauri::command]
pub async fn pick_repo(app: tauri::AppHandle) -> Response<Option<String>> {
    let picked = app.dialog().file().blocking_pick_folder();
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn has_analysis(state: State<'_, AppState>) -> Response<bool> {
    Ok(state.loaded.read().expect("loaded lock poisoned").is_some())
}

#[tauri::command]
pub async fn open_in_editor(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Response<()> {
    use tauri_plugin_opener::OpenerExt;

    let root = {
        let guard = state.loaded.read().expect("loaded lock poisoned");
        guard.as_ref().map(|l| l.info.root.clone())
    };
    let full = match root {
        Some(r) => zatlas_core::paths::rel_to_path(std::path::Path::new(&r), &path),
        None => std::path::PathBuf::from(&path),
    };

    app.opener()
        .open_path(full.to_string_lossy(), None::<&str>)
        .map_err(|e| {
            CommandError::new(
                "open_failed",
                e,
                "Set a default application for this file type, or open it manually.",
                true,
            )
        })
}
