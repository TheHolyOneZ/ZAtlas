use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use zatlas_core::analyse::{analyse, AnalyseOptions};
use zatlas_core::config::ZatlasConfig;
use zatlas_core::findings::{detect_all, FindingsOptions};
use zatlas_core::git::history::FileHistory;
use zatlas_core::git::{coupling, history, CouplingOptions, HistoryOptions};
use zatlas_core::graph::{tangles, Adjacency};
use zatlas_core::model::UnresolvedReason;

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".to_string()));
    let config = ZatlasConfig::load(&root).expect("config");
    let started = Instant::now();
    let a = analyse(
        &root,
        &config,
        &AnalyseOptions::default(),
        &Arc::new(AtomicBool::new(false)),
        &|_, _, _| {},
    )
    .expect("analyse");
    let elapsed = started.elapsed();

    let g = &a.graph;
    println!("{}", root.display());
    println!("  {:>7} files", g.files.len());
    println!("  {:>7} modules", g.modules.len());
    println!("  {:>7} edges", g.edges.len());
    println!(
        "  {:>7} LOC",
        g.files.iter().map(|f| f.loc as u64).sum::<u64>()
    );
    println!("  {:>7?} elapsed", elapsed);

    let mut external = 0usize;
    let mut no_such = 0usize;
    let mut alias = 0usize;
    let mut no_resolver = 0usize;
    let mut dynamic = 0usize;
    let mut deep = 0usize;
    let mut assets = 0usize;
    let mut excluded = 0usize;
    let mut no_tree = 0usize;
    for u in &g.unresolved {
        match u.reason {
            UnresolvedReason::External(_) => external += 1,
            UnresolvedReason::NoSuchFile(_) => no_such += 1,
            UnresolvedReason::UnmatchedAlias(_) => alias += 1,
            UnresolvedReason::NoResolverForLanguage(_) => no_resolver += 1,
            UnresolvedReason::DynamicSpecifier => dynamic += 1,
            UnresolvedReason::ReExportChainTooDeep(_) => deep += 1,
            UnresolvedReason::Asset(_) => assets += 1,
            UnresolvedReason::ExcludedFromScan(_) => excluded += 1,
            UnresolvedReason::FileNotInModuleTree => no_tree += 1,
        }
    }
    println!(
        "  unresolved: {} real ({} no-such-file, {} bad-alias, {} deep-chain), \
         {} external, {} assets, {} excluded, {} not-in-module-tree, {} no-resolver, {} dynamic",
        g.unresolved_failure_count(),
        no_such,
        alias,
        deep,
        external,
        assets,
        excluded,
        no_tree,
        no_resolver,
        dynamic
    );

    let adj = Adjacency::build(g.files.len(), &g.edges);
    let cycles = tangles(&adj);
    println!("  {:>7} cycles", cycles.len());
    for t in cycles.iter().take(5) {
        let chain: Vec<&str> = t
            .example
            .iter()
            .map(|id| g.file(*id).map(|f| f.path.as_str()).unwrap_or("?"))
            .collect();
        println!("      {} files: {}", t.members.len(), chain.join(" -> "));
    }

    let mut by_fan_in: Vec<_> = g.files.iter().map(|f| (adj.fan_in(f.id), f)).collect();
    by_fan_in.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    println!("  most depended upon:");
    for (n, f) in by_fan_in.iter().take(5) {
        println!("      {n:>4} dependents  {} ({} LOC)", f.path, f.loc);
    }

    let mut hist_map: std::collections::HashMap<String, FileHistory> = Default::default();
    let mut pairs_out = Vec::new();
    match history(
        &root,
        &HistoryOptions::default(),
        &Arc::new(AtomicBool::new(false)),
    ) {
        Ok(h) if h.total_commits > 0 => {
            println!("\n  history: {} commits in the last year", h.total_commits);
            let mut churn: Vec<_> = h.files.iter().collect();
            churn.sort_by_key(|(_, v)| std::cmp::Reverse(v.commits));
            println!("  churn hotspots:");
            for (path, fh) in churn.iter().take(5) {
                println!(
                    "      {:>4} commits  {:>2} authors  {:.0}% {:<14} {}",
                    fh.commits,
                    fh.authors,
                    fh.top_author_share * 100.0,
                    fh.top_author,
                    path
                );
            }
            let pairs = coupling(&h.commits, &CouplingOptions::default());
            hist_map = h.files.clone();
            pairs_out = pairs.clone();
            println!("  co-change coupling ({} pairs):", pairs.len());
            for p in pairs.iter().take(5) {
                println!(
                    "      {:.0}% ({}x)  {}  <->  {}",
                    p.confidence * 100.0,
                    p.together,
                    p.a,
                    p.b
                );
            }
        }
        Ok(_) => println!("\n  history: no commits"),
        Err(e) => println!("\n  history unavailable: {e}"),
    }

    let lay_started = Instant::now();
    let lay = zatlas_core::layout::layout(g, &adj, &zatlas_core::layout::LayoutOptions::default());
    println!(
        "  layout: {} nodes in {:?}  (box {:.0}x{:.0})",
        lay.len(),
        lay_started.elapsed(),
        lay.max_x - lay.min_x,
        lay.max_y - lay.min_y
    );

    let found = detect_all(
        g,
        &config,
        &hist_map,
        &pairs_out,
        &FindingsOptions::default(),
    );
    println!("\n  FINDINGS ({})", found.len());
    for f in found.iter().take(8) {
        println!("      [{:?}] {}", f.severity, f.headline);
    }

    let entry = zatlas_core::findings::entrypoints_of(g, &config);
    let tour = zatlas_core::tour::build(g, &entry, &hist_map, &Default::default());
    println!("\n  WHERE TO START ({} stops)", tour.len());
    for (i, s) in tour.iter().enumerate() {
        println!("    {}. {} ({} lines)", i + 1, s.path, s.loc);
        println!("       {}", s.reason);
    }

    if std::env::args().any(|a| a == "--export") {
        use std::collections::HashSet;
        let cycle_files: HashSet<_> = found
            .iter()
            .filter(|f| f.kind == zatlas_core::findings::FindingKind::Cycle)
            .flat_map(|f| f.files.iter().copied())
            .collect();
        println!("\n===== D2 =====");
        println!(
            "{}",
            zatlas_core::export::to_d2(g, &Default::default(), &cycle_files)
        );
        println!("===== MERMAID =====");
        println!(
            "{}",
            zatlas_core::export::to_mermaid(g, &Default::default(), &cycle_files)
        );
    }

    if no_such + alias > 0 {
        println!("\n  first real failures:");
        for u in g
            .unresolved
            .iter()
            .filter(|u| {
                matches!(
                    u.reason,
                    UnresolvedReason::NoSuchFile(_) | UnresolvedReason::UnmatchedAlias(_)
                )
            })
            .take(15)
        {
            println!(
                "    {}:{}  {}",
                g.file(u.from).map(|f| f.path.as_str()).unwrap_or("?"),
                u.line,
                u.specifier
            );
        }
    }
}
