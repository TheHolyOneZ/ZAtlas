pub mod go;
pub mod javascript;
pub mod python;
pub mod rust;

use std::collections::HashMap;

use crate::model::{FileId, Language, UnresolvedReason};
use crate::parse::{ImportedNames, ParsedFile, RawImport};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    File(FileId),

    ViaBarrel(FileId),

    External(String),
    Unresolved(UnresolvedReason),
}

pub struct FileIndex {
    by_path: HashMap<String, FileId>,

    by_path_folded: HashMap<String, FileId>,
    pub languages: Vec<Language>,
    pub paths: Vec<String>,
}

impl FileIndex {
    pub fn new(paths: Vec<String>, languages: Vec<Language>) -> Self {
        let by_path: HashMap<String, FileId> = paths
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), FileId(i as u32)))
            .collect();

        let mut by_path_folded: HashMap<String, FileId> = HashMap::new();
        for (i, p) in paths.iter().enumerate() {
            by_path_folded
                .entry(p.to_lowercase())
                .or_insert(FileId(i as u32));
        }
        Self {
            by_path,
            by_path_folded,
            languages,
            paths,
        }
    }

    pub fn id(&self, path: &str) -> Option<FileId> {
        self.by_path.get(path).copied()
    }

    pub fn id_case_insensitive(&self, path: &str) -> Option<(FileId, bool)> {
        if let Some(id) = self.by_path.get(path) {
            return Some((*id, true));
        }
        self.by_path_folded
            .get(&path.to_lowercase())
            .map(|id| (*id, false))
    }

    pub fn path(&self, id: FileId) -> Option<&str> {
        self.paths.get(id.0 as usize).map(|s| s.as_str())
    }

    pub fn contains(&self, path: &str) -> bool {
        self.by_path.contains_key(path)
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

pub const MAX_REEXPORT_DEPTH: usize = 16;

pub trait Resolver {
    fn resolve(&self, from: FileId, import: &RawImport) -> Resolution;
}

pub fn follow_reexport(
    start: FileId,
    symbol: &str,
    parsed: &HashMap<FileId, ParsedFile>,
    resolve_specifier: &dyn Fn(FileId, &str) -> Option<FileId>,
) -> Option<FileId> {
    let mut current = start;
    let mut seen = vec![start];

    for _ in 0..MAX_REEXPORT_DEPTH {
        let file = parsed.get(&current)?;

        if file
            .exports
            .iter()
            .any(|e| e.name == symbol && e.from.is_none())
        {
            return Some(current);
        }

        let next_spec = file
            .exports
            .iter()
            .find(|e| e.name == symbol && e.from.is_some())
            .and_then(|e| e.from.as_deref())
            .or_else(|| {
                file.exports
                    .iter()
                    .find(|e| e.name == "*" && e.from.is_some())
                    .and_then(|e| e.from.as_deref())
            })?;

        let next = resolve_specifier(current, next_spec)?;
        if seen.contains(&next) {
            return None;
        }
        seen.push(next);
        current = next;
    }
    None
}

pub fn traceable_symbols(names: &ImportedNames) -> Option<&[String]> {
    match names {
        ImportedNames::Named(v) => Some(v),
        ImportedNames::Namespace | ImportedNames::Default | ImportedNames::SideEffect => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ExportItem;

    fn file_with(exports: Vec<ExportItem>) -> ParsedFile {
        ParsedFile {
            exports,
            ..Default::default()
        }
    }

    fn ex(name: &str, from: Option<&str>) -> ExportItem {
        ExportItem {
            name: name.into(),
            from: from.map(|s| s.to_string()),
            line: 1,
        }
    }

    #[test]
    fn a_symbol_defined_locally_resolves_to_that_file() {
        let mut parsed = HashMap::new();
        parsed.insert(FileId(0), file_with(vec![ex("x", None)]));
        let got = follow_reexport(FileId(0), "x", &parsed, &|_, _| None);
        assert_eq!(got, Some(FileId(0)));
    }

    #[test]
    fn a_barrel_resolves_to_the_file_that_actually_defines_the_symbol() {
        let mut parsed = HashMap::new();
        parsed.insert(FileId(0), file_with(vec![ex("x", Some("./real"))]));
        parsed.insert(FileId(1), file_with(vec![ex("x", None)]));
        let got = follow_reexport(FileId(0), "x", &parsed, &|_, spec| {
            (spec == "./real").then_some(FileId(1))
        });
        assert_eq!(got, Some(FileId(1)));
    }

    #[test]
    fn a_chain_of_barrels_is_followed_to_the_end() {
        let mut parsed = HashMap::new();
        parsed.insert(FileId(0), file_with(vec![ex("x", Some("b"))]));
        parsed.insert(FileId(1), file_with(vec![ex("x", Some("c"))]));
        parsed.insert(FileId(2), file_with(vec![ex("x", None)]));
        let got = follow_reexport(FileId(0), "x", &parsed, &|_, spec| match spec {
            "b" => Some(FileId(1)),
            "c" => Some(FileId(2)),
            _ => None,
        });
        assert_eq!(got, Some(FileId(2)));
    }

    #[test]
    fn a_star_re_export_is_followed_when_no_named_forward_matches() {
        let mut parsed = HashMap::new();
        parsed.insert(FileId(0), file_with(vec![ex("*", Some("./all"))]));
        parsed.insert(FileId(1), file_with(vec![ex("buried", None)]));
        let got = follow_reexport(FileId(0), "buried", &parsed, &|_, _| Some(FileId(1)));
        assert_eq!(got, Some(FileId(1)));
    }

    #[test]
    fn a_cycle_between_barrels_terminates_instead_of_looping() {
        let mut parsed = HashMap::new();
        parsed.insert(FileId(0), file_with(vec![ex("x", Some("b"))]));
        parsed.insert(FileId(1), file_with(vec![ex("x", Some("a"))]));
        let got = follow_reexport(FileId(0), "x", &parsed, &|_, spec| match spec {
            "b" => Some(FileId(1)),
            "a" => Some(FileId(0)),
            _ => None,
        });
        assert_eq!(got, None, "a barrel cycle must not hang the scan");
    }

    #[test]
    fn a_chain_longer_than_the_cap_gives_up_rather_than_running_forever() {
        let mut parsed = HashMap::new();
        for i in 0..(MAX_REEXPORT_DEPTH as u32 + 5) {
            parsed.insert(FileId(i), file_with(vec![ex("x", Some("next"))]));
        }
        let got = follow_reexport(FileId(0), "x", &parsed, &|from, _| Some(FileId(from.0 + 1)));
        assert_eq!(got, None);
    }

    #[test]
    fn a_namespace_import_cannot_be_traced_through_a_barrel() {
        assert!(traceable_symbols(&ImportedNames::Namespace).is_none());
        assert!(traceable_symbols(&ImportedNames::SideEffect).is_none());
        assert_eq!(
            traceable_symbols(&ImportedNames::Named(vec!["a".into()])),
            Some(&["a".to_string()][..])
        );
    }

    #[test]
    fn a_case_mismatch_resolves_but_is_reported_as_one() {
        let idx = FileIndex::new(vec!["src/foo.ts".into()], vec![Language::TypeScript]);
        assert_eq!(
            idx.id_case_insensitive("src/foo.ts"),
            Some((FileId(0), true))
        );
        assert_eq!(
            idx.id_case_insensitive("src/Foo.ts"),
            Some((FileId(0), false))
        );
        assert_eq!(idx.id_case_insensitive("src/nope.ts"), None);

        assert_eq!(idx.id("src/Foo.ts"), None);
    }

    #[test]
    fn the_index_round_trips_paths_and_ids() {
        let idx = FileIndex::new(
            vec!["a.ts".into(), "b/c.ts".into()],
            vec![Language::TypeScript, Language::TypeScript],
        );
        assert_eq!(idx.id("b/c.ts"), Some(FileId(1)));
        assert_eq!(idx.path(FileId(1)), Some("b/c.ts"));
        assert_eq!(idx.id("nope.ts"), None);
        assert!(idx.contains("a.ts"));
    }
}
