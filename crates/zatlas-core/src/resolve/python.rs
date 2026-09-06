use std::collections::HashSet;
use std::path::Path;

use super::FileIndex;
use crate::model::{FileId, UnresolvedReason};

#[derive(Debug, Clone, Default)]
pub struct PythonProject {
    pub roots: Vec<String>,

    pub packages: HashSet<String>,
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

pub fn build_project(_root: &Path, all_files: &[String]) -> PythonProject {
    let mut project = PythonProject::default();
    let mut roots: HashSet<String> = HashSet::new();
    roots.insert(String::new());

    for rel in all_files {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if name == "__init__.py" {
            let dir = parent_dir(rel).to_string();
            project.packages.insert(dir.clone());

            roots.insert(parent_dir(&dir).to_string());
        }

        for marker in ["src", "lib"] {
            if rel.starts_with(&format!("{marker}/")) {
                roots.insert(marker.to_string());
            }
        }
    }

    project.roots = roots.into_iter().collect();

    project
        .roots
        .sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    project
}

fn try_module(candidate: &str, index: &FileIndex) -> Option<FileId> {
    if let Some(id) = index.id(&format!("{candidate}.py")) {
        return Some(id);
    }
    if let Some(id) = index.id(&format!("{candidate}/__init__.py")) {
        return Some(id);
    }
    if let Some(id) = index.id(&format!("{candidate}.pyi")) {
        return Some(id);
    }
    None
}

pub fn resolve_specifier(
    from_path: &str,
    specifier: &str,
    names: &[String],
    index: &FileIndex,
    project: &PythonProject,
) -> Result<FileId, UnresolvedReason> {
    if specifier.is_empty() {
        return Err(UnresolvedReason::NoSuchFile(specifier.to_string()));
    }

    let leading_dots = specifier.chars().take_while(|c| *c == '.').count();

    if leading_dots > 0 {
        let mut base = parent_dir(from_path).to_string();
        for _ in 1..leading_dots {
            base = parent_dir(&base).to_string();
        }
        let rest = specifier[leading_dots..].replace('.', "/");
        let candidate = if rest.is_empty() {
            base.clone()
        } else {
            join(&base, &rest)
        };

        for name in names {
            if let Some(id) = try_module(&join(&candidate, name), index) {
                return Ok(id);
            }
        }
        if let Some(id) = try_module(&candidate, index) {
            return Ok(id);
        }
        return Err(UnresolvedReason::NoSuchFile(specifier.to_string()));
    }

    let as_path = specifier.replace('.', "/");
    for root in &project.roots {
        let candidate = join(root, &as_path);
        for name in names {
            if let Some(id) = try_module(&join(&candidate, name), index) {
                return Ok(id);
            }
        }
        if let Some(id) = try_module(&candidate, index) {
            return Ok(id);
        }
    }

    let package = specifier.split('.').next().unwrap_or(specifier).to_string();
    Err(UnresolvedReason::External(package))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Language;

    fn index(paths: &[&str]) -> FileIndex {
        let paths: Vec<String> = paths.iter().map(|s| s.to_string()).collect();
        let langs = vec![Language::Python; paths.len()];
        FileIndex::new(paths, langs)
    }

    fn project(files: &[&str]) -> PythonProject {
        let owned: Vec<String> = files.iter().map(|s| s.to_string()).collect();
        build_project(Path::new("/repo"), &owned)
    }

    #[test]
    fn an_absolute_import_resolves_from_the_repository_root() {
        let files = ["app/__init__.py", "app/thing.py", "main.py"];
        let idx = index(&files);
        let p = project(&files);
        let got = resolve_specifier("main.py", "app.thing", &[], &idx, &p);
        assert_eq!(got, Ok(idx.id("app/thing.py").unwrap()));
    }

    #[test]
    fn a_src_layout_is_resolved_too() {
        let files = ["src/app/__init__.py", "src/app/thing.py", "src/main.py"];
        let idx = index(&files);
        let p = project(&files);
        let got = resolve_specifier("src/main.py", "app.thing", &[], &idx, &p);
        assert_eq!(got, Ok(idx.id("src/app/thing.py").unwrap()));
    }

    #[test]
    fn importing_a_package_lands_on_its_init_file() {
        let files = ["app/__init__.py", "main.py"];
        let idx = index(&files);
        let p = project(&files);
        let got = resolve_specifier("main.py", "app", &[], &idx, &p);
        assert_eq!(got, Ok(idx.id("app/__init__.py").unwrap()));
    }

    #[test]
    fn a_single_dot_import_stays_in_the_current_package() {
        let files = ["pkg/__init__.py", "pkg/a.py", "pkg/b.py"];
        let idx = index(&files);
        let p = project(&files);
        let got = resolve_specifier("pkg/a.py", ".b", &[], &idx, &p);
        assert_eq!(got, Ok(idx.id("pkg/b.py").unwrap()));
    }

    #[test]
    fn each_extra_dot_climbs_one_package() {
        let files = [
            "pkg/__init__.py",
            "pkg/sub/__init__.py",
            "pkg/sub/a.py",
            "pkg/top.py",
        ];
        let idx = index(&files);
        let p = project(&files);
        let got = resolve_specifier("pkg/sub/a.py", "..top", &[], &idx, &p);
        assert_eq!(got, Ok(idx.id("pkg/top.py").unwrap()));
    }

    #[test]
    fn from_dot_import_name_resolves_the_named_submodule() {
        let files = ["pkg/__init__.py", "pkg/a.py", "pkg/sibling.py"];
        let idx = index(&files);
        let p = project(&files);
        let got = resolve_specifier("pkg/a.py", ".", &["sibling".into()], &idx, &p);
        assert_eq!(got, Ok(idx.id("pkg/sibling.py").unwrap()));
    }

    #[test]
    fn from_package_import_submodule_resolves_to_the_submodule() {
        let files = ["app/__init__.py", "app/deep/__init__.py", "main.py"];
        let idx = index(&files);
        let p = project(&files);
        let got = resolve_specifier("main.py", "app", &["deep".into()], &idx, &p);
        assert_eq!(got, Ok(idx.id("app/deep/__init__.py").unwrap()));
    }

    #[test]
    fn from_package_import_a_symbol_lands_on_the_package_itself() {
        let files = ["app/__init__.py", "main.py"];
        let idx = index(&files);
        let p = project(&files);
        let got = resolve_specifier("main.py", "app", &["SomeClass".into()], &idx, &p);
        assert_eq!(got, Ok(idx.id("app/__init__.py").unwrap()));
    }

    #[test]
    fn the_standard_library_is_external_not_broken() {
        let files = ["main.py"];
        let idx = index(&files);
        let p = project(&files);
        assert_eq!(
            resolve_specifier("main.py", "os.path", &[], &idx, &p),
            Err(UnresolvedReason::External("os".into()))
        );
        assert_eq!(
            resolve_specifier("main.py", "numpy", &[], &idx, &p),
            Err(UnresolvedReason::External("numpy".into()))
        );
    }

    #[test]
    fn a_relative_import_pointing_nowhere_is_a_real_failure() {
        let files = ["pkg/__init__.py", "pkg/a.py"];
        let idx = index(&files);
        let p = project(&files);
        assert_eq!(
            resolve_specifier("pkg/a.py", ".gone", &[], &idx, &p),
            Err(UnresolvedReason::NoSuchFile(".gone".into()))
        );
    }

    #[test]
    fn a_stub_file_resolves_when_there_is_no_source() {
        let files = ["typings/thing.pyi", "main.py"];
        let idx = index(&files);
        let mut p = project(&files);
        p.roots.push("typings".into());
        p.roots.sort_by_key(|r| std::cmp::Reverse(r.len()));
        let got = resolve_specifier("main.py", "thing", &[], &idx, &p);
        assert_eq!(got, Ok(idx.id("typings/thing.pyi").unwrap()));
    }
}
