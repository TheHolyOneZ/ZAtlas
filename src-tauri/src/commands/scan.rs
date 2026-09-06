use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use zatlas_core::analyse::{analyse, AnalyseOptions, Phase};
use zatlas_core::config::ZatlasConfig;
use zatlas_core::findings::{detect_all, FindingsOptions};
use zatlas_core::git::{coupling, history, CouplingOptions, HistoryOptions};
use zatlas_core::graph::Adjacency;
use zatlas_core::layout::{layout, LayoutOptions};
use zatlas_core::lsp::{self, LspMode, LspReport, LspSettings};
use zatlas_core::repo;

use crate::dto::{ScanProgress, ScanSummary};
use crate::error::{CommandError, Response};
use crate::events;
use crate::state::{AppState, Loaded, ScanResult};

#[tauri::command]
pub async fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Response<String> {
    let info = repo::open(std::path::Path::new(&path))?;
    let root = std::path::PathBuf::from(&info.root);

    state.cancel_all();

    let scan_id = uuid::Uuid::new_v4().to_string();
    let cancel = state.register_scan(&scan_id);

    let id_for_task = scan_id.clone();
    let app_for_task = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let outcome = run_scan(&app_for_task, &id_for_task, root, info, &cancel);

        let state = tauri::Manager::state::<AppState>(&app_for_task);
        state.finish_scan(&id_for_task);

        if cancel.load(Ordering::Relaxed) {
            let _ = app_for_task.emit(events::SCAN_CANCELLED, &id_for_task);
            return;
        }

        match outcome {
            Ok(loaded) => {
                let summary = ScanSummary::of(&loaded);
                *state.loaded.write().expect("loaded lock poisoned") = Some(loaded);
                let _ = app_for_task.emit(events::SCAN_DONE, summary);
            }
            Err(e) => {
                let _ = app_for_task.emit(events::SCAN_FAILED, e);
            }
        }
    });

    Ok(scan_id)
}

fn run_scan(
    app: &AppHandle,
    scan_id: &str,
    root: std::path::PathBuf,
    info: zatlas_core::repo::RepoInfo,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<Loaded, CommandError> {
    let config = ZatlasConfig::load(&root)?;

    let last = std::sync::Mutex::new(std::time::Instant::now());
    let emit = |phase: Phase, done: usize, total: usize| {
        let label = match phase {
            Phase::Walking => "walking",
            Phase::Parsing => "parsing",
            Phase::Indexing => "indexing",
            Phase::Resolving => "resolving",
        };
        let mut guard = last.lock().expect("progress clock poisoned");
        let force = done == 0 || done == total;
        if !force && guard.elapsed().as_millis() < 80 {
            return;
        }
        *guard = std::time::Instant::now();
        let _ = app.emit(
            events::SCAN_PROGRESS,
            ScanProgress {
                scan_id: scan_id.to_string(),
                phase: label.to_string(),
                done,
                total,
            },
        );
    };

    let mut analysis = analyse(&root, &config, &AnalyseOptions::default(), cancel, &emit)?;

    let lsp_report = run_lsp(app, scan_id, &root, &mut analysis, &config);

    let _ = app.emit(events::GIT_PROGRESS, scan_id);
    let (hist_map, pairs) = match history(&root, &HistoryOptions::default(), cancel) {
        Ok(h) => {
            let pairs = coupling(&h.commits, &CouplingOptions::default());
            (h.files, pairs)
        }
        Err(_) => (HashMap::new(), Vec::new()),
    };

    let findings = detect_all(
        &analysis.graph,
        &config,
        &hist_map,
        &pairs,
        &FindingsOptions::default(),
    );

    let adj = Adjacency::build(analysis.graph.files.len(), &analysis.graph.edges);
    let positions = layout(&analysis.graph, &adj, &LayoutOptions::default());
    let _ = app.emit(events::LAYOUT_DONE, scan_id);

    Ok(Loaded::new(ScanResult {
        info,
        config,
        analysis,
        layout: positions,
        findings,
        history: hist_map,
        coupling: pairs,
        lsp: lsp_report,
    }))
}

fn run_lsp(
    app: &AppHandle,
    scan_id: &str,
    root: &std::path::Path,
    analysis: &mut zatlas_core::analyse::Analysis,
    config: &ZatlasConfig,
) -> LspReport {
    let state = tauri::Manager::state::<AppState>(app);
    let override_mode = *state.lsp_mode.read().expect("lsp mode lock poisoned");
    let settings = LspSettings {
        mode: override_mode.unwrap_or(config.lsp.mode),
        ..config.lsp.clone()
    };
    if settings.mode == LspMode::Off {
        return LspReport::default();
    }

    let _ = app.emit(
        events::SCAN_PROGRESS,
        ScanProgress {
            scan_id: scan_id.to_string(),
            phase: "language server".to_string(),
            done: 0,
            total: 0,
        },
    );
    lsp::apply(root, &mut analysis.graph, &analysis.parsed, &settings).unwrap_or_default()
}

#[tauri::command]
pub async fn set_lsp_mode(state: State<'_, AppState>, mode: Option<LspMode>) -> Response<()> {
    *state.lsp_mode.write().expect("lsp mode lock poisoned") = mode;
    Ok(())
}

#[tauri::command]
pub async fn lsp_servers(state: State<'_, AppState>) -> Response<Vec<crate::dto::LspServerDto>> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let (root, languages) = match guard.as_ref() {
        Some(l) => {
            let mut languages: Vec<zatlas_core::Language> =
                l.analysis.graph.files.iter().map(|f| f.language).collect();
            languages.sort_by_key(|x| format!("{x:?}"));
            languages.dedup();
            (std::path::PathBuf::from(&l.info.root), languages)
        }
        None => (std::path::PathBuf::new(), Vec::new()),
    };

    Ok(zatlas_core::lsp::servers::KNOWN
        .iter()
        .map(|spec| crate::dto::LspServerDto {
            id: spec.id.to_string(),
            label: spec.label.to_string(),
            command: spec.command.to_string(),
            installed: zatlas_core::lsp::servers::find_on_path(spec.command).is_some(),
            applicable: !root.as_os_str().is_empty()
                && spec.languages.iter().any(|l| languages.contains(l))
                && (spec.root_markers.is_empty()
                    || spec.root_markers.iter().any(|m| root.join(m).exists())),
        })
        .collect())
}

#[tauri::command]
pub async fn start_workspace_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Response<String> {
    if paths.is_empty() {
        return Err(CommandError::new(
            "empty_workspace",
            "No repositories given",
            "Choose at least one folder.",
            false,
        ));
    }

    let roots: Vec<std::path::PathBuf> = paths.iter().map(std::path::PathBuf::from).collect();
    for root in &roots {
        repo::open(root)?;
    }

    state.cancel_all();
    let scan_id = uuid::Uuid::new_v4().to_string();
    let cancel = state.register_scan(&scan_id);

    let id_for_task = scan_id.clone();
    let app_for_task = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let outcome = run_workspace(&app_for_task, &id_for_task, roots, &cancel);
        let state = tauri::Manager::state::<AppState>(&app_for_task);
        state.finish_scan(&id_for_task);

        if cancel.load(Ordering::Relaxed) {
            let _ = app_for_task.emit(events::SCAN_CANCELLED, &id_for_task);
            return;
        }
        match outcome {
            Ok(loaded) => {
                let summary = ScanSummary::of(&loaded);
                *state.loaded.write().expect("loaded lock poisoned") = Some(loaded);
                let _ = app_for_task.emit(events::SCAN_DONE, summary);
            }
            Err(e) => {
                let _ = app_for_task.emit(events::SCAN_FAILED, e);
            }
        }
    });

    Ok(scan_id)
}

fn run_workspace(
    app: &AppHandle,
    scan_id: &str,
    roots: Vec<std::path::PathBuf>,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
) -> Result<Loaded, CommandError> {
    let total = roots.len();
    let _ = app.emit(
        events::SCAN_PROGRESS,
        ScanProgress {
            scan_id: scan_id.to_string(),
            phase: "scanning repositories".into(),
            done: 0,
            total,
        },
    );

    let ws = zatlas_core::workspace::analyse_workspace(
        &roots,
        &zatlas_core::analyse::AnalyseOptions::default(),
        cancel,
    )?;

    let names: Vec<String> = ws.members.iter().map(|m| m.name.clone()).collect();
    let info = zatlas_core::repo::RepoInfo {
        root: roots
            .iter()
            .map(|r| r.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(", "),
        name: format!("{} repositories", names.len()),
        branch: None,
        head: None,

        is_git: false,
        has_config: false,
        invalid_rules: Vec::new(),
    };

    let mut analysis = ws
        .members
        .into_iter()
        .next()
        .map(|m| m.analysis)
        .unwrap_or_else(|| zatlas_core::analyse::Analysis {
            graph: zatlas_core::model::Graph::default(),
            parsed: Default::default(),
            skipped: Vec::new(),
        });
    analysis.graph = ws.graph;

    let config = ZatlasConfig::default();
    let findings = detect_all(
        &analysis.graph,
        &config,
        &HashMap::new(),
        &[],
        &FindingsOptions::default(),
    );
    let adj = Adjacency::build(analysis.graph.files.len(), &analysis.graph.edges);
    let positions = layout(&analysis.graph, &adj, &LayoutOptions::default());

    Ok(Loaded::new(ScanResult {
        info,
        config,
        analysis,
        layout: positions,
        findings,
        history: HashMap::new(),
        coupling: Vec::new(),

        lsp: LspReport::default(),
    }))
}

#[tauri::command]
pub async fn set_watching(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Response<bool> {
    if !enabled {
        *state.watch.lock().expect("watch lock poisoned") = None;
        return Ok(false);
    }

    let root = {
        let guard = state.loaded.read().expect("loaded lock poisoned");
        match guard.as_ref() {
            Some(loaded) => std::path::PathBuf::from(&loaded.info.root),
            None => return Ok(false),
        }
    };

    let handle = app.clone();
    let watch = zatlas_core::watch::watch(&root, move |paths| {
        let names: Vec<String> = paths
            .iter()
            .take(20)
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let _ = handle.emit(events::REPO_CHANGED, names);
    })?;

    *state.watch.lock().expect("watch lock poisoned") = Some(watch);
    Ok(true)
}

#[tauri::command]
pub async fn cancel_scan(state: State<'_, AppState>, scan_id: String) -> Response<bool> {
    Ok(state.cancel_scan(&scan_id))
}

#[tauri::command]
pub async fn get_summary(state: State<'_, AppState>) -> Response<Option<ScanSummary>> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    Ok(guard.as_ref().map(ScanSummary::of))
}
