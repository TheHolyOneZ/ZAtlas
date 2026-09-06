pub mod protocol;
pub mod servers;
pub mod uri;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};
use ts_rs::TS;

use crate::error::Result;
use crate::model::{
    Edge, EdgeKind, FileId, Graph, Language, LspDisagreement, LspRepair, UnresolvedReason,
};
use crate::parse::ParsedFile;
use crate::paths::rel_to_path;
use protocol::Transport;
use servers::Discovered;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum LspMode {
    #[default]
    Off,

    Repair,

    Verify,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct LspSettings {
    pub mode: LspMode,

    pub timeout_ms: u64,

    pub startup_ms: u64,

    pub budget_ms: u64,

    pub servers: Vec<String>,
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            mode: LspMode::Off,
            timeout_ms: 5_000,
            startup_ms: 30_000,
            budget_ms: 120_000,
            servers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct LspReport {
    pub servers: Vec<String>,

    pub failed: Vec<String>,
    pub queries: u32,

    pub repaired: u32,

    pub disagreements: u32,

    pub unlocatable: u32,

    pub truncated: bool,
}

impl LspReport {
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty() && self.failed.is_empty() && self.queries == 0
    }
}

pub struct LspSession {
    transport: Transport,
    label: String,
    open: HashSet<String>,
    timeout: Duration,

    warming: bool,
    startup: Duration,
}

impl LspSession {
    pub fn start(root: &Path, discovered: &Discovered, settings: &LspSettings) -> Result<Self> {
        let mut transport = Transport::spawn(discovered.command())?;
        let root_uri = uri::path_to_uri(root);

        let params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "rootPath": root.to_string_lossy(),
            "clientInfo": { "name": "ZAtlas" },
            "workspaceFolders": [{
                "uri": root_uri,
                "name": root.file_name().map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "workspace".to_owned()),
            }],
            "capabilities": {
                "textDocument": {
                    "synchronization": { "dynamicRegistration": false },


                    "definition": { "linkSupport": true },
                },
                "workspace": { "workspaceFolders": true },
                "window": { "workDoneProgress": true },
            },
        });

        let startup = Duration::from_millis(settings.startup_ms.max(settings.timeout_ms));
        transport
            .request("initialize", params, startup)
            .map_err(|e| {
                let tail = transport.stderr_tail();
                crate::error::CoreError::other(if tail.is_empty() {
                    format!("{}: {e}", discovered.spec.label)
                } else {
                    format!("{}: {e}\n{tail}", discovered.spec.label)
                })
            })?;
        transport.notify("initialized", json!({}))?;

        Ok(Self {
            transport,
            label: discovered.spec.label.to_owned(),
            open: HashSet::new(),
            timeout: Duration::from_millis(settings.timeout_ms),
            warming: true,
            startup,
        })
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    fn open_document(&mut self, path: &Path, language: Language) -> Result<()> {
        let uri = uri::path_to_uri(path);
        if !self.open.insert(uri.clone()) {
            return Ok(());
        }
        let text =
            std::fs::read_to_string(path).map_err(|e| crate::error::CoreError::io(path, e))?;
        self.transport.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": servers::language_id(language),
                    "version": 1,
                    "text": text,
                }
            }),
        )
    }

    pub fn definition(
        &mut self,
        path: &Path,
        language: Language,
        line: u32,
        character: u32,
    ) -> Result<Vec<std::path::PathBuf>> {
        self.open_document(path, language)?;
        let params = json!({
            "textDocument": { "uri": uri::path_to_uri(path) },
            "position": { "line": line, "character": character },
        });

        let deadline = Instant::now() + self.startup;
        loop {
            let result =
                self.transport
                    .request("textDocument/definition", params.clone(), self.timeout)?;
            let locations = locations_from(&result);
            if !locations.is_empty() {
                self.warming = false;
                return Ok(locations);
            }
            if !self.warming || Instant::now() >= deadline {
                return Ok(Vec::new());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    pub fn shutdown(mut self) {
        let _ = self
            .transport
            .request("shutdown", Value::Null, Duration::from_millis(2_000));
        let _ = self.transport.notify("exit", Value::Null);
    }
}

fn locations_from(result: &Value) -> Vec<std::path::PathBuf> {
    fn one(value: &Value) -> Option<std::path::PathBuf> {
        let uri = value
            .get("uri")
            .or_else(|| value.get("targetUri"))
            .and_then(Value::as_str)?;
        uri::uri_to_path(uri)
    }

    match result {
        Value::Array(items) => items.iter().filter_map(one).collect(),
        Value::Object(_) => one(result).into_iter().collect(),
        _ => Vec::new(),
    }
}

pub fn utf16_offset(line: &str, byte_index: usize) -> u32 {
    line[..byte_index.min(line.len())]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum()
}

struct Site {
    from: FileId,
    specifier: String,
    line: u32,
    character: u32,
}

fn locate(source: &str, line: u32, specifier: &str) -> Option<u32> {
    let text = source.lines().nth(line.checked_sub(1)? as usize)?;

    let start = text.find(specifier)?;
    let middle = start + specifier.len() / 2;

    let middle = (start..=start + specifier.len())
        .rev()
        .find(|i| text.is_char_boundary(*i) && *i <= middle)
        .unwrap_or(start);
    Some(utf16_offset(text, middle))
}

pub fn apply(
    root: &Path,
    graph: &mut Graph,
    parsed: &HashMap<FileId, ParsedFile>,
    settings: &LspSettings,
) -> Result<LspReport> {
    if settings.mode == LspMode::Off {
        return Ok(LspReport::default());
    }
    let discovered = servers::discover(root, &languages_in(graph), &settings.servers);
    apply_with(root, graph, parsed, settings, &discovered)
}

fn languages_in(graph: &Graph) -> Vec<Language> {
    let mut seen: Vec<Language> = graph.files.iter().map(|f| f.language).collect();
    seen.sort_by_key(|l| format!("{l:?}"));
    seen.dedup();
    seen
}

pub fn apply_with(
    root: &Path,
    graph: &mut Graph,
    parsed: &HashMap<FileId, ParsedFile>,
    settings: &LspSettings,
    discovered: &[Discovered],
) -> Result<LspReport> {
    let mut report = LspReport::default();
    if settings.mode == LspMode::Off || discovered.is_empty() {
        return Ok(report);
    }
    let languages = languages_in(graph);

    let by_path: HashMap<&str, FileId> = graph
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.id))
        .collect();
    let existing: HashSet<(u32, u32)> = graph
        .edges
        .iter()
        .filter(|e| e.kind != EdgeKind::CoChange)
        .map(|e| (e.from.0, e.to.0))
        .collect();

    let deadline = Instant::now() + Duration::from_millis(settings.budget_ms);
    let mut repaired: Vec<LspRepair> = Vec::new();
    let mut disagreements: Vec<LspDisagreement> = Vec::new();
    let mut new_edges: Vec<Edge> = Vec::new();
    let mut resolved_by_lsp: HashSet<(u32, u32, String)> = HashSet::new();

    for found in discovered {
        if Instant::now() >= deadline {
            report.truncated = true;
            break;
        }
        let handled: Vec<Language> = languages
            .iter()
            .copied()
            .filter(|l| found.spec.handles(*l))
            .collect();
        if handled.is_empty() {
            continue;
        }

        let sites = plan(graph, parsed, &handled, settings.mode, &mut report);
        if sites.is_empty() {
            continue;
        }

        let mut session = match LspSession::start(root, found, settings) {
            Ok(s) => s,
            Err(e) => {
                report.failed.push(e.to_string());
                continue;
            }
        };
        report.servers.push(session.label().to_owned());

        for site in sites {
            if Instant::now() >= deadline {
                report.truncated = true;
                break;
            }
            let Some(node) = graph.file(site.from) else {
                continue;
            };
            let path = rel_to_path(root, &node.path);
            let language = node.language;
            report.queries += 1;

            let targets = match session.definition(&path, language, site.line - 1, site.character) {
                Ok(t) => t,

                Err(_) => continue,
            };

            let Some(target) = targets
                .iter()
                .filter_map(|t| relative_to(root, t))
                .find_map(|rel| by_path.get(rel.as_str()).copied())
            else {
                continue;
            };
            if target == site.from {
                continue;
            }

            let key = (site.from.0, target.0, site.specifier.clone());
            if existing.contains(&(site.from.0, target.0)) {
                continue;
            }
            if !resolved_by_lsp.insert(key) {
                continue;
            }

            let heuristic_failed = graph.unresolved.iter().any(|u| {
                u.from == site.from && u.line == site.line && u.specifier == site.specifier
            });

            if heuristic_failed {
                report.repaired += 1;
                repaired.push(LspRepair {
                    from: site.from,
                    specifier: site.specifier.clone(),
                    line: site.line,
                    to: target,
                    server: session.label().to_owned(),
                });
            } else {
                report.disagreements += 1;
                disagreements.push(LspDisagreement {
                    from: site.from,
                    specifier: site.specifier.clone(),
                    line: site.line,
                    lsp_target: target,
                    server: session.label().to_owned(),
                });
            }
            new_edges.push(Edge {
                from: site.from,
                to: target,
                kind: EdgeKind::Import,
                weight: 1.0,
            });
        }

        session.shutdown();
    }

    let fixed: HashSet<(u32, u32, String)> = repaired
        .iter()
        .map(|r| (r.from.0, r.line, r.specifier.clone()))
        .collect();
    graph
        .unresolved
        .retain(|u| !fixed.contains(&(u.from.0, u.line, u.specifier.clone())));

    graph.edges.extend(new_edges);
    graph
        .edges
        .sort_by_key(|e| (e.from.0, e.to.0, e.kind as u8));
    graph
        .edges
        .dedup_by_key(|e| (e.from.0, e.to.0, e.kind as u8));

    repaired.sort_by_key(|r| (r.from.0, r.line, r.specifier.clone()));
    disagreements.sort_by_key(|d| (d.from.0, d.line, d.specifier.clone()));
    graph.lsp_repairs = repaired;
    graph.lsp_disagreements = disagreements;

    Ok(report)
}

fn plan(
    graph: &Graph,
    parsed_files: &HashMap<FileId, ParsedFile>,
    handled: &[Language],
    mode: LspMode,
    report: &mut LspReport,
) -> Vec<Site> {
    let mut sites: Vec<Site> = Vec::new();
    let mut sources: HashMap<FileId, String> = HashMap::new();

    let failures: HashSet<(u32, u32, String)> = graph
        .unresolved
        .iter()
        .filter(|u| is_failure(&u.reason))
        .map(|u| (u.from.0, u.line, u.specifier.clone()))
        .collect();

    for file in &graph.files {
        if !handled.contains(&file.language) {
            continue;
        }
        let Some(parsed) = parsed_files.get(&file.id) else {
            continue;
        };
        for import in &parsed.imports {
            let key = (file.id.0, import.line, import.specifier.clone());
            let wanted = match mode {
                LspMode::Off => false,
                LspMode::Repair => failures.contains(&key),
                LspMode::Verify => true,
            };
            if !wanted {
                continue;
            }
            let source = sources.entry(file.id).or_insert_with(|| {
                let path = rel_to_path(&graph.root, &file.path);
                std::fs::read_to_string(path).unwrap_or_default()
            });
            match locate(source, import.line, &import.specifier) {
                Some(character) => sites.push(Site {
                    from: file.id,
                    specifier: import.specifier.clone(),
                    line: import.line,
                    character,
                }),
                None => report.unlocatable += 1,
            }
        }
    }
    sites
}

fn is_failure(reason: &UnresolvedReason) -> bool {
    matches!(
        reason,
        UnresolvedReason::NoSuchFile(_)
            | UnresolvedReason::UnmatchedAlias(_)
            | UnresolvedReason::ReExportChainTooDeep(_)
            | UnresolvedReason::FileNotInModuleTree
    )
}

fn relative_to(root: &Path, target: &Path) -> Option<String> {
    let target = target.canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    let rel = target.strip_prefix(&root).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_is_the_default_because_this_starts_a_process() {
        assert_eq!(LspSettings::default().mode, LspMode::Off);
    }

    #[test]
    fn utf16_offsets_count_code_units_not_bytes() {
        assert_eq!(utf16_offset("abc", 3), 3);
        assert_eq!(utf16_offset("é!", 3), 2);
        assert_eq!(utf16_offset("🗺x", 4), 2);
    }

    #[test]
    fn locate_points_inside_the_specifier_not_at_its_quote() {
        let source = "import { a } from './lib/thing';\n";
        let at = locate(source, 1, "./lib/thing").unwrap();

        assert!(at > 19 && at < 30, "got {at}");
    }

    #[test]
    fn locate_declines_a_specifier_that_is_not_on_the_line() {
        let source = "use crate::{a, b};\n";
        assert_eq!(locate(source, 1, "crate::a"), None);
    }

    #[test]
    fn locate_accounts_for_multibyte_text_earlier_on_the_line() {
        let source = "import x from './a'; // — dash\n";
        let at = locate(source, 1, "./a").unwrap();
        assert_eq!(at, utf16_offset("import x from './a'; // — dash", 16));
    }

    #[test]
    fn locate_survives_a_specifier_whose_midpoint_is_inside_a_character() {
        let source = "import x from './é';\n";
        assert!(locate(source, 1, "./é").is_some());
    }

    #[test]
    fn locate_declines_a_line_past_the_end_of_the_file() {
        assert_eq!(locate("one line\n", 9, "x"), None);
        assert_eq!(locate("one line\n", 0, "x"), None);
    }

    #[test]
    fn all_three_definition_response_shapes_are_understood() {
        let single = json!({ "uri": "file:///r/a.ts", "range": {} });
        let array = json!([{ "uri": "file:///r/a.ts" }, { "uri": "file:///r/b.ts" }]);
        let links = json!([{ "targetUri": "file:///r/c.ts" }]);
        assert_eq!(locations_from(&single).len(), 1);
        assert_eq!(locations_from(&array).len(), 2);
        assert_eq!(
            locations_from(&links)[0],
            std::path::PathBuf::from("/r/c.ts")
        );
        assert!(locations_from(&Value::Null).is_empty());
    }

    #[test]
    fn a_non_file_location_is_dropped_rather_than_becoming_a_bogus_edge() {
        let result = json!([{ "uri": "jdt://contents/rt.jar" }]);
        assert!(locations_from(&result).is_empty());
    }

    #[test]
    fn only_real_failures_are_worth_asking_a_server_about() {
        assert!(is_failure(&UnresolvedReason::NoSuchFile("x".into())));
        assert!(is_failure(&UnresolvedReason::FileNotInModuleTree));

        assert!(!is_failure(&UnresolvedReason::External("serde".into())));
        assert!(!is_failure(&UnresolvedReason::Asset("a.css".into())));
        assert!(!is_failure(&UnresolvedReason::DynamicSpecifier));
        assert!(!is_failure(&UnresolvedReason::ExcludedFromScan("g".into())));
    }

    #[test]
    fn a_target_outside_the_repository_is_not_a_node() {
        let dir = tempfile::tempdir().unwrap();
        let inside = dir.path().join("src");
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::write(inside.join("a.ts"), "").unwrap();
        assert_eq!(
            relative_to(dir.path(), &inside.join("a.ts")).as_deref(),
            Some("src/a.ts")
        );
        assert_eq!(relative_to(dir.path(), Path::new("/usr/lib/x.ts")), None);
    }

    #[test]
    fn the_pass_is_a_no_op_when_the_mode_is_off() {
        let dir = tempfile::tempdir().unwrap();
        let mut graph = Graph::default();
        let report = apply(
            dir.path(),
            &mut graph,
            &HashMap::new(),
            &LspSettings::default(),
        )
        .unwrap();
        assert!(report.is_empty());
        assert_eq!(report.queries, 0);
    }

    #[test]
    fn a_repository_with_no_installed_server_reports_nothing_rather_than_failing() {
        let dir = tempfile::tempdir().unwrap();
        let mut graph = Graph::default();
        let settings = LspSettings {
            mode: LspMode::Repair,
            servers: vec!["not-a-real-server".to_owned()],
            ..Default::default()
        };
        let report = apply(dir.path(), &mut graph, &HashMap::new(), &settings).unwrap();
        assert!(report.servers.is_empty());
        assert!(report.failed.is_empty());
    }
}
