use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::analyse::{analyse, AnalyseOptions, Analysis};
use crate::config::ZatlasConfig;
use crate::error::Result;
use crate::model::{
    Edge, EdgeKind, FileId, FileNode, Graph, ModuleId, ModuleNode, UnresolvedReason,
};

pub struct Member {
    pub name: String,
    pub root: PathBuf,
    pub analysis: Analysis,

    pub packages: Vec<String>,
}

pub struct Workspace {
    pub members: Vec<Member>,

    pub graph: Graph,
}

pub fn published_packages(root: &Path, manifests: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for rel in manifests {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let Ok(text) = std::fs::read_to_string(crate::paths::rel_to_path(root, rel)) else {
            continue;
        };
        match name {
            "package.json" => {
                if let Some(found) = json_string_field(&text, "name") {
                    out.push(found);
                }
            }
            "Cargo.toml" => {
                let mut in_package = false;
                for line in text.lines() {
                    let line = line.trim();
                    if line.starts_with('[') {
                        in_package = line == "[package]";
                        continue;
                    }
                    if !in_package {
                        continue;
                    }
                    if let Some(value) = line.strip_prefix("name") {
                        let value = value.trim_start_matches(['=', ' ']).trim();
                        let value = value.trim_matches('"').trim_matches('\'');
                        if !value.is_empty() {
                            out.push(value.to_string());

                            let underscored = value.replace('-', "_");
                            if underscored != value {
                                out.push(underscored);
                            }
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out.sort();
    out.dedup();
    out
}

fn json_string_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let at = text.find(&needle)? + needle.len();
    let rest = text.get(at..)?;
    let colon = rest.find(':')? + 1;
    let after = rest.get(colon..)?;
    let open = after.find('"')?;
    let tail = after.get(open + 1..)?;
    let close = tail.find('"')?;
    Some(tail[..close].to_string())
}

pub fn analyse_workspace(
    roots: &[PathBuf],
    opts: &AnalyseOptions,
    cancel: &Arc<AtomicBool>,
) -> Result<Workspace> {
    let mut members = Vec::new();

    for root in roots {
        let config = ZatlasConfig::load(root)?;
        let analysis = analyse(root, &config, opts, cancel, &|_, _, _| {})?;
        let manifests = crate::walk::walk(root, &config, &opts.walk, cancel)?.manifests;
        let packages = published_packages(root, &manifests);
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.to_string_lossy().into_owned());
        members.push(Member {
            name,
            root: root.clone(),
            analysis,
            packages,
        });
    }

    let graph = merge(&members);
    Ok(Workspace { members, graph })
}

pub fn merge(members: &[Member]) -> Graph {
    let mut files: Vec<FileNode> = Vec::new();
    let mut modules: Vec<ModuleNode> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();

    let mut file_base: Vec<u32> = Vec::with_capacity(members.len());
    let mut module_base: Vec<u32> = Vec::with_capacity(members.len());

    for member in members {
        file_base.push(files.len() as u32);
        module_base.push(modules.len() as u32);
        let fb = *file_base.last().unwrap();
        let mb = *module_base.last().unwrap();

        for f in &member.analysis.graph.files {
            files.push(FileNode {
                id: FileId(fb + f.id.0),

                path: format!("{}/{}", member.name, f.path),
                module: ModuleId(mb + f.module.0),
                language: f.language,
                loc: f.loc,
                bytes: f.bytes,
                content_hash: f.content_hash.clone(),
            });
        }
        for m in &member.analysis.graph.modules {
            modules.push(ModuleNode {
                id: ModuleId(mb + m.id.0),
                path: format!("{}/{}", member.name, m.path),
                files: m.files.iter().map(|f| FileId(fb + f.0)).collect(),
            });
        }
        for e in &member.analysis.graph.edges {
            edges.push(Edge {
                from: FileId(fb + e.from.0),
                to: FileId(fb + e.to.0),
                kind: e.kind,
                weight: e.weight,
            });
        }
    }

    let mut owner: HashMap<&str, Option<usize>> = HashMap::new();
    for (i, member) in members.iter().enumerate() {
        for pkg in &member.packages {
            owner
                .entry(pkg.as_str())
                .and_modify(|slot| *slot = None)
                .or_insert(Some(i));
        }
    }

    for (i, member) in members.iter().enumerate() {
        for u in &member.analysis.graph.unresolved {
            let UnresolvedReason::External(pkg) = &u.reason else {
                continue;
            };
            let Some(Some(target)) = owner.get(pkg.as_str()) else {
                continue;
            };
            if *target == i {
                continue;
            }

            let Some(entry) = representative_file(&members[*target]) else {
                continue;
            };
            edges.push(Edge {
                from: FileId(file_base[i] + u.from.0),
                to: FileId(file_base[*target] + entry.0),
                kind: EdgeKind::Import,
                weight: 1.0,
            });
        }
    }

    edges.sort_by_key(|e| (e.from.0, e.to.0, e.kind as u8));
    edges.dedup_by_key(|e| (e.from.0, e.to.0, e.kind as u8));

    Graph {
        root: PathBuf::new(),
        files,
        modules,
        edges,
        unresolved: Vec::new(),

        case_mismatches: Vec::new(),
        lsp_repairs: Vec::new(),
        lsp_disagreements: Vec::new(),
        alias_audit: Vec::new(),
        edge_sites: Vec::new(),
    }
}

fn representative_file(member: &Member) -> Option<FileId> {
    let g = &member.analysis.graph;
    let adj = crate::graph::Adjacency::build(g.files.len(), &g.edges);
    g.files
        .iter()
        .max_by_key(|f| (adj.fan_in(f.id), std::cmp::Reverse(f.path.clone())))
        .map(|f| f.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::walk::WalkOptions;

    fn touch(root: &Path, rel: &str, body: &str) {
        let p = crate::paths::rel_to_path(root, rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn opts() -> AnalyseOptions {
        AnalyseOptions {
            walk: WalkOptions {
                threads: 2,
                ..Default::default()
            },
            use_cache: false,
            ..Default::default()
        }
    }

    fn run(roots: &[PathBuf]) -> Workspace {
        analyse_workspace(roots, &opts(), &Arc::new(AtomicBool::new(false))).unwrap()
    }

    fn edge_paths(g: &Graph) -> Vec<String> {
        let mut out: Vec<String> = g
            .edges
            .iter()
            .filter_map(|e| {
                Some(format!(
                    "{} -> {}",
                    g.file(e.from)?.path,
                    g.file(e.to)?.path
                ))
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn package_names_come_from_both_manifest_kinds() {
        let dir = tempfile::tempdir().unwrap();
        touch(
            dir.path(),
            "package.json",
            r#"{ "name": "@app/ui", "version": "1.0.0" }"#,
        );
        touch(dir.path(), "Cargo.toml", "[package]\nname = \"my-crate\"\n");
        let names = published_packages(
            dir.path(),
            &["package.json".to_string(), "Cargo.toml".to_string()],
        );
        assert!(names.contains(&"@app/ui".to_string()), "{names:?}");
        assert!(names.contains(&"my-crate".to_string()), "{names:?}");

        assert!(names.contains(&"my_crate".to_string()), "{names:?}");
    }

    #[test]
    fn a_workspace_dependency_becomes_a_real_edge() {
        let a = tempfile::tempdir().unwrap();
        touch(a.path(), "package.json", r#"{ "name": "@acme/ui" }"#);
        touch(a.path(), "src/button.ts", "export const Button = 1;");
        touch(
            a.path(),
            "src/index.ts",
            "export { Button } from './button';",
        );

        let b = tempfile::tempdir().unwrap();
        touch(b.path(), "package.json", r#"{ "name": "@acme/web" }"#);
        touch(
            b.path(),
            "src/app.ts",
            "import { Button } from '@acme/ui';\nexport const App = Button;",
        );

        let ws = run(&[a.path().to_path_buf(), b.path().to_path_buf()]);
        let crossing: Vec<String> = edge_paths(&ws.graph)
            .into_iter()
            .filter(|e| {
                let (from, to) = e.split_once(" -> ").unwrap();
                from.split('/').next() != to.split('/').next()
            })
            .collect();
        assert_eq!(
            crossing.len(),
            1,
            "expected one cross-repo edge, got {crossing:?}"
        );
        assert!(crossing[0].contains("app.ts -> "), "{crossing:?}");
    }

    #[test]
    fn paths_are_prefixed_so_two_repositories_do_not_collide() {
        let a = tempfile::tempdir().unwrap();
        touch(a.path(), "src/index.ts", "export const a = 1;");
        let b = tempfile::tempdir().unwrap();
        touch(b.path(), "src/index.ts", "export const b = 1;");

        let ws = run(&[a.path().to_path_buf(), b.path().to_path_buf()]);
        let paths: Vec<&str> = ws.graph.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths.len(), 2);
        assert_ne!(
            paths[0], paths[1],
            "identical paths would merge unrelated files"
        );
        assert!(paths.iter().all(|p| p.ends_with("src/index.ts")));
    }

    #[test]
    fn within_repository_edges_survive_the_merge() {
        let a = tempfile::tempdir().unwrap();
        touch(a.path(), "src/b.ts", "export const b = 1;");
        touch(
            a.path(),
            "src/a.ts",
            "import { b } from './b';\nexport const a = b;",
        );

        let ws = run(&[a.path().to_path_buf()]);
        assert_eq!(ws.graph.edges.len(), 1);
        assert!(edge_paths(&ws.graph)[0].contains("a.ts -> "));
    }

    #[test]
    fn an_external_package_nobody_publishes_stays_external() {
        let a = tempfile::tempdir().unwrap();
        touch(a.path(), "package.json", r#"{ "name": "@acme/web" }"#);
        touch(
            a.path(),
            "src/app.ts",
            "import React from 'react';\nexport const A = React;",
        );

        let ws = run(&[a.path().to_path_buf()]);
        assert!(ws.graph.edges.is_empty(), "react is not a workspace member");
    }

    #[test]
    fn a_package_name_published_twice_links_to_neither() {
        let a = tempfile::tempdir().unwrap();
        touch(a.path(), "package.json", r#"{ "name": "@acme/ui" }"#);
        touch(a.path(), "src/x.ts", "export const x = 1;");

        let b = tempfile::tempdir().unwrap();
        touch(b.path(), "package.json", r#"{ "name": "@acme/ui" }"#);
        touch(b.path(), "src/y.ts", "export const y = 1;");

        let c = tempfile::tempdir().unwrap();
        touch(c.path(), "package.json", r#"{ "name": "@acme/web" }"#);
        touch(
            c.path(),
            "src/app.ts",
            "import { x } from '@acme/ui';\nexport const A = x;",
        );

        let ws = run(&[
            a.path().to_path_buf(),
            b.path().to_path_buf(),
            c.path().to_path_buf(),
        ]);
        let crossing: Vec<String> = edge_paths(&ws.graph)
            .into_iter()
            .filter(|e| {
                let (from, to) = e.split_once(" -> ").unwrap();
                from.split('/').next() != to.split('/').next()
            })
            .collect();
        assert!(
            crossing.is_empty(),
            "ambiguous package linked anyway: {crossing:?}"
        );
    }

    #[test]
    fn a_repository_importing_its_own_package_name_gets_no_self_edge() {
        let a = tempfile::tempdir().unwrap();
        touch(a.path(), "package.json", r#"{ "name": "@acme/ui" }"#);
        touch(
            a.path(),
            "src/x.ts",
            "import { y } from '@acme/ui';\nexport const x = y;",
        );

        let ws = run(&[a.path().to_path_buf()]);
        assert!(ws.graph.edges.is_empty(), "{:?}", edge_paths(&ws.graph));
    }

    #[test]
    fn an_empty_workspace_is_not_an_error() {
        let ws = run(&[]);
        assert!(ws.members.is_empty());
        assert!(ws.graph.files.is_empty());
    }

    #[test]
    fn a_rust_workspace_links_by_the_underscored_crate_name() {
        let core = tempfile::tempdir().unwrap();
        touch(
            core.path(),
            "Cargo.toml",
            "[package]\nname = \"acme-core\"\n",
        );
        touch(core.path(), "src/lib.rs", "pub struct Thing;\n");

        let app = tempfile::tempdir().unwrap();
        touch(app.path(), "Cargo.toml", "[package]\nname = \"acme-app\"\n");
        touch(
            app.path(),
            "src/main.rs",
            "use acme_core::Thing;\nfn main() { let _ = Thing; }\n",
        );

        let ws = run(&[core.path().to_path_buf(), app.path().to_path_buf()]);
        let crossing: Vec<String> = edge_paths(&ws.graph)
            .into_iter()
            .filter(|e| {
                let (from, to) = e.split_once(" -> ").unwrap();
                from.split('/').next() != to.split('/').next()
            })
            .collect();
        assert_eq!(crossing.len(), 1, "got {crossing:?}");
        assert!(crossing[0].contains("main.rs -> "), "{crossing:?}");
    }
}
