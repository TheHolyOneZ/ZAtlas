use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use zatlas_core::analyse::{analyse, AnalyseOptions, Analysis};
use zatlas_core::config::ZatlasConfig;
use zatlas_core::findings::{detect_all, Finding, FindingsOptions};
use zatlas_core::git::history::FileHistory;
use zatlas_core::git::{coupling, history, CoupledPair, CouplingOptions, HistoryOptions};
use zatlas_core::graph::{Adjacency, Relation};
use zatlas_core::repo::{self, RepoInfo};

pub struct Loaded {
    pub info: RepoInfo,
    pub config: ZatlasConfig,
    pub analysis: Analysis,
    pub deps: Adjacency,
    pub structural: Adjacency,
    pub findings: Vec<Finding>,
    pub history: HashMap<String, FileHistory>,
    pub coupling: Vec<CoupledPair>,
}

pub struct LoadOptions {
    pub use_cache: bool,

    pub git: bool,
}

pub fn load(path: &Path, opts: &LoadOptions) -> Result<Loaded, String> {
    let info = repo::open(path).map_err(|e| e.to_string())?;
    let root = PathBuf::from(&info.root);
    let config = ZatlasConfig::load(&root).map_err(|e| e.to_string())?;

    let cancel = Arc::new(AtomicBool::new(false));
    let analyse_opts = AnalyseOptions {
        use_cache: opts.use_cache,
        ..Default::default()
    };
    let analysis = analyse(&root, &config, &analyse_opts, &cancel, &|_, _, _| {})
        .map_err(|e| e.to_string())?;

    let (history_map, pairs) = if opts.git && info.is_git {
        match history(&root, &HistoryOptions::default(), &cancel) {
            Ok(h) => {
                let pairs = coupling(&h.commits, &CouplingOptions::default());
                (h.files, pairs)
            }
            Err(_) => (HashMap::new(), Vec::new()),
        }
    } else {
        (HashMap::new(), Vec::new())
    };

    let findings = detect_all(
        &analysis.graph,
        &config,
        &history_map,
        &pairs,
        &FindingsOptions::default(),
    );

    let n = analysis.graph.files.len();
    let deps = Adjacency::build(n, &analysis.graph.edges);
    let structural = Adjacency::build_for(n, &analysis.graph.edges, Relation::Structural);

    Ok(Loaded {
        info,
        config,
        analysis,
        deps,
        structural,
        findings,
        history: history_map,
        coupling: pairs,
    })
}

impl Loaded {
    pub fn path_of(&self, id: zatlas_core::model::FileId) -> &str {
        self.analysis
            .graph
            .file(id)
            .map(|f| f.path.as_str())
            .unwrap_or("?")
    }

    pub fn find_file(&self, needle: &str) -> Result<zatlas_core::model::FileId, String> {
        let normalised = needle.replace('\\', "/");
        let trimmed = normalised
            .strip_prefix(&format!("{}/", self.info.root.replace('\\', "/")))
            .unwrap_or(&normalised)
            .trim_start_matches("./");

        if let Some(f) = self.analysis.graph.files.iter().find(|f| f.path == trimmed) {
            return Ok(f.id);
        }
        let matches: Vec<&zatlas_core::model::FileNode> = self
            .analysis
            .graph
            .files
            .iter()
            .filter(|f| f.path.ends_with(trimmed))
            .collect();
        match matches.len() {
            0 => Err(format!("no file matching {needle:?} is in the scan")),
            1 => Ok(matches[0].id),
            n => Err(format!(
                "{needle:?} matches {n} files, including {} and {}",
                matches[0].path, matches[1].path
            )),
        }
    }
}
