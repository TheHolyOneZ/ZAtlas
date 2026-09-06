mod go;
pub mod grammar;
mod javascript;
mod python;
mod rust;

use serde::{Deserialize, Serialize};

use crate::model::Language;
pub use grammar::Damage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportKind {
    Static,

    ReExport,

    Dynamic,

    Require,

    ModDecl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportedNames {
    Named(Vec<String>),

    Namespace,

    Default,

    SideEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawImport {
    pub specifier: String,
    pub line: u32,
    pub kind: ImportKind,
    pub names: ImportedNames,

    pub type_only: bool,

    pub scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportItem {
    pub name: String,

    pub from: Option<String>,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub kind: DefKind,
    pub line: u32,
    pub exported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum DefKind {
    Function,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    TypeAlias,
    Const,
    Impl,
    Module,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ParsedFile {
    pub imports: Vec<RawImport>,
    pub exports: Vec<ExportItem>,
    pub definitions: Vec<Definition>,

    pub loc: u32,
    pub total_lines: u32,
    pub errors: usize,
    pub missing: usize,
}

impl ParsedFile {
    pub fn is_clean(&self) -> bool {
        self.errors == 0 && self.missing == 0
    }

    pub fn reexport_specifiers(&self) -> Vec<&str> {
        self.exports
            .iter()
            .filter_map(|e| e.from.as_deref())
            .collect()
    }
}

pub fn parse(language: Language, source: &str) -> ParsedFile {
    let mut out = ParsedFile {
        total_lines: source.lines().count() as u32,
        loc: count_loc(language, source),
        ..Default::default()
    };

    let Some(mut parser) = grammar::parser_for(language) else {
        return out;
    };
    let Some(tree) = parser.parse(source, None) else {
        return out;
    };

    let d = grammar::damage(&tree);
    out.errors = d.errors;
    out.missing = d.missing;

    match language {
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx => {
            javascript::extract(&tree, source, &mut out);
        }
        Language::Rust => rust::extract(&tree, source, &mut out),
        Language::Python => python::extract(&tree, source, &mut out),
        Language::Go => go::extract(&tree, source, &mut out),
    }

    out.imports.sort_by_key(|i| (i.line, i.specifier.clone()));
    out.exports.sort_by_key(|e| (e.line, e.name.clone()));
    out.definitions.sort_by_key(|d| (d.line, d.name.clone()));

    out
}

fn count_loc(language: Language, source: &str) -> u32 {
    let line_comment = match language {
        Language::Python => "#",
        _ => "//",
    };
    let mut in_block = false;
    let mut count = 0u32;

    for line in source.lines() {
        let t = line.trim();
        if in_block {
            if t.contains("*/") {
                in_block = false;
            }
            continue;
        }
        if t.is_empty() {
            continue;
        }
        if t.starts_with("/*") {
            if !t.contains("*/") {
                in_block = true;
            }
            continue;
        }
        if t.starts_with(line_comment) {
            continue;
        }
        count += 1;
    }
    count
}

pub fn node_text<'a>(node: tree_sitter::Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}

pub fn node_line(node: tree_sitter::Node) -> u32 {
    node.start_position().row as u32 + 1
}

pub(crate) fn unquote(text: &str) -> &str {
    let bytes = text.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' || first == b'\'' || first == b'`') && first == last {
            return &text[1..text.len() - 1];
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loc_ignores_blank_lines_and_comments() {
        let src = "\
const a = 1;

// a comment
const b = 2;
/* block
   spanning
   lines */
const c = 3;
";
        let p = parse(Language::TypeScript, src);
        assert_eq!(p.loc, 3);
        assert_eq!(p.total_lines, 8);
    }

    #[test]
    fn python_now_reports_both_size_and_imports() {
        let p = parse(Language::Python, "import os\n\ndef f():\n    return 1\n");
        assert_eq!(p.loc, 3);
        assert_eq!(p.imports.len(), 1);
    }

    #[test]
    fn unquote_handles_every_string_style() {
        assert_eq!(unquote("\"./a\""), "./a");
        assert_eq!(unquote("'./a'"), "./a");
        assert_eq!(unquote("`./a`"), "./a");
        assert_eq!(unquote("./a"), "./a");
        assert_eq!(unquote("\""), "\"");
    }
}
