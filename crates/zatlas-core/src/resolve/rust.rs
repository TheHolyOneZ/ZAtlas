use std::collections::HashMap;
use std::path::Path;

use super::FileIndex;
use crate::model::{FileId, UnresolvedReason};

#[derive(Debug, Clone)]
pub struct CrateInfo {
    pub name: String,

    pub dir: String,

    pub roots: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RustProject {
    pub crates: Vec<CrateInfo>,

    pub module_to_file: HashMap<String, String>,

    pub file_to_module: HashMap<String, String>,

    pub external_crates: Vec<String>,
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

pub fn normalise_crate_name(name: &str) -> String {
    name.replace('-', "_")
}

fn read_manifest(text: &str) -> (Option<String>, Vec<String>, Vec<String>, Vec<String>) {
    let mut package_name = None;
    let mut members = Vec::new();
    let mut bins = Vec::new();
    let mut deps = Vec::new();
    let mut section = String::new();
    let mut pending_bin_path: Option<String> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if section == "bin" {
                if let Some(p) = pending_bin_path.take() {
                    bins.push(p);
                }
            }
            section = line.trim_matches(['[', ']']).to_string();
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches(['"', '\'']).to_string();

        match section.as_str() {
            "package" if key == "name" => package_name = Some(value),
            "workspace" if key == "members" => {
                members = split_inline_array(line);
            }
            "bin" if key == "path" => pending_bin_path = Some(value),
            "dependencies"
            | "dev-dependencies"
            | "build-dependencies"
            | "workspace.dependencies" => {
                deps.push(normalise_crate_name(key.trim_matches(['"', '\''])));
            }
            _ => {}
        }
    }
    if let Some(p) = pending_bin_path {
        bins.push(p);
    }
    (package_name, members, bins, deps)
}

fn split_inline_array(line: &str) -> Vec<String> {
    let Some(open) = line.find('[') else {
        return Vec::new();
    };
    let close = line.rfind(']').unwrap_or(line.len());
    if close <= open {
        return Vec::new();
    }
    line[open + 1..close]
        .split(',')
        .map(|s| s.trim().trim_matches(['"', '\'']).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn build_project(
    root: &Path,
    index: &FileIndex,
    all_files: &[String],
    parsed_mods: &HashMap<String, Vec<String>>,
) -> RustProject {
    let mut project = RustProject::default();

    for rel in all_files {
        if rel.rsplit('/').next() != Some("Cargo.toml") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(crate::paths::rel_to_path(root, rel)) else {
            continue;
        };
        let dir = parent_dir(rel).to_string();
        let (name, _members, bins, deps) = read_manifest(&text);
        project.external_crates.extend(deps);

        let Some(name) = name else {
            continue;
        };

        let mut roots = Vec::new();
        for candidate in ["src/lib.rs", "src/main.rs"] {
            let p = join(&dir, candidate);
            if index.contains(&p) {
                roots.push(p);
            }
        }
        for bin in bins {
            let p = join(&dir, &bin);
            if index.contains(&p) {
                roots.push(p);
            }
        }

        let mut extra_roots = Vec::new();
        for candidate in index.paths.iter() {
            if !candidate.ends_with(".rs") {
                continue;
            }
            let candidate_dir = parent_dir(candidate);
            let last = candidate_dir.rsplit('/').next().unwrap_or(candidate_dir);
            if !matches!(last, "tests" | "benches" | "examples") {
                continue;
            }
            let expected = if dir.is_empty() {
                last.to_string()
            } else {
                format!("{dir}/{last}")
            };
            if candidate_dir == expected {
                extra_roots.push(candidate.clone());
            }
        }

        if roots.is_empty() && extra_roots.is_empty() {
            continue;
        }

        let crate_name = normalise_crate_name(&name);

        for aux in extra_roots {
            let stem = aux
                .rsplit('/')
                .next()
                .and_then(|f| f.strip_suffix(".rs"))
                .unwrap_or("aux");
            project.crates.push(CrateInfo {
                name: normalise_crate_name(&format!("{crate_name}_{stem}")),
                dir: dir.clone(),
                roots: vec![aux],
            });
        }

        if roots.is_empty() {
            continue;
        }

        project.crates.push(CrateInfo {
            name: crate_name,
            dir,
            roots,
        });
    }

    project.external_crates.sort();
    project.external_crates.dedup();
    project.crates.sort_by(|a, b| a.name.cmp(&b.name));

    for krate in &project.crates {
        for root_file in &krate.roots {
            walk_module_tree(
                krate,
                root_file,
                index,
                parsed_mods,
                &mut project.module_to_file,
                &mut project.file_to_module,
            );
        }
    }

    project
}

fn walk_module_tree(
    krate: &CrateInfo,
    root_file: &str,
    index: &FileIndex,
    parsed_mods: &HashMap<String, Vec<String>>,
    module_to_file: &mut HashMap<String, String>,
    file_to_module: &mut HashMap<String, String>,
) {
    let mut queue = vec![(krate.name.clone(), root_file.to_string())];
    let mut seen: Vec<String> = Vec::new();

    module_to_file.insert(krate.name.clone(), root_file.to_string());
    file_to_module
        .entry(root_file.to_string())
        .or_insert_with(|| krate.name.clone());

    while let Some((mod_path, file)) = queue.pop() {
        if seen.contains(&file) {
            continue;
        }
        seen.push(file.clone());

        let Some(decls) = parsed_mods.get(&file) else {
            continue;
        };

        for decl in decls {
            let (name, explicit) = match decl.split_once('\u{1}') {
                Some((n, p)) => (n, Some(p)),
                None => (decl.as_str(), None),
            };

            let child_file = match explicit {
                Some(p) => {
                    let candidate = join(&path_attr_base(&file, None), p);
                    index.contains(&candidate).then_some(candidate)
                }
                None => find_module_file(&file, name, index),
            };

            let Some(child_file) = child_file else {
                continue;
            };

            let child_path = format!("{mod_path}::{name}");
            module_to_file.insert(child_path.clone(), child_file.clone());
            file_to_module
                .entry(child_file.clone())
                .or_insert(child_path.clone());
            queue.push((child_path, child_file));
        }
    }
}

fn is_crate_root(file: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    if matches!(name, "lib.rs" | "main.rs" | "mod.rs") {
        return true;
    }
    let dir = parent_dir(file);
    let last_dir = dir.rsplit('/').next().unwrap_or(dir);
    matches!(last_dir, "tests" | "benches" | "examples")
}

fn module_dir(file: &str) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    let dir = parent_dir(file);
    if is_crate_root(file) {
        return dir.to_string();
    }
    let stem = name.strip_suffix(".rs").unwrap_or(name);
    join(dir, stem)
}

fn module_dir_scoped(file: &str, scope: Option<&str>) -> String {
    let base = module_dir(file);
    match scope {
        Some(s) if !s.is_empty() => join(&base, &s.replace("::", "/")),
        _ => base,
    }
}

fn path_attr_base(file: &str, scope: Option<&str>) -> String {
    let base = parent_dir(file).to_string();
    match scope {
        Some(s) if !s.is_empty() => join(&base, &s.replace("::", "/")),
        _ => base,
    }
}

fn find_module_file(parent_file: &str, name: &str, index: &FileIndex) -> Option<String> {
    find_module_file_scoped(parent_file, name, None, index)
}

fn find_module_file_scoped(
    parent_file: &str,
    name: &str,
    scope: Option<&str>,
    index: &FileIndex,
) -> Option<String> {
    let dir = module_dir_scoped(parent_file, scope);

    let flat = join(&dir, &format!("{name}.rs"));
    if index.contains(&flat) {
        return Some(flat);
    }
    let nested = join(&dir, &format!("{name}/mod.rs"));
    if index.contains(&nested) {
        return Some(nested);
    }
    None
}

pub fn resolve_specifier(
    from_path: &str,
    specifier: &str,
    is_mod_decl: bool,
    index: &FileIndex,
    project: &RustProject,
) -> Result<FileId, UnresolvedReason> {
    resolve_specifier_scoped(from_path, specifier, is_mod_decl, None, index, project)
}

pub fn resolve_specifier_scoped(
    from_path: &str,
    specifier: &str,
    is_mod_decl: bool,
    scope: Option<&str>,
    index: &FileIndex,
    project: &RustProject,
) -> Result<FileId, UnresolvedReason> {
    if is_mod_decl {
        let (name, explicit) = match specifier.split_once('\u{1}') {
            Some((n, p)) => (n, Some(p)),
            None => (specifier, None),
        };
        let candidate = match explicit {
            Some(p) => join(&path_attr_base(from_path, scope), p),
            None => {
                return find_module_file_scoped(from_path, name, scope, index)
                    .and_then(|f| index.id(&f))
                    .ok_or_else(|| UnresolvedReason::NoSuchFile(format!("mod {name}")))
            }
        };
        return index
            .id(&candidate)
            .ok_or(UnresolvedReason::NoSuchFile(candidate));
    }

    let segments: Vec<&str> = specifier.split("::").filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return Err(UnresolvedReason::NoSuchFile(specifier.to_string()));
    }

    let current_module: Option<String> =
        project.file_to_module.get(from_path).map(|m| match scope {
            Some(s) if !s.is_empty() => format!("{m}::{s}"),
            _ => m.clone(),
        });
    let current_module = current_module.as_ref();

    let needs_module_tree = matches!(segments[0], "crate" | "self" | "super");
    if needs_module_tree && current_module.is_none() {
        return Err(UnresolvedReason::FileNotInModuleTree);
    }

    let absolute: String = match segments[0] {
        "crate" => {
            let Some(current) = current_module else {
                return Err(UnresolvedReason::NoSuchFile(specifier.to_string()));
            };
            let krate = current.split("::").next().unwrap_or(current);
            std::iter::once(krate)
                .chain(segments[1..].iter().copied())
                .collect::<Vec<_>>()
                .join("::")
        }
        "self" => {
            let Some(current) = current_module else {
                return Err(UnresolvedReason::NoSuchFile(specifier.to_string()));
            };
            std::iter::once(current.as_str())
                .chain(segments[1..].iter().copied())
                .collect::<Vec<_>>()
                .join("::")
        }
        "super" => {
            let Some(current) = current_module else {
                return Err(UnresolvedReason::NoSuchFile(specifier.to_string()));
            };

            let mut parts: Vec<&str> = current.split("::").collect();
            let mut rest = &segments[..];
            while rest.first() == Some(&"super") {
                if parts.len() <= 1 {
                    return Err(UnresolvedReason::NoSuchFile(specifier.to_string()));
                }
                parts.pop();
                rest = &rest[1..];
            }
            parts
                .into_iter()
                .chain(rest.iter().copied())
                .collect::<Vec<_>>()
                .join("::")
        }
        first => {
            let normalised = normalise_crate_name(first);
            let is_local = project.crates.iter().any(|c| c.name == normalised);
            if !is_local {
                return Err(UnresolvedReason::External(normalised));
            }
            std::iter::once(normalised.as_str())
                .chain(segments[1..].iter().copied())
                .collect::<Vec<_>>()
                .join("::")
        }
    };

    let parts: Vec<&str> = absolute.split("::").collect();
    for take in (1..=parts.len()).rev() {
        let candidate = parts[..take].join("::");
        if let Some(file) = project.module_to_file.get(&candidate) {
            if let Some(id) = index.id(file) {
                return Ok(id);
            }
        }
    }

    Err(UnresolvedReason::NoSuchFile(specifier.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Language;

    fn index(paths: &[&str]) -> FileIndex {
        let paths: Vec<String> = paths.iter().map(|s| s.to_string()).collect();
        let langs = vec![Language::Rust; paths.len()];
        FileIndex::new(paths, langs)
    }

    fn project(crate_name: &str, root: &str, mods: &[(&str, &[&str])]) -> RustProject {
        let mut parsed_mods = HashMap::new();
        for (file, decls) in mods {
            parsed_mods.insert(
                file.to_string(),
                decls.iter().map(|s| s.to_string()).collect(),
            );
        }
        let mut all: Vec<String> = mods.iter().map(|(f, _)| f.to_string()).collect();
        all.push(root.to_string());
        all.sort();
        all.dedup();

        let idx = index(&all.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        let krate = CrateInfo {
            name: crate_name.into(),
            dir: String::new(),
            roots: vec![root.into()],
        };
        let mut p = RustProject {
            crates: vec![krate.clone()],
            ..Default::default()
        };
        walk_module_tree(
            &krate,
            root,
            &idx,
            &parsed_mods,
            &mut p.module_to_file,
            &mut p.file_to_module,
        );
        p
    }

    #[test]
    fn module_dir_follows_rusts_one_rule() {
        assert_eq!(module_dir("src/lib.rs"), "src");
        assert_eq!(module_dir("src/main.rs"), "src");
        assert_eq!(module_dir("src/a/mod.rs"), "src/a");

        assert_eq!(module_dir("src/a.rs"), "src/a");
        assert_eq!(module_dir("src/a/b.rs"), "src/a/b");
    }

    #[test]
    fn the_module_tree_is_built_from_mod_declarations_not_the_file_tree() {
        let p = project(
            "mycrate",
            "src/lib.rs",
            &[("src/lib.rs", &["auth"]), ("src/auth.rs", &["session"])],
        );
        assert_eq!(p.module_to_file.get("mycrate").unwrap(), "src/lib.rs");
        assert_eq!(
            p.module_to_file.get("mycrate::auth").unwrap(),
            "src/auth.rs"
        );
    }

    #[test]
    fn a_file_never_reached_by_a_mod_declaration_is_not_in_the_module_tree() {
        let p = project(
            "mycrate",
            "src/lib.rs",
            &[
                ("src/lib.rs", &["a"]),
                ("src/a.rs", &[]),
                ("src/orphan.rs", &[]),
            ],
        );
        assert!(p.file_to_module.contains_key("src/a.rs"));
        assert!(!p.file_to_module.contains_key("src/orphan.rs"));
    }

    #[test]
    fn both_the_flat_and_mod_rs_layouts_resolve() {
        let mut parsed = HashMap::new();
        parsed.insert(
            "src/lib.rs".to_string(),
            vec!["flat".to_string(), "nested".to_string()],
        );
        let idx = index(&["src/lib.rs", "src/flat.rs", "src/nested/mod.rs"]);
        let krate = CrateInfo {
            name: "k".into(),
            dir: String::new(),
            roots: vec!["src/lib.rs".into()],
        };
        let mut m = HashMap::new();
        let mut f = HashMap::new();
        walk_module_tree(&krate, "src/lib.rs", &idx, &parsed, &mut m, &mut f);
        assert_eq!(m.get("k::flat").unwrap(), "src/flat.rs");
        assert_eq!(m.get("k::nested").unwrap(), "src/nested/mod.rs");
    }

    #[test]
    fn a_crate_path_resolves_to_the_module_that_owns_the_item() {
        let p = project(
            "mycrate",
            "src/lib.rs",
            &[
                ("src/lib.rs", &["auth"]),
                ("src/auth.rs", &["session"]),
                ("src/auth/session.rs", &[]),
            ],
        );
        let idx = index(&["src/auth.rs", "src/auth/session.rs", "src/lib.rs"]);
        let got = resolve_specifier("src/lib.rs", "crate::auth::session::Thing", false, &idx, &p);
        assert_eq!(got, Ok(idx.id("src/auth/session.rs").unwrap()));
    }

    #[test]
    fn super_climbs_one_module_not_one_directory() {
        let p = project(
            "k",
            "src/lib.rs",
            &[
                ("src/lib.rs", &["a"]),
                ("src/a.rs", &["b", "c"]),
                ("src/a/b.rs", &[]),
                ("src/a/c.rs", &[]),
            ],
        );
        let idx = index(&["src/a.rs", "src/a/b.rs", "src/a/c.rs", "src/lib.rs"]);
        let got = resolve_specifier("src/a/b.rs", "super::c::Thing", false, &idx, &p);
        assert_eq!(got, Ok(idx.id("src/a/c.rs").unwrap()));
    }

    #[test]
    fn repeated_super_climbs_repeatedly() {
        let p = project(
            "k",
            "src/lib.rs",
            &[
                ("src/lib.rs", &["a", "top"]),
                ("src/a.rs", &["b"]),
                ("src/a/b.rs", &[]),
                ("src/top.rs", &[]),
            ],
        );
        let idx = index(&["src/a.rs", "src/a/b.rs", "src/lib.rs", "src/top.rs"]);
        let got = resolve_specifier("src/a/b.rs", "super::super::top::T", false, &idx, &p);
        assert_eq!(got, Ok(idx.id("src/top.rs").unwrap()));
    }

    #[test]
    fn self_resolves_within_the_current_module() {
        let p = project(
            "k",
            "src/lib.rs",
            &[
                ("src/lib.rs", &["a"]),
                ("src/a.rs", &["child"]),
                ("src/a/child.rs", &[]),
            ],
        );
        let idx = index(&["src/a.rs", "src/a/child.rs", "src/lib.rs"]);
        let got = resolve_specifier("src/a.rs", "self::child::Thing", false, &idx, &p);
        assert_eq!(got, Ok(idx.id("src/a/child.rs").unwrap()));
    }

    #[test]
    fn a_path_attribute_sends_the_module_somewhere_else_entirely() {
        let mut parsed = HashMap::new();
        parsed.insert(
            "src/lib.rs".to_string(),
            vec!["weird\u{1}actually/here.rs".to_string()],
        );
        let idx = index(&["src/lib.rs", "src/actually/here.rs"]);
        let krate = CrateInfo {
            name: "k".into(),
            dir: String::new(),
            roots: vec!["src/lib.rs".into()],
        };
        let mut m = HashMap::new();
        let mut f = HashMap::new();
        walk_module_tree(&krate, "src/lib.rs", &idx, &parsed, &mut m, &mut f);
        assert_eq!(m.get("k::weird").unwrap(), "src/actually/here.rs");
    }

    #[test]
    fn super_inside_an_inline_test_module_lands_on_the_file_itself() {
        let p = project("k", "src/lib.rs", &[("src/lib.rs", &[])]);
        let idx = index(&["src/lib.rs"]);
        let got = resolve_specifier_scoped("src/lib.rs", "super", false, Some("tests"), &idx, &p);
        assert_eq!(got, Ok(idx.id("src/lib.rs").unwrap()));
    }

    #[test]
    fn super_from_a_nested_inline_module_climbs_only_within_the_file() {
        let p = project("k", "src/lib.rs", &[("src/lib.rs", &[])]);
        let idx = index(&["src/lib.rs"]);
        let got =
            resolve_specifier_scoped("src/lib.rs", "super::super", false, Some("a::b"), &idx, &p);
        assert_eq!(got, Ok(idx.id("src/lib.rs").unwrap()));
    }

    #[test]
    fn an_integration_test_file_is_a_crate_root() {
        assert!(is_crate_root("crates/k/tests/robustness.rs"));
        assert!(is_crate_root("benches/bench.rs"));
        assert!(is_crate_root("examples/demo.rs"));
        assert!(!is_crate_root("src/a.rs"));
        assert_eq!(module_dir("crates/k/tests/robustness.rs"), "crates/k/tests");
    }

    #[test]
    fn a_mod_inside_an_inline_module_looks_in_that_subdirectory() {
        let idx = index(&["tests/robustness.rs", "tests/attacks/jpeg.rs"]);
        let p = RustProject::default();
        let got = resolve_specifier_scoped(
            "tests/robustness.rs",
            "jpeg",
            true,
            Some("attacks"),
            &idx,
            &p,
        );
        assert_eq!(got, Ok(idx.id("tests/attacks/jpeg.rs").unwrap()));
    }

    #[test]
    fn a_mod_inside_a_nested_inline_module_descends_twice() {
        let idx = index(&["src/lib.rs", "src/a/b/c.rs"]);
        let p = RustProject::default();
        let got = resolve_specifier_scoped("src/lib.rs", "c", true, Some("a::b"), &idx, &p);
        assert_eq!(got, Ok(idx.id("src/a/b/c.rs").unwrap()));
    }

    #[test]
    fn a_file_outside_every_module_tree_says_so_rather_than_blaming_its_imports() {
        let p = project("k", "src/lib.rs", &[("src/lib.rs", &[])]);
        let idx = index(&["src/lib.rs", "src/stray.rs"]);
        assert_eq!(
            resolve_specifier("src/stray.rs", "crate::Thing", false, &idx, &p),
            Err(UnresolvedReason::FileNotInModuleTree)
        );
        assert_eq!(
            resolve_specifier("src/stray.rs", "super::Thing", false, &idx, &p),
            Err(UnresolvedReason::FileNotInModuleTree)
        );

        assert_eq!(
            resolve_specifier("src/stray.rs", "serde::Serialize", false, &idx, &p),
            Err(UnresolvedReason::External("serde".into()))
        );
    }

    #[test]
    fn an_external_crate_is_reported_as_external_not_as_a_broken_path() {
        let p = project("k", "src/lib.rs", &[("src/lib.rs", &[])]);
        let idx = index(&["src/lib.rs"]);
        assert_eq!(
            resolve_specifier("src/lib.rs", "serde::Serialize", false, &idx, &p),
            Err(UnresolvedReason::External("serde".into()))
        );
        assert_eq!(
            resolve_specifier("src/lib.rs", "std::collections::HashMap", false, &idx, &p),
            Err(UnresolvedReason::External("std".into()))
        );
    }

    #[test]
    fn an_integration_test_file_is_registered_as_its_own_crate_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::create_dir_all(dir.path().join("tests")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "").unwrap();
        std::fs::write(dir.path().join("tests/qa.rs"), "").unwrap();

        let files = ["Cargo.toml", "src/lib.rs", "tests/qa.rs"];
        let idx = index(&files);
        let owned: Vec<String> = files.iter().map(|s| s.to_string()).collect();
        let p = build_project(dir.path(), &idx, &owned, &HashMap::new());

        assert!(
            p.file_to_module.contains_key("tests/qa.rs"),
            "the test file must be in the module tree: {:?}",
            p.file_to_module
        );

        let got =
            resolve_specifier_scoped("tests/qa.rs", "super", false, Some("image_tests"), &idx, &p);
        assert_eq!(got, Ok(idx.id("tests/qa.rs").unwrap()));
    }

    #[test]
    fn a_hyphenated_crate_name_is_matched_by_its_underscored_identifier() {
        assert_eq!(normalise_crate_name("zatlas-core"), "zatlas_core");
        let mut p = project(
            "zatlas_core",
            "src/lib.rs",
            &[("src/lib.rs", &["model"]), ("src/model.rs", &[])],
        );
        p.crates[0].name = "zatlas_core".into();
        let idx = index(&["src/lib.rs", "src/model.rs"]);
        let got = resolve_specifier("src/lib.rs", "zatlas_core::model::FileId", false, &idx, &p);
        assert_eq!(got, Ok(idx.id("src/model.rs").unwrap()));
    }

    #[test]
    fn a_mod_declaration_resolves_to_the_child_file() {
        let idx = index(&["src/lib.rs", "src/child.rs"]);
        let p = RustProject::default();
        let got = resolve_specifier("src/lib.rs", "child", true, &idx, &p);
        assert_eq!(got, Ok(idx.id("src/child.rs").unwrap()));
    }

    #[test]
    fn a_mod_declaration_with_no_file_reports_no_such_file() {
        let idx = index(&["src/lib.rs"]);
        let p = RustProject::default();
        assert_eq!(
            resolve_specifier("src/lib.rs", "missing", true, &idx, &p),
            Err(UnresolvedReason::NoSuchFile("mod missing".into()))
        );
    }

    #[test]
    fn a_manifest_yields_its_package_name_bins_and_dependencies() {
        let (name, _members, bins, deps) = read_manifest(
            r#"
[package]
name = "my-crate"
version = "0.1.0"

[[bin]]
name = "tool"
path = "src/tool.rs"

[dependencies]
serde = { version = "1" }
tokio = "1"
"#,
        );
        assert_eq!(name.as_deref(), Some("my-crate"));
        assert_eq!(bins, ["src/tool.rs"]);
        assert!(deps.contains(&"serde".to_string()));
        assert!(deps.contains(&"tokio".to_string()));
    }

    #[test]
    fn a_path_attribute_resolves_against_the_files_own_directory() {
        assert_eq!(path_attr_base("src/scan/windows.rs", None), "src/scan");
        assert_eq!(module_dir("src/scan/windows.rs"), "src/scan/windows");

        let idx = index(&["src/scan/windows.rs", "src/scan/windows_wlanapi.rs"]);
        let p = RustProject::default();
        let got = resolve_specifier(
            "src/scan/windows.rs",
            "imp\u{1}windows_wlanapi.rs",
            true,
            &idx,
            &p,
        );
        assert_eq!(got, Ok(idx.id("src/scan/windows_wlanapi.rs").unwrap()));
    }

    #[test]
    fn a_cycle_created_by_path_attributes_does_not_hang_the_walk() {
        let mut parsed = HashMap::new();
        parsed.insert("src/lib.rs".to_string(), vec!["a\u{1}a.rs".to_string()]);
        parsed.insert(
            "src/a.rs".to_string(),
            vec!["back\u{1}../lib.rs".to_string()],
        );
        let idx = index(&["src/lib.rs", "src/a.rs"]);
        let krate = CrateInfo {
            name: "k".into(),
            dir: String::new(),
            roots: vec!["src/lib.rs".into()],
        };
        let mut m = HashMap::new();
        let mut f = HashMap::new();
        walk_module_tree(&krate, "src/lib.rs", &idx, &parsed, &mut m, &mut f);
        assert!(m.contains_key("k::a"));
    }
}
