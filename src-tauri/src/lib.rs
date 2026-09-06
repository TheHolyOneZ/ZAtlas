pub mod commands;
pub mod dto;
pub mod error;
pub mod events;
pub mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::repo::startup_args,
            commands::repo::open_repo,
            commands::repo::pick_repo,
            commands::repo::has_analysis,
            commands::repo::open_in_editor,
            commands::scan::start_scan,
            commands::scan::start_workspace_scan,
            commands::scan::cancel_scan,
            commands::scan::set_watching,
            commands::scan::get_summary,
            commands::scan::set_lsp_mode,
            commands::scan::lsp_servers,
            commands::graph::get_graph,
            commands::graph::get_layout,
            commands::graph::get_node,
            commands::graph::edge_detail,
            commands::graph::impact,
            commands::graph::dsm_order,
            commands::graph::get_tour,
            commands::graph::get_symbols,
            commands::graph::cycle_members,
            commands::export::export_text,
            commands::export::export_to_file,
            commands::export::save_image,
            commands::timeline::get_timeline,
            commands::timeline::compare_refs,
            commands::timeline::list_refs,
            commands::findings::get_findings,
            commands::findings::finding_counts,
            commands::config::get_config,
            commands::config::layer_candidates,
            commands::config::write_layers,
            commands::findings::accept_baseline,
            commands::findings::accept_finding,
            commands::findings::unaccept_finding,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ZAtlas");
}
