use tree_sitter::{Node, Tree};

use super::{
    node_line, node_text, DefKind, Definition, ExportItem, ImportKind, ImportedNames, ParsedFile,
    RawImport,
};

pub fn extract(tree: &Tree, source: &str, out: &mut ParsedFile) {
    let root = tree.root_node();
    walk(root, source, out, 0);
}

fn walk(parent: Node, source: &str, out: &mut ParsedFile, depth: u32) {
    let mut cursor = parent.walk();
    for node in parent.children(&mut cursor) {
        match node.kind() {
            "import_statement" => import_statement(node, source, out),
            "import_from_statement" => import_from(node, source, out),
            "function_definition" => {
                define(node, source, out, DefKind::Function, depth == 0);

                if let Some(body) = node.child_by_field_name("body") {
                    walk(body, source, out, depth + 1);
                }
            }
            "class_definition" => {
                define(node, source, out, DefKind::Class, depth == 0);

                if let Some(body) = node.child_by_field_name("body") {
                    walk(body, source, out, depth + 1);
                }
            }
            "decorated_definition" => walk(node, source, out, depth),
            "expression_statement" if depth == 0 => {
                assignment(node, source, out);
            }

            "if_statement" | "try_statement" | "with_statement" | "for_statement"
            | "while_statement" | "block" | "else_clause" | "elif_clause" | "except_clause" => {
                walk(node, source, out, depth + 1);
            }
            _ => {}
        }
    }
}

fn define(node: Node, source: &str, out: &mut ParsedFile, kind: DefKind, top_level: bool) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(name_node, source).to_string();
    if name.is_empty() {
        return;
    }

    let exported = top_level && !name.starts_with('_');
    out.definitions.push(Definition {
        name: name.clone(),
        kind,
        line: node_line(node),
        exported,
    });
    if exported {
        out.exports.push(ExportItem {
            name,
            from: None,
            line: node_line(node),
        });
    }
}

fn assignment(node: Node, source: &str, out: &mut ParsedFile) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "assignment" {
            continue;
        }
        let Some(left) = child.child_by_field_name("left") else {
            continue;
        };
        if left.kind() != "identifier" {
            continue;
        }
        let name = node_text(left, source).to_string();
        if name.is_empty() || name.starts_with('_') {
            continue;
        }
        out.definitions.push(Definition {
            name: name.clone(),
            kind: DefKind::Const,
            line: node_line(child),
            exported: true,
        });
        out.exports.push(ExportItem {
            name,
            from: None,
            line: node_line(child),
        });
    }
}

fn import_statement(node: Node, source: &str, out: &mut ParsedFile) {
    let line = node_line(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let specifier = match child.kind() {
            "dotted_name" => node_text(child, source).to_string(),
            "aliased_import" => child
                .child_by_field_name("name")
                .map(|n| node_text(n, source).to_string())
                .unwrap_or_default(),
            _ => continue,
        };
        if specifier.is_empty() {
            continue;
        }
        out.imports.push(RawImport {
            specifier,
            line,
            kind: ImportKind::Static,
            names: ImportedNames::Namespace,
            type_only: false,
            scope: None,
        });
    }
}

fn import_from(node: Node, source: &str, out: &mut ParsedFile) {
    let line = node_line(node);
    let text = node_text(node, source);

    let module = node
        .child_by_field_name("module_name")
        .map(|n| node_text(n, source).to_string())
        .unwrap_or_default();

    let dots = text
        .strip_prefix("from")
        .map(|rest| rest.trim_start())
        .map(|rest| rest.chars().take_while(|c| *c == '.').count())
        .unwrap_or(0);

    let specifier = if module.starts_with('.') || dots == 0 {
        module.clone()
    } else {
        format!("{}{}", ".".repeat(dots), module)
    };

    if specifier.is_empty() {
        return;
    }

    let star = text.contains(" import *");
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "dotted_name" => {
                let t = node_text(child, source);
                if t != module.trim_start_matches('.') && !specifier.ends_with(t) {
                    names.push(t.to_string());
                }
            }
            "aliased_import" => {
                if let Some(n) = child.child_by_field_name("name") {
                    names.push(node_text(n, source).to_string());
                }
            }
            _ => {}
        }
    }
    names.retain(|n| !n.is_empty());

    out.imports.push(RawImport {
        specifier,
        line,
        kind: ImportKind::Static,
        names: if star || names.is_empty() {
            ImportedNames::Namespace
        } else {
            ImportedNames::Named(names)
        },
        type_only: false,
        scope: None,
    });
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;
    use crate::model::Language;

    fn py(src: &str) -> ParsedFile {
        parse(Language::Python, src)
    }

    fn specs(p: &ParsedFile) -> Vec<&str> {
        p.imports.iter().map(|i| i.specifier.as_str()).collect()
    }

    #[test]
    fn a_plain_import_names_the_module() {
        let p = py("import os\nimport a.b.c\n");
        assert_eq!(specs(&p), ["os", "a.b.c"]);
    }

    #[test]
    fn multiple_modules_on_one_line_are_separate_imports() {
        let p = py("import os, sys\n");
        assert_eq!(specs(&p), ["os", "sys"]);
    }

    #[test]
    fn an_aliased_import_records_the_real_module() {
        let p = py("import numpy as np\n");
        assert_eq!(specs(&p), ["numpy"]);
    }

    #[test]
    fn from_import_records_the_module_and_the_symbols() {
        let p = py("from pkg.mod import thing, other\n");
        assert_eq!(specs(&p), ["pkg.mod"]);
        assert_eq!(
            p.imports[0].names,
            ImportedNames::Named(vec!["thing".into(), "other".into()])
        );
    }

    #[test]
    fn a_relative_import_keeps_its_leading_dots() {
        let p = py("from . import sibling\nfrom .. import parent\nfrom .mod import x\n");
        let got = specs(&p);
        assert!(got.contains(&"."), "got {got:?}");
        assert!(got.contains(&".."), "got {got:?}");
        assert!(
            got.iter().any(|s| s.starts_with('.') && s.contains("mod")),
            "got {got:?}"
        );
    }

    #[test]
    fn a_star_import_is_a_namespace_import() {
        let p = py("from pkg.mod import *\n");
        assert_eq!(p.imports[0].names, ImportedNames::Namespace);
    }

    #[test]
    fn imports_inside_a_function_or_type_checking_block_still_count() {
        let p = py("from typing import TYPE_CHECKING\n\
             if TYPE_CHECKING:\n    from pkg.types import Thing\n\n\
             def f():\n    import lazy_module\n    return lazy_module\n");
        let got = specs(&p);
        assert!(got.contains(&"pkg.types"), "got {got:?}");
        assert!(got.contains(&"lazy_module"), "got {got:?}");
    }

    #[test]
    fn top_level_definitions_are_found() {
        let p = py("def f():\n    pass\n\nclass C:\n    def method(self):\n        pass\n");
        let names: Vec<&str> = p.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"f"), "got {names:?}");
        assert!(names.contains(&"C"), "got {names:?}");
    }

    #[test]
    fn an_underscore_prefixed_name_is_not_exported() {
        let p = py("def public():\n    pass\n\ndef _private():\n    pass\n");
        let exports: Vec<&str> = p.exports.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(exports, ["public"]);
    }

    #[test]
    fn a_module_level_constant_is_an_export() {
        let p = py("VERSION = \"1.0\"\n_INTERNAL = 2\n");
        let exports: Vec<&str> = p.exports.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(exports, ["VERSION"]);
    }

    #[test]
    fn a_decorated_function_is_still_found() {
        let p = py("@app.route('/')\ndef handler():\n    pass\n");
        assert!(p.definitions.iter().any(|d| d.name == "handler"), "{p:?}");
    }
}
