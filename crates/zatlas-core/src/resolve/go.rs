use std::collections::HashMap;
use std::path::Path;

use super::FileIndex;
use crate::model::{FileId, UnresolvedReason};

#[derive(Debug, Clone, Default)]
pub struct GoProject {
    pub modules: Vec<(String, String)>,
}

fn parent_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

fn join(base: &str, rel: &str) -> String {
    super::javascript::join_rel(base, rel)
}

pub fn build_project(root: &Path, all_files: &[String]) -> GoProject {
    let mut project = GoProject::default();

    for rel in all_files {
        if rel.rsplit('/').next() != Some("go.mod") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(crate::paths::rel_to_path(root, rel)) else {
            continue;
        };
        let Some(module) = text
            .lines()
            .map(str::trim)
            .find_map(|l| l.strip_prefix("module ").map(str::trim))
        else {
            continue;
        };
        let module = module.trim_matches('"').to_string();
        if module.is_empty() {
            continue;
        }
        project.modules.push((module, parent_dir(rel).to_string()));
    }

    project
        .modules
        .sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    project
}

fn package_file(dir: &str, index: &FileIndex) -> Option<FileId> {
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    let mut best: Option<(&str, FileId)> = None;
    for (i, path) in index.paths.iter().enumerate() {
        if !path.ends_with(".go") {
            continue;
        }
        if parent_dir(path) != dir.trim_end_matches('/') {
            continue;
        }
        let _ = &prefix;

        let is_test = path.ends_with("_test.go");
        let better = match best {
            None => true,
            Some((current, _)) => {
                let current_test = current.ends_with("_test.go");
                (current_test && !is_test) || (current_test == is_test && path.as_str() < current)
            }
        };
        if better {
            best = Some((path.as_str(), FileId(i as u32)));
        }
    }
    best.map(|(_, id)| id)
}

pub fn resolve_specifier(
    from_path: &str,
    specifier: &str,
    index: &FileIndex,
    project: &GoProject,
) -> Result<FileId, UnresolvedReason> {
    if specifier.is_empty() {
        return Err(UnresolvedReason::NoSuchFile(specifier.to_string()));
    }

    if specifier.starts_with("./") || specifier.starts_with("../") {
        let dir = join(parent_dir(from_path), specifier);
        return package_file(&dir, index)
            .ok_or_else(|| UnresolvedReason::NoSuchFile(specifier.to_string()));
    }

    for (module, dir) in &project.modules {
        let inside = if specifier == module {
            Some(dir.clone())
        } else {
            specifier
                .strip_prefix(&format!("{module}/"))
                .map(|rest| join(dir, rest))
        };
        let Some(candidate) = inside else { continue };
        if let Some(id) = package_file(&candidate, index) {
            return Ok(id);
        }

        return Err(UnresolvedReason::NoSuchFile(specifier.to_string()));
    }

    Err(UnresolvedReason::External(specifier.to_string()))
}

pub fn package_dirs(index: &FileIndex) -> HashMap<String, usize> {
    let mut out: HashMap<String, usize> = HashMap::new();
    for path in &index.paths {
        if path.ends_with(".go") {
            *out.entry(parent_dir(path).to_string()).or_insert(0) += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Language;

    fn index(paths: &[&str]) -> FileIndex {
        let owned: Vec<String> = paths.iter().map(|s| s.to_string()).collect();
        let langs = vec![Language::Go; owned.len()];
        FileIndex::new(owned, langs)
    }

    fn project(modules: &[(&str, &str)]) -> GoProject {
        let mut p = GoProject {
            modules: modules
                .iter()
                .map(|(m, d)| (m.to_string(), d.to_string()))
                .collect(),
        };
        p.modules.sort_by_key(|m| std::cmp::Reverse(m.0.len()));
        p
    }

    #[test]
    fn an_internal_package_resolves_through_the_module_path() {
        let idx = index(&["main.go", "pkg/thing/thing.go"]);
        let p = project(&[("github.com/user/repo", "")]);
        let got = resolve_specifier("main.go", "github.com/user/repo/pkg/thing", &idx, &p);
        assert_eq!(got, Ok(idx.id("pkg/thing/thing.go").unwrap()));
    }

    #[test]
    fn the_module_root_itself_resolves() {
        let idx = index(&["main.go"]);
        let p = project(&[("github.com/user/repo", "")]);
        let got = resolve_specifier("main.go", "github.com/user/repo", &idx, &p);
        assert_eq!(got, Ok(idx.id("main.go").unwrap()));
    }

    #[test]
    fn the_standard_library_is_external() {
        let idx = index(&["main.go"]);
        let p = project(&[("github.com/user/repo", "")]);
        assert_eq!(
            resolve_specifier("main.go", "fmt", &idx, &p),
            Err(UnresolvedReason::External("fmt".into()))
        );
    }

    #[test]
    fn a_third_party_package_is_external() {
        let idx = index(&["main.go"]);
        let p = project(&[("github.com/user/repo", "")]);
        assert_eq!(
            resolve_specifier("main.go", "github.com/lib/pq", &idx, &p),
            Err(UnresolvedReason::External("github.com/lib/pq".into()))
        );
    }

    #[test]
    fn a_missing_package_inside_our_own_module_is_a_real_failure() {
        let idx = index(&["main.go"]);
        let p = project(&[("github.com/user/repo", "")]);
        assert_eq!(
            resolve_specifier("main.go", "github.com/user/repo/gone", &idx, &p),
            Err(UnresolvedReason::NoSuchFile(
                "github.com/user/repo/gone".into()
            ))
        );
    }

    #[test]
    fn a_nested_module_wins_over_its_parent() {
        let idx = index(&["main.go", "tools/sub/a.go"]);
        let p = project(&[
            ("github.com/user/repo", ""),
            ("github.com/user/repo/tools", "tools"),
        ]);
        let got = resolve_specifier("main.go", "github.com/user/repo/tools/sub", &idx, &p);
        assert_eq!(got, Ok(idx.id("tools/sub/a.go").unwrap()));
    }

    #[test]
    fn a_package_target_is_stable_and_prefers_non_test_files() {
        let idx = index(&["pkg/a_test.go", "pkg/b.go", "pkg/c.go"]);
        let p = project(&[("m", "")]);
        let first = resolve_specifier("main.go", "m/pkg", &idx, &p);
        assert_eq!(first, Ok(idx.id("pkg/b.go").unwrap()));
        assert_eq!(first, resolve_specifier("main.go", "m/pkg", &idx, &p));
    }

    #[test]
    fn a_relative_import_resolves_against_the_importing_file() {
        let idx = index(&["cmd/main.go", "cmd/helper/h.go"]);
        let p = GoProject::default();
        let got = resolve_specifier("cmd/main.go", "./helper", &idx, &p);
        assert_eq!(got, Ok(idx.id("cmd/helper/h.go").unwrap()));
    }

    #[test]
    fn a_go_mod_file_yields_its_module_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("go.mod"),
            "module github.com/user/repo\n\ngo 1.22\n",
        )
        .unwrap();
        let p = build_project(dir.path(), &["go.mod".to_string()]);
        assert_eq!(
            p.modules,
            [("github.com/user/repo".to_string(), String::new())]
        );
    }
}
