use tauri::State;
use zatlas_core::findings::FindingKind;
use zatlas_core::model::FileId;

use crate::dto::{
    file_name, heat_of, node_ref, EdgeDto, GraphPayload, ImpactResult, LayoutPayload, ModuleDto,
    NodeDetail, NodeDto, UnresolvedDto,
};
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
pub async fn get_graph(state: State<'_, AppState>) -> Response<GraphPayload> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;
    let g = &loaded.analysis.graph;

    let mut heat = vec![0.0f32; g.files.len()];
    for f in &loaded.findings {
        let h = heat_of(f.severity);
        for id in &f.files {
            if let Some(slot) = heat.get_mut(id.0 as usize) {
                *slot = slot.max(h);
            }
        }
    }

    let nodes = g
        .files
        .iter()
        .map(|f| {
            let h = loaded.history.get(&f.path);
            NodeDto {
                id: f.id.0,
                path: f.path.clone(),
                name: file_name(&f.path),
                module: f.module.0,
                language: f.language,
                loc: f.loc,
                fan_in: loaded.deps.fan_in(f.id) as u32,
                fan_out: loaded.deps.fan_out(f.id) as u32,
                churn: h.map(|x| x.commits).unwrap_or(0),
                authors: h.map(|x| x.authors).unwrap_or(0),
                heat: heat[f.id.0 as usize],
            }
        })
        .collect();

    let edges = g
        .edges
        .iter()
        .map(|e| EdgeDto {
            from: e.from.0,
            to: e.to.0,
            kind: e.kind,
            weight: e.weight,
        })
        .collect();

    let modules = g
        .modules
        .iter()
        .map(|m| ModuleDto {
            id: m.id.0,
            path: m.path.clone(),
            name: if m.path.is_empty() {
                "(root)".to_string()
            } else {
                file_name(&m.path)
            },
            file_count: m.files.len(),
            loc: m
                .files
                .iter()
                .filter_map(|f| g.file(*f))
                .map(|f| f.loc)
                .sum(),
        })
        .collect();

    Ok(GraphPayload {
        nodes,
        edges,
        modules,
    })
}

#[tauri::command]
pub async fn get_layout(state: State<'_, AppState>) -> Response<LayoutPayload> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;
    let l = &loaded.layout;
    Ok(LayoutPayload {
        xy: l.xy.clone(),
        radius: l.radius.clone(),
        min_x: l.min_x,
        min_y: l.min_y,
        max_x: l.max_x,
        max_y: l.max_y,
    })
}

#[tauri::command]
pub async fn get_node(state: State<'_, AppState>, id: u32) -> Response<NodeDetail> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;
    let g = &loaded.analysis.graph;
    let file_id = FileId(id);

    let f = g.file(file_id).ok_or_else(|| {
        CommandError::new(
            "no_such_node",
            format!("No file with id {id}"),
            "Re-scan the repository; the selection is from an older scan.",
            false,
        )
    })?;

    let h = loaded.history.get(&f.path);
    let parsed = loaded.analysis.parsed.get(&file_id);

    Ok(NodeDetail {
        id: f.id.0,
        path: f.path.clone(),
        language: f.language,
        loc: f.loc,
        bytes: f.bytes,
        module: g
            .module(f.module)
            .map(|m| {
                if m.path.is_empty() {
                    "(root)".to_string()
                } else {
                    m.path.clone()
                }
            })
            .unwrap_or_default(),
        fan_in: loaded.deps.fan_in(file_id) as u32,
        fan_out: loaded.deps.fan_out(file_id) as u32,
        churn: h.map(|x| x.commits).unwrap_or(0),
        authors: h.map(|x| x.authors).unwrap_or(0),
        top_author: h.map(|x| x.top_author.clone()).unwrap_or_default(),
        top_author_share: h.map(|x| x.top_author_share).unwrap_or(0.0),
        last_touched: h.map(|x| x.last_touched).unwrap_or(0),
        exports: parsed
            .map(|p| {
                let mut names: Vec<String> = p.exports.iter().map(|e| e.name.clone()).collect();
                names.sort();
                names.dedup();
                names
            })
            .unwrap_or_default(),
        definitions: parsed.map(|p| p.definitions.len() as u32).unwrap_or(0),
        dependencies: loaded
            .deps
            .dependencies(file_id)
            .iter()
            .filter_map(|&d| node_ref(loaded, d))
            .collect(),
        dependents: loaded
            .deps
            .dependents(file_id)
            .iter()
            .filter_map(|&d| node_ref(loaded, d))
            .collect(),
        unresolved: g
            .unresolved
            .iter()
            .filter(|u| u.from == file_id)
            .map(UnresolvedDto::of)
            .collect(),
        findings: loaded
            .findings
            .iter()
            .filter(|x| x.files.contains(&file_id))
            .cloned()
            .collect(),
    })
}

#[tauri::command]
pub async fn impact(state: State<'_, AppState>, ids: Vec<u32>) -> Response<ImpactResult> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;
    let seeds: Vec<FileId> = ids.iter().map(|&i| FileId(i)).collect();
    let affected = zatlas_core::graph::impact(&loaded.deps, &seeds);
    let total_files = loaded.analysis.graph.files.len().max(1);

    Ok(ImpactResult {
        seeds: ids,
        total: affected.len(),
        share: affected.len() as f32 / total_files as f32,
        affected: affected.iter().map(|f| f.0).collect(),
    })
}

#[tauri::command]
pub async fn dsm_order(state: State<'_, AppState>) -> Response<Vec<u32>> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;
    Ok(zatlas_core::graph::dsm_order(&loaded.deps)
        .iter()
        .map(|f| f.0)
        .collect())
}

#[tauri::command]
pub async fn get_symbols(
    state: State<'_, AppState>,
    module: u32,
) -> Response<zatlas_core::symbols::SymbolGraph> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;
    let g = &loaded.analysis.graph;

    let module_node = g
        .module(zatlas_core::model::ModuleId(module))
        .ok_or_else(|| {
            CommandError::new(
                "no_such_module",
                format!("No module with id {module}"),
                "Re-scan the repository; the selection is from an older scan.",
                false,
            )
        })?;

    let root = std::path::Path::new(&loaded.info.root);
    let files: Vec<(FileId, String, zatlas_core::model::Language, String)> = module_node
        .files
        .iter()
        .filter_map(|id| {
            let f = g.file(*id)?;
            let source =
                std::fs::read_to_string(zatlas_core::paths::rel_to_path(root, &f.path)).ok()?;
            Some((f.id, f.path.clone(), f.language, source))
        })
        .collect();

    Ok(zatlas_core::symbols::build(&files))
}

#[tauri::command]
pub async fn get_tour(state: State<'_, AppState>) -> Response<Vec<zatlas_core::tour::TourStop>> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;
    let entrypoints = zatlas_core::findings::entrypoints_of(&loaded.analysis.graph, &loaded.config);
    Ok(zatlas_core::tour::build(
        &loaded.analysis.graph,
        &entrypoints,
        &loaded.history,
        &zatlas_core::tour::TourOptions::default(),
    ))
}

#[tauri::command]
pub async fn cycle_members(state: State<'_, AppState>) -> Response<Vec<Vec<u32>>> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;
    Ok(loaded
        .findings
        .iter()
        .filter(|f| f.kind == FindingKind::Cycle)
        .map(|f| f.files.iter().map(|x| x.0).collect())
        .collect())
}

#[tauri::command]
pub async fn edge_detail(
    state: State<'_, AppState>,
    from: u32,
    to: u32,
) -> Response<crate::dto::EdgeDetail> {
    let guard = state.loaded.read().expect("loaded lock poisoned");
    let loaded = guard.as_ref().ok_or_else(not_loaded)?;
    let g = &loaded.analysis.graph;

    let from_id = zatlas_core::model::FileId(from);
    let to_id = zatlas_core::model::FileId(to);
    let from_node = g.file(from_id).ok_or_else(|| unknown_node(from))?;
    let to_node = g.file(to_id).ok_or_else(|| unknown_node(to))?;

    let kind = g
        .edges
        .iter()
        .find(|e| e.from == from_id && e.to == to_id)
        .map(|e| e.kind)
        .ok_or_else(|| {
            CommandError::new(
                "no_such_edge",
                format!("{} does not import {}", from_node.path, to_node.path),
                "Re-scan; the graph may have changed since this was drawn.",
                true,
            )
        })?;

    let parsed = loaded.analysis.parsed.get(&from_id);
    let mut sites: Vec<crate::dto::ImportSite> = g
        .edge_sites
        .iter()
        .filter(|s| s.from == from_id && s.to == to_id)
        .filter_map(|s| {
            let import = parsed?.imports.get(s.import as usize)?;
            Some(crate::dto::ImportSite {
                line: import.line,
                specifier: import.specifier.clone(),
                kind: match import.kind {
                    zatlas_core::parse::ImportKind::Static => "import",
                    zatlas_core::parse::ImportKind::ReExport => "re-export",
                    zatlas_core::parse::ImportKind::Dynamic => "dynamic",
                    zatlas_core::parse::ImportKind::Require => "require",
                    zatlas_core::parse::ImportKind::ModDecl => "mod",
                }
                .to_owned(),
                type_only: import.type_only,
            })
        })
        .collect();

    sites.sort_by_key(|s| s.line);

    Ok(crate::dto::EdgeDetail {
        from,
        to,
        from_path: from_node.path.clone(),
        to_path: to_node.path.clone(),
        kind,
        sites,
    })
}

fn unknown_node(id: u32) -> CommandError {
    CommandError::new(
        "no_such_node",
        format!("File {id} is not in this scan"),
        "Re-scan and try again.",
        true,
    )
}
