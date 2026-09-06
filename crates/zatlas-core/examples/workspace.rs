use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use zatlas_core::analyse::AnalyseOptions;
use zatlas_core::workspace::analyse_workspace;

fn main() {
    let roots: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: workspace <repo> [repo...]");
        return;
    }

    let started = Instant::now();
    let ws = analyse_workspace(
        &roots,
        &AnalyseOptions::default(),
        &Arc::new(AtomicBool::new(false)),
    )
    .expect("workspace");

    println!("{} members in {:?}", ws.members.len(), started.elapsed());
    for m in &ws.members {
        println!(
            "  {:<22} {:>4} files   publishes: {}",
            m.name,
            m.analysis.graph.files.len(),
            if m.packages.is_empty() {
                "(nothing)".to_string()
            } else {
                m.packages.join(", ")
            }
        );
    }

    println!(
        "\n  merged: {} files, {} edges",
        ws.graph.files.len(),
        ws.graph.edges.len()
    );

    let crossing: Vec<String> = ws
        .graph
        .edges
        .iter()
        .filter_map(|e| {
            let from = ws.graph.file(e.from)?;
            let to = ws.graph.file(e.to)?;
            let fm = from.path.split('/').next()?;
            let tm = to.path.split('/').next()?;
            (fm != tm).then(|| format!("{} -> {}", from.path, to.path))
        })
        .collect();

    println!("  cross-repository edges: {}", crossing.len());
    for e in crossing.iter().take(10) {
        println!("    {e}");
    }
}
