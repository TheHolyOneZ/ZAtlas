use tree_sitter::{Node, Tree};

use super::{
    node_line, node_text, unquote, DefKind, Definition, ExportItem, ImportKind, ImportedNames,
    ParsedFile, RawImport,
};

pub fn extract(tree: &Tree, source: &str, out: &mut ParsedFile) {
    let root = tree.root_node();
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        match node.kind() {
            "import_declaration" => imports(node, source, out),
            "function_declaration" | "method_declaration" => {
                declare(node, source, out, DefKind::Function)
            }
            "type_declaration" => type_declaration(node, source, out),
            "const_declaration" | "var_declaration" => value_declaration(node, source, out),
            _ => {}
        }
    }
}

fn imports(node: Node, source: &str, out: &mut ParsedFile) {
    let mut stack = vec![node];
    let mut cursor = node.walk();
    while let Some(current) = stack.pop() {
        if current.kind() == "import_spec" {
            let mut inner = current.walk();
            let path = current
                .children(&mut inner)
                .find(|c| {
                    c.kind() == "interpreted_string_literal" || c.kind() == "raw_string_literal"
                })
                .map(|c| unquote(node_text(c, source)).to_string());
            if let Some(specifier) = path {
                if !specifier.is_empty() {
                    out.imports.push(RawImport {
                        specifier,
                        line: node_line(current),
                        kind: ImportKind::Static,

                        names: ImportedNames::Namespace,
                        type_only: false,
                        scope: None,
                    });
                }
            }
            continue;
        }
        for child in current.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn is_exported(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_uppercase())
}

fn push(out: &mut ParsedFile, name: String, kind: DefKind, line: u32) {
    if name.is_empty() {
        return;
    }
    let exported = is_exported(&name);
    out.definitions.push(Definition {
        name: name.clone(),
        kind,
        line,
        exported,
    });
    if exported {
        out.exports.push(ExportItem {
            name,
            from: None,
            line,
        });
    }
}

fn declare(node: Node, source: &str, out: &mut ParsedFile, kind: DefKind) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    push(
        out,
        node_text(name_node, source).to_string(),
        kind,
        node_line(node),
    );
}

fn type_declaration(node: Node, source: &str, out: &mut ParsedFile) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "type_spec" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let kind = match child.child_by_field_name("type").map(|t| t.kind()) {
            Some("struct_type") => DefKind::Struct,
            Some("interface_type") => DefKind::Interface,
            _ => DefKind::TypeAlias,
        };
        push(
            out,
            node_text(name_node, source).to_string(),
            kind,
            node_line(child),
        );
    }
}

fn value_declaration(node: Node, source: &str, out: &mut ParsedFile) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !matches!(child.kind(), "const_spec" | "var_spec") {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        push(
            out,
            node_text(name_node, source).to_string(),
            DefKind::Const,
            node_line(child),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;
    use crate::model::Language;

    fn go(src: &str) -> ParsedFile {
        parse(Language::Go, src)
    }

    fn specs(p: &ParsedFile) -> Vec<&str> {
        p.imports.iter().map(|i| i.specifier.as_str()).collect()
    }

    #[test]
    fn a_single_import_is_found() {
        let p = go("package main\n\nimport \"fmt\"\n");
        assert_eq!(specs(&p), ["fmt"]);
    }

    #[test]
    fn a_grouped_import_block_yields_every_package() {
        let p = go(
            "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n\t\"github.com/user/repo/pkg\"\n)\n",
        );
        let mut got = specs(&p);
        got.sort_unstable();
        assert_eq!(got, ["fmt", "github.com/user/repo/pkg", "os"]);
    }

    #[test]
    fn an_aliased_import_records_the_package_path_not_the_alias() {
        let p = go("package main\n\nimport f \"fmt\"\n");
        assert_eq!(specs(&p), ["fmt"]);
    }

    #[test]
    fn a_blank_import_still_counts_as_a_dependency() {
        let p = go("package main\n\nimport _ \"github.com/lib/pq\"\n");
        assert_eq!(specs(&p), ["github.com/lib/pq"]);
    }

    #[test]
    fn a_go_import_is_always_a_whole_package() {
        let p = go("package main\n\nimport \"fmt\"\n");
        assert_eq!(p.imports[0].names, ImportedNames::Namespace);
    }

    #[test]
    fn capitalised_names_are_exported_and_lowercase_ones_are_not() {
        let p = go("package p\n\nfunc Public() {}\nfunc private() {}\n\
             type Thing struct{}\ntype hidden struct{}\n");
        let mut exports: Vec<&str> = p.exports.iter().map(|e| e.name.as_str()).collect();
        exports.sort_unstable();
        assert_eq!(exports, ["Public", "Thing"]);
    }

    #[test]
    fn structs_interfaces_and_aliases_are_classified() {
        let p = go("package p\n\ntype S struct{}\ntype I interface{}\ntype A = int\n");
        let kinds: Vec<DefKind> = p.definitions.iter().map(|d| d.kind).collect();
        assert!(kinds.contains(&DefKind::Struct), "{kinds:?}");
        assert!(kinds.contains(&DefKind::Interface), "{kinds:?}");
    }

    #[test]
    fn constants_are_declarations() {
        let p = go("package p\n\nconst Version = \"1.0\"\nvar Global = 2\n");
        let names: Vec<&str> = p.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"Version"), "{names:?}");
        assert!(names.contains(&"Global"), "{names:?}");
    }
}
