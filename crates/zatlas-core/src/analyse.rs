use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::cache::{content_hash, Cache};
use crate::config::ZatlasConfig;
use crate::error::Result;
use crate::model::{
    CaseMismatch, Edge, EdgeKind, EdgeSite, FileId, FileNode, Graph, Language, ModuleId,
    ModuleNode, UnresolvedImport, UnresolvedReason,
};
use crate::parse::{self, ImportKind, ParsedFile};
use crate::resolve::{self, go, javascript as js, python as py, rust as rs, FileIndex};
use crate::walk::{self, SkippedFile, WalkOptions};

pub struct Analysis {
    pub graph: Graph,

    pub parsed: HashMap<FileId, ParsedFile>,
    pub skipped: Vec<SkippedFile>,
}

pub type Progress<'a> = &'a (dyn Fn(Phase, usize, usize) + Sync);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Walking,
    Parsing,
    Indexing,
    Resolving,
}

pub struct AnalyseOptions {
    pub walk: WalkOptions,

    pub use_cache: bool,

    pub include_type_only: bool,
}

impl Default for AnalyseOptions {
    fn default() -> Self {
        Self {
            walk: WalkOptions::default(),
            use_cache: true,
            include_type_only: true,
        }
    }
}

pub fn analyse(
    root: &Path,
    config: &ZatlasConfig,
    opts: &AnalyseOptions,
    cancel: &Arc<AtomicBool>,
    progress: Progress,
) -> Result<Analysis> {
    progress(Phase::Walking, 0, 0);
    let walked = walk::walk(root, config, &opts.walk, cancel)?;
    let total = walked.files.len();

    let cache = if opts.use_cache {
        Cache::open(&crate::config::cache_db_path(root)).ok()
    } else {
        None
    };

    progress(Phase::Parsing, 0, total);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let parsed_pairs: Vec<(String, ParsedFile, bool)> = walked
        .files
        .par_iter()
        .map(|f| {
            if cancel.load(Ordering::Relaxed) {
                return (String::new(), ParsedFile::default(), true);
            }
            let bytes = std::fs::read(&f.abs_path).unwrap_or_default();
            let hash = content_hash(&bytes);

            if let Some(cache) = &cache {
                if let Some(hit) = cache.get(&hash) {
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if n % 256 == 0 {
                        progress(Phase::Parsing, n, total);
                    }
                    return (hash, hit, true);
                }
            }

            let source = String::from_utf8_lossy(&bytes);
            let p = parse::parse(f.language, &source);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 256 == 0 {
                progress(Phase::Parsing, n, total);
            }
            (hash, p, false)
        })
        .collect();
    progress(Phase::Parsing, total, total);

    if let Some(cache) = &cache {
        let fresh: Vec<(String, u8, ParsedFile)> = parsed_pairs
            .iter()
            .zip(walked.files.iter())
            .filter(|((hash, _, hit), _)| !*hit && !hash.is_empty())
            .map(|((hash, parsed, _), f)| (hash.clone(), f.language as u8, parsed.clone()))
            .collect();
        let _ = cache.put_many(&fresh);
    }

    let hashes: Vec<String> = parsed_pairs.iter().map(|(h, _, _)| h.clone()).collect();
    let parsed_vec: Vec<ParsedFile> = parsed_pairs.into_iter().map(|(_, p, _)| p).collect();

    if cancel.load(Ordering::Relaxed) {
        return Ok(Analysis {
            graph: Graph {
                root: root.to_path_buf(),
                ..Default::default()
            },
            parsed: HashMap::new(),
            skipped: walked.skipped,
        });
    }

    progress(Phase::Indexing, 0, total);
    let paths: Vec<String> = walked.files.iter().map(|f| f.rel_path.clone()).collect();
    let languages: Vec<Language> = walked.files.iter().map(|f| f.language).collect();
    let index = FileIndex::new(paths.clone(), languages);

    let mut all_for_projects = paths.clone();
    all_for_projects.extend(walked.manifests.iter().cloned());

    let js_project = js::build_project(root, &index, &all_for_projects);

    let mut mod_decls: HashMap<String, Vec<String>> = HashMap::new();
    for (i, p) in parsed_vec.iter().enumerate() {
        let decls: Vec<String> = p
            .imports
            .iter()
            .filter(|im| im.kind == ImportKind::ModDecl)
            .map(|im| im.specifier.clone())
            .collect();
        if !decls.is_empty() {
            mod_decls.insert(paths[i].clone(), decls);
        }
    }
    let rust_project = rs::build_project(root, &index, &all_for_projects, &mod_decls);
    let python_project = py::build_project(root, &all_for_projects);
    let go_project = go::build_project(root, &all_for_projects);

    let parsed: HashMap<FileId, ParsedFile> = parsed_vec
        .iter()
        .enumerate()
        .map(|(i, p)| (FileId(i as u32), p.clone()))
        .collect();

    progress(Phase::Resolving, 0, total);
    let mut edges: Vec<Edge> = Vec::new();
    let mut unresolved: Vec<UnresolvedImport> = Vec::new();
    let mut case_mismatches: Vec<CaseMismatch> = Vec::new();
    let mut edge_sites: Vec<EdgeSite> = Vec::new();

    for (i, file) in walked.files.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let from = FileId(i as u32);
        let from_path = &file.rel_path;

        for (import_index, import) in parsed_vec[i].imports.iter().enumerate() {
            if import.type_only && !opts.include_type_only {
                continue;
            }

            if !file.language.has_resolver() {
                unresolved.push(UnresolvedImport {
                    from,
                    specifier: import.specifier.clone(),
                    line: import.line,
                    reason: UnresolvedReason::NoResolverForLanguage(file.language),
                });
                continue;
            }

            let direct = match file.language {
                Language::Rust => rs::resolve_specifier_scoped(
                    from_path,
                    &import.specifier,
                    import.kind == ImportKind::ModDecl,
                    import.scope.as_deref(),
                    &index,
                    &rust_project,
                ),
                Language::Python => {
                    let names = match &import.names {
                        crate::parse::ImportedNames::Named(v) => v.as_slice(),
                        _ => &[],
                    };
                    py::resolve_specifier(
                        from_path,
                        &import.specifier,
                        names,
                        &index,
                        &python_project,
                    )
                }
                Language::Go => {
                    go::resolve_specifier(from_path, &import.specifier, &index, &go_project)
                }
                _ => js::resolve_specifier_cased(from_path, &import.specifier, &index, &js_project)
                    .map(|(id, exact)| {
                        if !exact {
                            case_mismatches.push(CaseMismatch {
                                from,
                                specifier: import.specifier.clone(),
                                line: import.line,
                                actual: index.path(id).unwrap_or_default().to_string(),
                            });
                        }
                        id
                    }),
            };

            let target = match direct {
                Ok(id) => id,
                Err(reason) => {
                    let reason = reclassify_if_on_disk(
                        root,
                        from_path,
                        &import.specifier,
                        reason,
                        &js_project,
                    );
                    unresolved.push(UnresolvedImport {
                        from,
                        specifier: import.specifier.clone(),
                        line: import.line,
                        reason,
                    });
                    continue;
                }
            };

            let (final_target, kind) = see_through_barrel(
                target,
                import,
                &parsed,
                &index,
                &js_project,
                &rust_project,
                file.language,
            );

            if final_target == from {
                continue;
            }

            let kind = if import.kind == ImportKind::ModDecl {
                EdgeKind::Contains
            } else {
                kind
            };

            edges.push(Edge {
                from,
                to: final_target,
                kind,
                weight: 1.0,
            });

            edge_sites.push(EdgeSite {
                from,
                to: final_target,
                import: import_index as u32,
            });
        }
    }
    progress(Phase::Resolving, total, total);

    edges.sort_by_key(|e| (e.from.0, e.to.0, e.kind as u8));
    edges.dedup_by_key(|e| (e.from.0, e.to.0, e.kind as u8));

    let (modules, module_of) = build_modules(&paths);

    let files: Vec<FileNode> = walked
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| FileNode {
            id: FileId(i as u32),
            path: f.rel_path.clone(),
            module: module_of[i],
            language: f.language,
            loc: parsed_vec[i].loc,
            bytes: f.bytes,
            content_hash: hashes.get(i).cloned().unwrap_or_default(),
        })
        .collect();

    let js_specifiers: Vec<String> = walked
        .files
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            matches!(
                f.language,
                Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx
            )
        })
        .flat_map(|(i, _)| parsed_vec[i].imports.iter().map(|im| im.specifier.clone()))
        .collect();
    let alias_audit = js::audit_aliases(root, &js_project, &index, &js_specifiers);

    unresolved.sort_by_key(|u| (u.from.0, u.line, u.specifier.clone()));
    case_mismatches.sort_by_key(|c| (c.from.0, c.line, c.specifier.clone()));
    case_mismatches.dedup();

    Ok(Analysis {
        graph: Graph {
            root: root.to_path_buf(),
            files,
            modules,
            edges,
            unresolved,
            case_mismatches,
            lsp_repairs: Vec::new(),
            lsp_disagreements: Vec::new(),
            alias_audit,
            edge_sites,
        },
        parsed,
        skipped: walked.skipped,
    })
}

fn see_through_barrel(
    target: FileId,
    import: &parse::RawImport,
    parsed: &HashMap<FileId, ParsedFile>,
    index: &FileIndex,
    js_project: &js::JsProject,
    rust_project: &rs::RustProject,
    language: Language,
) -> (FileId, EdgeKind) {
    let Some(symbols) = resolve::traceable_symbols(&import.names) else {
        return (target, EdgeKind::Import);
    };
    let Some(symbol) = symbols.first() else {
        return (target, EdgeKind::Import);
    };

    let Some(target_file) = parsed.get(&target) else {
        return (target, EdgeKind::Import);
    };

    if target_file.exports.iter().all(|e| e.from.is_none()) {
        return (target, EdgeKind::Import);
    }

    let resolve_spec = |from: FileId, spec: &str| -> Option<FileId> {
        let from_path = index.path(from)?;
        match language {
            Language::Rust => {
                rs::resolve_specifier(from_path, spec, false, index, rust_project).ok()
            }

            Language::Python | Language::Go => None,
            _ => js::resolve_specifier(from_path, spec, index, js_project).ok(),
        }
    };

    match resolve::follow_reexport(target, symbol, parsed, &resolve_spec) {
        Some(defining) => (defining, EdgeKind::Import),

        None => (target, EdgeKind::ViaBarrel),
    }
}

fn reclassify_if_on_disk(
    root: &Path,
    from_path: &str,
    specifier: &str,
    reason: UnresolvedReason,
    js_project: &js::JsProject,
) -> UnresolvedReason {
    let looked_for_a_file = matches!(
        reason,
        UnresolvedReason::NoSuchFile(_) | UnresolvedReason::UnmatchedAlias(_)
    );
    if !looked_for_a_file {
        return reason;
    }

    let mut bases = js::candidate_paths(from_path, specifier, js_project);
    if bases.is_empty() {
        let dir = match from_path.rfind('/') {
            Some(i) => &from_path[..i],
            None => "",
        };
        bases.push(js::join_rel(dir, specifier));
    }

    const CANDIDATE_SUFFIXES: &[&str] = &[
        "",
        ".ts",
        ".tsx",
        ".d.ts",
        ".js",
        ".jsx",
        ".mts",
        ".cts",
        ".mjs",
        ".cjs",
        ".rs",
        "/index.ts",
        "/index.tsx",
        "/index.js",
        "/index.jsx",
        "/mod.rs",
    ];

    for base in &bases {
        if base.is_empty() {
            continue;
        }
        for suffix in CANDIDATE_SUFFIXES {
            let candidate = crate::paths::rel_to_path(root, &format!("{base}{suffix}"));
            if candidate.is_file() {
                return UnresolvedReason::ExcludedFromScan(specifier.to_string());
            }
        }
    }
    reason
}

fn build_modules(paths: &[String]) -> (Vec<ModuleNode>, Vec<ModuleId>) {
    let mut by_dir: HashMap<&str, Vec<FileId>> = HashMap::new();
    for (i, p) in paths.iter().enumerate() {
        let dir = match p.rfind('/') {
            Some(idx) => &p[..idx],
            None => "",
        };
        by_dir.entry(dir).or_default().push(FileId(i as u32));
    }

    let mut dirs: Vec<&str> = by_dir.keys().copied().collect();
    dirs.sort_unstable();

    let mut modules = Vec::with_capacity(dirs.len());
    let mut module_of = vec![ModuleId(0); paths.len()];

    for (mi, dir) in dirs.iter().enumerate() {
        let id = ModuleId(mi as u32);
        let files = by_dir.remove(dir).unwrap_or_default();
        for f in &files {
            module_of[f.0 as usize] = id;
        }
        modules.push(ModuleNode {
            id,
            path: (*dir).to_string(),
            files,
        });
    }

    (modules, module_of)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_every_edge_has_a_site(a: &Analysis) {
        for edge in &a.graph.edges {
            if edge.kind == EdgeKind::CoChange {
                continue;
            }
            let found = a
                .graph
                .edge_sites
                .iter()
                .any(|s| s.from == edge.from && s.to == edge.to);
            assert!(
                found,
                "edge {:?} -> {:?} has no import site",
                edge.from, edge.to
            );
        }
    }

    fn touch(root: &Path, rel: &str, body: &str) {
        let p = crate::paths::rel_to_path(root, rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn run(root: &Path) -> Analysis {
        let opts = AnalyseOptions {
            walk: WalkOptions {
                threads: 2,
                ..Default::default()
            },

            use_cache: false,
            ..Default::default()
        };
        analyse(
            root,
            &ZatlasConfig::default(),
            &opts,
            &Arc::new(AtomicBool::new(false)),
            &|_, _, _| {},
        )
        .unwrap()
    }

    fn edges(a: &Analysis) -> Vec<String> {
        let mut out: Vec<String> = a
            .graph
            .edges
            .iter()
            .map(|e| {
                format!(
                    "{} -> {}",
                    a.graph.file(e.from).unwrap().path,
                    a.graph.file(e.to).unwrap().path
                )
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn a_two_file_typescript_repo_produces_one_edge() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(
            p,
            "src/a.ts",
            "import { b } from './b';\nexport const a = b;",
        );
        touch(p, "src/b.ts", "export const b = 1;");

        let a = run(p);
        assert_eq!(edges(&a), ["src/a.ts -> src/b.ts"]);
        assert_eq!(a.graph.unresolved_failure_count(), 0);
    }

    #[test]
    fn a_barrel_does_not_swallow_the_graph() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let mut barrel = String::new();
        for i in 0..40 {
            touch(
                p,
                &format!("src/m{i}.ts"),
                &format!("export const s{i} = {i};"),
            );
            barrel.push_str(&format!("export {{ s{i} }} from './m{i}';\n"));
        }
        touch(p, "src/index.ts", &barrel);
        touch(
            p,
            "src/app.ts",
            "import { s7 } from './index';\nexport const x = s7;",
        );

        let a = run(p);
        let from_app: Vec<&Edge> = a
            .graph
            .edges
            .iter()
            .filter(|e| a.graph.file(e.from).unwrap().path == "src/app.ts")
            .collect();
        assert_eq!(
            from_app.len(),
            1,
            "expected exactly one edge, got {from_app:?}"
        );
        assert_eq!(a.graph.file(from_app[0].to).unwrap().path, "src/m7.ts");
        assert_eq!(from_app[0].kind, EdgeKind::Import);
    }

    #[test]
    fn a_namespace_import_of_a_barrel_depends_on_the_barrel_itself() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "src/m.ts", "export const s = 1;");
        touch(p, "src/index.ts", "export { s } from './m';");
        touch(
            p,
            "src/app.ts",
            "import * as ns from './index';\nexport const x = ns;",
        );

        let a = run(p);
        assert!(edges(&a).contains(&"src/app.ts -> src/index.ts".to_string()));
    }

    #[test]
    fn an_untraceable_barrel_import_is_tagged_rather_than_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "src/index.ts", "export { missing } from './nowhere';");
        touch(
            p,
            "src/app.ts",
            "import { missing } from './index';\nexport const x = missing;",
        );

        let a = run(p);
        let e = a
            .graph
            .edges
            .iter()
            .find(|e| a.graph.file(e.from).unwrap().path == "src/app.ts")
            .expect("the edge must exist, weaker evidence or not");
        assert_eq!(e.kind, EdgeKind::ViaBarrel);
    }

    #[test]
    fn a_mod_declaration_is_containment_not_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "Cargo.toml", "[package]\nname = \"demo\"\n");
        touch(p, "src/lib.rs", "pub mod auth;\npub struct Shared;\n");
        touch(
            p,
            "src/auth.rs",
            "use crate::Shared;\npub fn f(_: Shared) {}\n",
        );

        let a = run(p);
        let lib = a
            .graph
            .files
            .iter()
            .find(|f| f.path == "src/lib.rs")
            .unwrap();
        let decl = a
            .graph
            .edges
            .iter()
            .find(|e| e.from == lib.id)
            .expect("lib.rs declares the module");
        assert_eq!(decl.kind, EdgeKind::Contains);

        let adj = crate::graph::Adjacency::build(a.graph.files.len(), &a.graph.edges);
        assert!(
            crate::graph::tangles(&adj).is_empty(),
            "containment must not create a cycle"
        );
    }

    #[test]
    fn a_rust_crate_resolves_through_its_module_tree() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "Cargo.toml", "[package]\nname = \"demo\"\n");
        touch(p, "src/lib.rs", "pub mod auth;\npub mod api;\n");
        touch(p, "src/auth.rs", "pub struct Session;\n");
        touch(
            p,
            "src/api.rs",
            "use crate::auth::Session;\npub fn f(_: Session) {}\n",
        );

        let a = run(p);
        let got = edges(&a);
        assert!(
            got.contains(&"src/api.rs -> src/auth.rs".to_string()),
            "got {got:?}"
        );
        assert!(
            got.contains(&"src/lib.rs -> src/auth.rs".to_string()),
            "got {got:?}"
        );
    }

    #[test]
    fn external_packages_never_count_as_resolution_failures() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(
            p,
            "src/a.ts",
            "import React from 'react';\nexport const x = React;",
        );

        let a = run(p);
        assert!(a.graph.edges.is_empty());
        assert_eq!(a.graph.unresolved.len(), 1);
        assert_eq!(a.graph.unresolved_failure_count(), 0);
    }

    #[test]
    fn a_genuinely_broken_import_is_counted_and_explained() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(
            p,
            "src/a.ts",
            "import { x } from './does-not-exist';\nexport const y = x;",
        );

        let a = run(p);
        assert_eq!(a.graph.unresolved_failure_count(), 1);
        assert_eq!(
            a.graph.unresolved[0].reason,
            UnresolvedReason::NoSuchFile("./does-not-exist".into())
        );
        assert_eq!(a.graph.unresolved[0].line, 1);
    }

    #[test]
    fn importing_two_symbols_from_one_file_is_one_dependency() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "src/b.ts", "export const b = 1;\nexport const c = 2;");
        touch(
            p,
            "src/a.ts",
            "import { b, c } from './b';\nexport const x = b + c;",
        );

        let a = run(p);
        assert_eq!(a.graph.edges.len(), 1, "fan-in would be inflated otherwise");
    }

    #[test]
    fn a_file_importing_itself_is_not_an_edge() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(
            p,
            "src/a.ts",
            "import { x } from './a';\nexport const x = 1;",
        );
        let a = run(p);
        assert!(a.graph.edges.is_empty());
    }

    #[test]
    fn a_gitignored_but_present_dependency_is_excluded_not_broken() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, ".gitignore", "src/generated/\n");
        touch(p, "src/generated/Thing.ts", "export type Thing = string;");
        touch(
            p,
            "src/a.ts",
            "import type { Thing } from './generated/Thing';\nexport const x: Thing = 'y';",
        );

        let a = run(p);
        assert_eq!(a.graph.unresolved_failure_count(), 0);
        assert_eq!(
            a.graph.unresolved[0].reason,
            UnresolvedReason::ExcludedFromScan("./generated/Thing".into())
        );
    }

    #[test]
    fn a_genuinely_absent_file_is_still_reported_as_broken() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(
            p,
            "src/a.ts",
            "import { x } from './truly-gone';\nexport const y = x;",
        );
        let a = run(p);
        assert_eq!(a.graph.unresolved_failure_count(), 1);
    }

    #[test]
    fn a_second_scan_reuses_the_cache_and_produces_the_same_graph() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(
            p,
            "src/a.ts",
            "import { b } from './b';\nexport const a = b;",
        );
        touch(p, "src/b.ts", "export const b = 1;");

        let opts = AnalyseOptions {
            walk: WalkOptions {
                threads: 2,
                ..Default::default()
            },
            use_cache: true,
            ..Default::default()
        };
        let go = || {
            analyse(
                p,
                &ZatlasConfig::default(),
                &opts,
                &Arc::new(AtomicBool::new(false)),
                &|_, _, _| {},
            )
            .unwrap()
        };

        let cold = go();
        assert!(
            crate::config::cache_db_path(p).exists(),
            "the cache should have been written"
        );
        let warm = go();
        assert_eq!(edges(&cold), edges(&warm));
        assert_eq!(
            warm.graph.files[0].content_hash,
            cold.graph.files[0].content_hash
        );
        assert!(!warm.graph.files[0].content_hash.is_empty());
    }

    #[test]
    fn editing_a_file_invalidates_only_that_entry() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "src/a.ts", "export const a = 1;");
        touch(p, "src/b.ts", "export const b = 1;");

        let opts = AnalyseOptions {
            walk: WalkOptions {
                threads: 2,
                ..Default::default()
            },
            use_cache: true,
            ..Default::default()
        };
        let go = || {
            analyse(
                p,
                &ZatlasConfig::default(),
                &opts,
                &Arc::new(AtomicBool::new(false)),
                &|_, _, _| {},
            )
            .unwrap()
        };

        let first = go();
        touch(
            p,
            "src/a.ts",
            "import { b } from './b';\nexport const a = b;",
        );
        let second = go();

        assert_ne!(
            first.graph.files[0].content_hash, second.graph.files[0].content_hash,
            "the edited file must hash differently"
        );
        assert_eq!(
            first.graph.files[1].content_hash, second.graph.files[1].content_hash,
            "the untouched file keeps its hash, and so its cache entry"
        );
        assert_eq!(edges(&second), ["src/a.ts -> src/b.ts"]);
    }

    #[test]
    fn modules_group_files_by_directory() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "src/ui/a.ts", "export const a = 1;");
        touch(p, "src/ui/b.ts", "export const b = 1;");
        touch(p, "src/data/c.ts", "export const c = 1;");

        let a = run(p);
        let paths: Vec<&str> = a.graph.modules.iter().map(|m| m.path.as_str()).collect();
        assert_eq!(paths, ["src/data", "src/ui"]);
        assert_eq!(a.graph.modules[1].files.len(), 2);
    }

    #[test]
    fn a_python_package_resolves_its_own_imports() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "app/__init__.py", "");
        touch(p, "app/models.py", "class Thing:\n    pass\n");
        touch(
            p,
            "app/views.py",
            "import os\nfrom .models import Thing\n\ndef view():\n    return Thing()\n",
        );

        let a = run(p);
        assert!(
            edges(&a).contains(&"app/views.py -> app/models.py".to_string()),
            "got {:?}",
            edges(&a)
        );

        assert_eq!(a.graph.unresolved_failure_count(), 0);
    }

    #[test]
    fn a_go_module_resolves_its_own_packages() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "go.mod", "module example.com/demo\n\ngo 1.22\n");
        touch(
            p,
            "pkg/store/store.go",
            "package store\n\ntype Store struct{}\n",
        );
        touch(
            p,
            "main.go",
            "package main\n\nimport (\n\t\"fmt\"\n\t\"example.com/demo/pkg/store\"\n)\n\nfunc main() {}\n",
        );

        let a = run(p);
        assert!(
            edges(&a).contains(&"main.go -> pkg/store/store.go".to_string()),
            "got {:?}",
            edges(&a)
        );
        assert_eq!(a.graph.unresolved_failure_count(), 0);
    }

    #[test]
    fn a_broken_python_relative_import_is_still_reported() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(p, "app/__init__.py", "");
        touch(p, "app/views.py", "from .gone import Thing\n");

        let a = run(p);
        assert_eq!(a.graph.unresolved_failure_count(), 1);
    }

    #[test]
    fn two_scans_of_the_same_repo_produce_identical_graphs() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        for i in 0..20 {
            touch(
                p,
                &format!("src/f{i}.ts"),
                &format!(
                    "import {{ v }} from './f{}';\nexport const v = {i};",
                    (i + 1) % 20
                ),
            );
        }
        let first = run(p);
        let second = run(p);
        assert_eq!(edges(&first), edges(&second));
        let ids1: Vec<&str> = first.graph.files.iter().map(|f| f.path.as_str()).collect();
        let ids2: Vec<&str> = second.graph.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(ids1, ids2);
    }

    #[test]
    fn a_tsconfig_alias_is_read_from_disk_and_applied() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(
            p,
            "tsconfig.json",
            r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["src/*"] } } }"#,
        );
        touch(p, "src/lib/util.ts", "export const u = 1;");
        touch(
            p,
            "src/app.ts",
            "import { u } from '@/lib/util';\nexport const x = u;",
        );

        let a = run(p);
        assert_eq!(edges(&a), ["src/app.ts -> src/lib/util.ts"]);
    }
    #[test]
    fn an_edge_can_be_traced_to_the_line_that_made_it() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "src/dep.ts", "export const d = 1;\n");
        touch(
            dir.path(),
            "src/main.ts",
            "// a comment\nimport { d } from './dep';\nexport const m = d;\n",
        );
        let a = run(dir.path());

        assert_eq!(a.graph.edges.len(), 1);
        assert_eq!(a.graph.edge_sites.len(), 1);
        let site = a.graph.edge_sites[0];

        let parsed = a.parsed.get(&site.from).expect("the importer was parsed");
        let import = &parsed.imports[site.import as usize];
        assert_eq!(import.line, 2);
        assert_eq!(import.specifier, "./dep");
        assert_every_edge_has_a_site(&a);
    }

    #[test]
    fn two_imports_of_one_file_are_one_edge_but_two_sites() {
        let dir = tempfile::tempdir().unwrap();
        touch(
            dir.path(),
            "src/dep.ts",
            "export const a = 1;\nexport const b = 2;\n",
        );
        touch(
            dir.path(),
            "src/main.ts",
            "import { a } from './dep';\nimport { b } from './dep';\nexport const m = a + b;\n",
        );
        let a = run(dir.path());
        assert_eq!(a.graph.edges.len(), 1);
        assert_eq!(a.graph.edge_sites.len(), 2);
        let lines: Vec<u32> = a
            .graph
            .edge_sites
            .iter()
            .map(|s| a.parsed[&s.from].imports[s.import as usize].line)
            .collect();
        assert_eq!(lines, [1, 2]);
    }

    #[test]
    fn every_edge_in_a_realistic_tree_has_a_site() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "src/a.ts", "export const a = 1;\n");
        touch(
            dir.path(),
            "src/b.ts",
            "import { a } from './a';\nexport const b = a;\n",
        );
        touch(
            dir.path(),
            "src/c.ts",
            "import { a } from './a';\nimport { b } from './b';\nexport const c = a + b;\n",
        );
        assert_every_edge_has_a_site(&run(dir.path()));
    }
}
