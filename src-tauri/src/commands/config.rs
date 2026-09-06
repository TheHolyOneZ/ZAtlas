use tauri::State;
use zatlas_core::config::{Layer, ZatlasConfig};

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

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Response<ZatlasConfig> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    Ok(guard.as_ref().ok_or_else(not_loaded)?.config.clone())
}

#[tauri::command]
pub async fn layer_candidates(state: State<'_, AppState>) -> Response<Vec<(String, usize)>> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;

    let mut counts: std::collections::HashMap<String, usize> = Default::default();
    for file in &loaded.analysis.graph.files {
        let parts: Vec<&str> = file.path.split('/').collect();
        let key = match parts.len() {
            0 | 1 => continue,
            2 => parts[0].to_owned(),
            _ => format!("{}/{}", parts[0], parts[1]),
        };
        *counts.entry(key).or_insert(0) += 1;
    }

    let mut out: Vec<(String, usize)> = counts.into_iter().collect();

    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out.truncate(40);
    Ok(out)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerSpec {
    pub name: String,
    pub paths: Vec<String>,
}

#[tauri::command]
pub async fn write_layers(
    state: State<'_, AppState>,
    layers: Vec<LayerSpec>,
    rules: Vec<String>,
) -> Response<String> {
    let mut guard = state.loaded.write().expect("loaded lock poisoned");
    let loaded = guard.as_mut().ok_or_else(not_loaded)?;
    let root = std::path::PathBuf::from(&loaded.info.root);

    let config = ZatlasConfig {
        layers: layers
            .into_iter()
            .map(|l| Layer {
                name: l.name,
                paths: l.paths,
                rules: Vec::new(),
            })
            .collect(),
        rules,

        include: loaded.config.include.clone(),
        exclude: loaded.config.exclude.clone(),
        entrypoints: loaded.config.entrypoints.clone(),
        lsp: loaded.config.lsp.clone(),
    };

    let path = config.save(&root)?;

    loaded.config = config;
    loaded.info.has_config = true;
    Ok(path.to_string_lossy().into_owned())
}
