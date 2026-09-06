use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use std::collections::HashSet;

use zatlas_core::analyse::Analysis;
use zatlas_core::config::ZatlasConfig;
use zatlas_core::findings::Finding;
use zatlas_core::git::history::FileHistory;
use zatlas_core::git::CoupledPair;
use zatlas_core::graph::{Adjacency, Relation};
use zatlas_core::layout::Layout;
use zatlas_core::lsp::{LspMode, LspReport};
use zatlas_core::repo::RepoInfo;

pub type CancelFlag = Arc<AtomicBool>;

pub struct Loaded {
    pub info: RepoInfo,
    pub config: ZatlasConfig,
    pub analysis: Analysis,
    pub deps: Adjacency,
    pub structural: Adjacency,
    pub layout: Layout,
    pub findings: Vec<Finding>,
    pub history: HashMap<String, FileHistory>,
    pub coupling: Vec<CoupledPair>,

    pub scanned_at: u64,

    pub lsp: LspReport,

    pub accepted: HashSet<String>,
}

pub struct ScanResult {
    pub info: RepoInfo,
    pub config: ZatlasConfig,
    pub analysis: Analysis,
    pub layout: Layout,
    pub findings: Vec<Finding>,
    pub history: HashMap<String, FileHistory>,
    pub coupling: Vec<CoupledPair>,
    pub lsp: LspReport,
}

impl Loaded {
    pub fn new(parts: ScanResult) -> Self {
        let n = parts.analysis.graph.files.len();

        let accepted = zatlas_core::baseline::load(std::path::Path::new(&parts.info.root))
            .ok()
            .flatten()
            .map(|b| b.entries.into_iter().map(|e| e.id).collect())
            .unwrap_or_default();
        let deps = Adjacency::build(n, &parts.analysis.graph.edges);
        let structural = Adjacency::build_for(n, &parts.analysis.graph.edges, Relation::Structural);
        Self {
            info: parts.info,
            config: parts.config,
            analysis: parts.analysis,
            deps,
            structural,
            layout: parts.layout,
            findings: parts.findings,
            history: parts.history,
            coupling: parts.coupling,
            scanned_at: now_ms(),
            lsp: parts.lsp,
            accepted,
        }
    }
}

impl Loaded {
    pub fn is_accepted(&self, finding: &zatlas_core::findings::Finding) -> bool {
        !self.accepted.is_empty()
            && self.accepted.contains(&zatlas_core::baseline::fingerprint(
                finding,
                &self.analysis.graph,
            ))
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Default)]
pub struct AppState {
    pub loaded: RwLock<Option<Loaded>>,

    pub watch: Mutex<Option<zatlas_core::watch::Watch>>,

    pub cancels: Mutex<HashMap<String, CancelFlag>>,

    pub lsp_mode: RwLock<Option<LspMode>>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_scan(&self, id: &str) -> CancelFlag {
        let flag: CancelFlag = Arc::new(AtomicBool::new(false));
        self.cancels
            .lock()
            .expect("cancel map poisoned")
            .insert(id.to_owned(), flag.clone());
        flag
    }

    pub fn cancel_scan(&self, id: &str) -> bool {
        match self.cancels.lock().expect("cancel map poisoned").get(id) {
            Some(flag) => {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    pub fn finish_scan(&self, id: &str) {
        self.cancels.lock().expect("cancel map poisoned").remove(id);
    }

    pub fn cancel_all(&self) {
        for flag in self.cancels.lock().expect("cancel map poisoned").values() {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn cancelling_a_registered_scan_sets_its_flag() {
        let state = AppState::new();
        let flag = state.register_scan("s1");
        assert!(!flag.load(Ordering::Relaxed));
        assert!(state.cancel_scan("s1"));
        assert!(flag.load(Ordering::Relaxed));
    }

    #[test]
    fn cancelling_an_unknown_scan_reports_that_rather_than_panicking() {
        let state = AppState::new();
        assert!(!state.cancel_scan("nope"));
    }

    #[test]
    fn finishing_a_scan_removes_it_so_the_map_does_not_grow_without_bound() {
        let state = AppState::new();
        state.register_scan("s1");
        state.finish_scan("s1");
        assert!(!state.cancel_scan("s1"));
    }

    #[test]
    fn starting_a_new_scan_can_cancel_every_earlier_one() {
        let state = AppState::new();
        let a = state.register_scan("a");
        let b = state.register_scan("b");
        state.cancel_all();
        assert!(a.load(Ordering::Relaxed));
        assert!(b.load(Ordering::Relaxed));
    }
}
