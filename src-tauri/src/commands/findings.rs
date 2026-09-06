use tauri::State;
use zatlas_core::findings::{Finding, FindingKind};

use crate::dto::FindingPage;
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
pub async fn get_findings(
    state: State<'_, AppState>,
    offset: usize,
    limit: usize,
    kinds: Option<Vec<FindingKind>>,
    query: Option<String>,
    include_accepted: Option<bool>,
) -> Response<FindingPage> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;

    let needle = query.map(|q| q.to_lowercase()).filter(|q| !q.is_empty());
    let show_accepted = include_accepted.unwrap_or(false) || loaded.accepted.is_empty();

    let filtered: Vec<(usize, &Finding)> = loaded
        .findings
        .iter()
        .enumerate()
        .filter(|(_, f)| show_accepted || !loaded.is_accepted(f))
        .filter(|(_, f)| kinds.as_ref().is_none_or(|k| k.contains(&f.kind)))
        .filter(|(_, f)| {
            needle
                .as_ref()
                .is_none_or(|q| f.headline.to_lowercase().contains(q))
        })
        .collect();

    let total = filtered.len();
    let page: Vec<(usize, &Finding)> = filtered
        .into_iter()
        .skip(offset)
        .take(limit.clamp(1, 500))
        .collect();

    Ok(FindingPage {
        total,
        indices: page.iter().map(|(i, _)| *i).collect(),
        is_accepted: page.iter().map(|(_, f)| loaded.is_accepted(f)).collect(),
        rows: page.into_iter().map(|(_, f)| f.clone()).collect(),
        accepted: loaded
            .findings
            .iter()
            .filter(|f| loaded.is_accepted(f))
            .count(),
    })
}

#[tauri::command]
pub async fn accept_finding(
    state: State<'_, AppState>,
    index: usize,
    note: Option<String>,
) -> Response<()> {
    let mut guard = state.loaded.write().expect("loaded lock poisoned");
    let loaded = guard.as_mut().ok_or_else(not_loaded)?;
    let root = std::path::PathBuf::from(&loaded.info.root);

    let finding = loaded.findings.get(index).cloned().ok_or_else(|| {
        CommandError::new(
            "no_such_finding",
            "That finding is no longer in the list",
            "Re-scan and try again.",
            true,
        )
    })?;

    let id = zatlas_core::baseline::fingerprint(&finding, &loaded.analysis.graph);
    let mut baseline = zatlas_core::baseline::load(&root)?.unwrap_or_else(|| {
        zatlas_core::baseline::from_findings(
            &[],
            &loaded.analysis.graph,
            &zatlas_core::baseline::today(),
        )
    });
    if !baseline.contains(&id) {
        let mut one = zatlas_core::baseline::from_findings(
            std::slice::from_ref(&finding),
            &loaded.analysis.graph,
            &baseline.created,
        );
        if let Some(entry) = one.entries.first_mut() {
            entry.note = note.filter(|n| !n.trim().is_empty());
        }
        baseline.entries.extend(one.entries);
        baseline.entries.sort_by(|a, b| a.id.cmp(&b.id));
    }
    zatlas_core::baseline::save(&root, &baseline)?;

    loaded.accepted = baseline.entries.iter().map(|e| e.id.clone()).collect();
    Ok(())
}

#[tauri::command]
pub async fn unaccept_finding(state: State<'_, AppState>, index: usize) -> Response<()> {
    let mut guard = state.loaded.write().expect("loaded lock poisoned");
    let loaded = guard.as_mut().ok_or_else(not_loaded)?;
    let root = std::path::PathBuf::from(&loaded.info.root);

    let finding = loaded.findings.get(index).cloned().ok_or_else(|| {
        CommandError::new(
            "no_such_finding",
            "That finding is no longer in the list",
            "Re-scan and try again.",
            true,
        )
    })?;
    let id = zatlas_core::baseline::fingerprint(&finding, &loaded.analysis.graph);

    let Some(mut baseline) = zatlas_core::baseline::load(&root)? else {
        return Ok(());
    };
    baseline.entries.retain(|e| e.id != id);
    zatlas_core::baseline::save(&root, &baseline)?;
    loaded.accepted = baseline.entries.iter().map(|e| e.id.clone()).collect();
    Ok(())
}

#[tauri::command]
pub async fn accept_baseline(state: State<'_, AppState>) -> Response<String> {
    let mut guard = state.loaded.write().expect("loaded lock poisoned");
    let loaded = guard.as_mut().ok_or_else(not_loaded)?;
    let root = std::path::PathBuf::from(&loaded.info.root);

    let baseline = zatlas_core::baseline::from_findings(
        &loaded.findings,
        &loaded.analysis.graph,
        &zatlas_core::baseline::today(),
    );
    zatlas_core::baseline::save(&root, &baseline)?;

    loaded.accepted = baseline.entries.iter().map(|e| e.id.clone()).collect();
    Ok(zatlas_core::baseline::path_of(&root)
        .to_string_lossy()
        .into_owned())
}

#[tauri::command]
pub async fn finding_counts(state: State<'_, AppState>) -> Response<Vec<(FindingKind, usize)>> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;

    let mut counts: std::collections::HashMap<FindingKind, usize> = Default::default();

    for f in loaded.findings.iter().filter(|f| !loaded.is_accepted(f)) {
        *counts.entry(f.kind).or_insert(0) += 1;
    }
    let mut out: Vec<(FindingKind, usize)> = counts.into_iter().collect();

    out.sort_by_key(|(k, _)| format!("{k:?}"));
    Ok(out)
}
