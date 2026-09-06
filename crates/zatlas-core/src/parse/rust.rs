use tree_sitter::{Node, Tree};

use super::{
    node_line, node_text, DefKind, Definition, ExportItem, ImportKind, ImportedNames, ParsedFile,
    RawImport,
};

pub fn extract(tree: &Tree, source: &str, out: &mut ParsedFile) {
    let root = tree.root_node();
    walk_items(root, source, out, None);
}

fn walk_items(parent: Node, source: &str, out: &mut ParsedFile, scope: Option<&str>) {
    let mut cursor = parent.walk();
    for node in parent.children(&mut cursor) {
        match node.kind() {
            "use_declaration" => use_declaration(node, source, out, scope),
            "mod_item" => mod_item(node, source, out, scope),
            "function_item" => item(node, source, out, DefKind::Function),
            "struct_item" => item(node, source, out, DefKind::Struct),
            "enum_item" => item(node, source, out, DefKind::Enum),
            "trait_item" => item(node, source, out, DefKind::Trait),
            "type_item" => item(node, source, out, DefKind::TypeAlias),
            "const_item" | "static_item" => item(node, source, out, DefKind::Const),
            "union_item" => item(node, source, out, DefKind::Struct),
            "impl_item" => {
                let name = node
                    .child_by_field_name("type")
                    .map(|t| node_text(t, source).to_string())
                    .unwrap_or_default();
                if !name.is_empty() {
                    out.definitions.push(Definition {
                        name,
                        kind: DefKind::Impl,
                        line: node_line(node),
                        exported: false,
                    });
                }

                if let Some(body) = node.child_by_field_name("body") {
                    walk_items(body, source, out, scope);
                }
            }
            "macro_definition" => item(node, source, out, DefKind::Function),
            _ => {}
        }
    }
}

fn is_pub(node: Node, source: &str) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|c| c.kind() == "visibility_modifier" && node_text(c, source).starts_with("pub"));
    found
}

fn item(node: Node, source: &str, out: &mut ParsedFile, kind: DefKind) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(name_node, source).to_string();
    if name.is_empty() {
        return;
    }
    let exported = is_pub(node, source);
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

fn mod_item(node: Node, source: &str, out: &mut ParsedFile, scope: Option<&str>) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = node_text(name_node, source).to_string();
    let line = node_line(node);
    let inline = node.child_by_field_name("body").is_some();

    out.definitions.push(Definition {
        name: name.clone(),
        kind: DefKind::Module,
        line,
        exported: is_pub(node, source),
    });

    if inline {
        if let Some(body) = node.child_by_field_name("body") {
            let nested = match scope {
                Some(outer) => format!("{outer}::{name}"),
                None => name.clone(),
            };
            walk_items(body, source, out, Some(&nested));
        }
        return;
    }

    let specifier = match path_attribute(node, source) {
        Some(p) => format!("{name}\u{1}{p}"),
        None => name,
    };

    out.imports.push(RawImport {
        specifier,
        line,
        kind: ImportKind::ModDecl,
        names: ImportedNames::Namespace,
        type_only: false,
        scope: scope.map(|s| s.to_string()),
    });
}

fn path_attribute(node: Node, source: &str) -> Option<String> {
    let mut sibling = node.prev_sibling();
    while let Some(s) = sibling {
        if s.kind() == "attribute_item" {
            let text = node_text(s, source);
            if let Some(rest) = text.split_once("path") {
                if let Some(open) = rest.1.find(['"', '\'']) {
                    let after = &rest.1[open + 1..];
                    if let Some(end) = after.find(['"', '\'']) {
                        return Some(after[..end].to_string());
                    }
                }
            }
            sibling = s.prev_sibling();
            continue;
        }
        break;
    }
    None
}

fn use_declaration(node: Node, source: &str, out: &mut ParsedFile, scope: Option<&str>) {
    let line = node_line(node);
    let reexport = is_pub(node, source);
    let Some(arg) = node.child_by_field_name("argument") else {
        return;
    };

    let mut leaves = Vec::new();
    expand(arg, source, String::new(), &mut leaves);

    for (path, alias, glob) in leaves {
        if path.is_empty() {
            continue;
        }
        let names = if glob {
            ImportedNames::Namespace
        } else {
            let leaf = path.rsplit("::").next().unwrap_or(&path).to_string();
            ImportedNames::Named(vec![leaf])
        };

        if reexport {
            out.exports.push(ExportItem {
                name: alias
                    .clone()
                    .unwrap_or_else(|| path.rsplit("::").next().unwrap_or(&path).to_string()),
                from: Some(path.clone()),
                line,
            });
        }

        out.imports.push(RawImport {
            specifier: path,
            line,
            kind: if reexport {
                ImportKind::ReExport
            } else {
                ImportKind::Static
            },
            names,
            type_only: false,
            scope: scope.map(|s| s.to_string()),
        });
    }
}

fn expand(node: Node, source: &str, prefix: String, out: &mut Vec<(String, Option<String>, bool)>) {
    let join = |prefix: &str, seg: &str| -> String {
        if prefix.is_empty() {
            seg.to_string()
        } else {
            format!("{prefix}::{seg}")
        }
    };

    match node.kind() {
        "scoped_identifier" | "identifier" | "crate" | "super" | "self" | "metavariable" => {
            out.push((join(&prefix, node_text(node, source)), None, false));
        }
        "use_as_clause" => {
            let path = node
                .child_by_field_name("path")
                .map(|p| join(&prefix, node_text(p, source)))
                .unwrap_or_default();
            let alias = node
                .child_by_field_name("alias")
                .map(|a| node_text(a, source).to_string());
            out.push((path, alias, false));
        }
        "use_wildcard" => {
            let mut cursor = node.walk();
            let base = node
                .children(&mut cursor)
                .find(|c| c.kind() != "*" && c.kind() != "::")
                .map(|c| join(&prefix, node_text(c, source)))
                .unwrap_or_else(|| prefix.clone());
            out.push((base, None, true));
        }
        "scoped_use_list" => {
            let base = node
                .child_by_field_name("path")
                .map(|p| join(&prefix, node_text(p, source)))
                .unwrap_or_else(|| prefix.clone());
            if let Some(list) = node.child_by_field_name("list") {
                expand(list, source, base, out);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "{" | "}" | ",") {
                    continue;
                }
                expand(child, source, prefix.clone(), out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;
    use crate::model::Language;

    fn rs(src: &str) -> ParsedFile {
        parse(Language::Rust, src)
    }

    fn specs(p: &ParsedFile) -> Vec<&str> {
        p.imports.iter().map(|i| i.specifier.as_str()).collect()
    }

    #[test]
    fn a_simple_use_is_recorded_with_its_full_path() {
        let p = rs("use crate::auth::session;");
        assert_eq!(specs(&p), ["crate::auth::session"]);
        assert_eq!(p.imports[0].kind, ImportKind::Static);
    }

    #[test]
    fn a_use_list_expands_into_one_import_per_leaf() {
        let p = rs("use crate::a::{b, c, d};");
        assert_eq!(specs(&p), ["crate::a::b", "crate::a::c", "crate::a::d"]);
    }

    #[test]
    fn nested_use_lists_expand_correctly() {
        let p = rs("use crate::{a::{x, y}, b::z};");
        let mut got = specs(&p);
        got.sort_unstable();
        assert_eq!(got, ["crate::a::x", "crate::a::y", "crate::b::z"]);
    }

    #[test]
    fn an_aliased_use_keeps_the_source_path() {
        let p = rs("use crate::very::long::Name as Short;");
        assert_eq!(specs(&p), ["crate::very::long::Name"]);
    }

    #[test]
    fn a_glob_use_is_a_namespace_import() {
        let p = rs("use crate::prelude::*;");
        assert_eq!(specs(&p), ["crate::prelude"]);
        assert_eq!(p.imports[0].names, ImportedNames::Namespace);
    }

    #[test]
    fn self_and_super_paths_are_preserved_for_the_resolver() {
        let p = rs("use super::sibling::Thing;\nuse self::child::Other;");
        let mut got = specs(&p);
        got.sort_unstable();
        assert_eq!(got, ["self::child::Other", "super::sibling::Thing"]);
    }

    #[test]
    fn pub_use_is_a_re_export_not_just_an_import() {
        let p = rs("pub use crate::inner::Thing;");
        assert_eq!(p.imports[0].kind, ImportKind::ReExport);
        assert_eq!(p.exports.len(), 1);
        assert_eq!(p.exports[0].name, "Thing");
        assert_eq!(p.exports[0].from.as_deref(), Some("crate::inner::Thing"));
    }

    #[test]
    fn a_pub_use_alias_is_exported_under_the_alias() {
        let p = rs("pub use crate::inner::Thing as Renamed;");
        assert_eq!(p.exports[0].name, "Renamed");
    }

    #[test]
    fn mod_declarations_are_the_file_edges() {
        let p = rs("mod a;\npub mod b;");
        assert_eq!(specs(&p), ["a", "b"]);
        assert!(p.imports.iter().all(|i| i.kind == ImportKind::ModDecl));
    }

    #[test]
    fn an_inline_mod_declares_no_file_dependency_but_its_items_still_count() {
        let p = rs("mod inline { pub fn helper() {} pub struct S; }");
        assert!(
            p.imports.is_empty(),
            "an inline module lives in this file, so it is not an edge"
        );
        let names: Vec<&str> = p.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"helper"), "got {names:?}");
        assert!(names.contains(&"S"), "got {names:?}");
    }

    #[test]
    fn a_path_attribute_overrides_where_the_module_lives() {
        let p = rs("#[path = \"actually/here.rs\"]\nmod weird;");
        assert_eq!(specs(&p), ["weird\u{1}actually/here.rs"]);
    }

    #[test]
    fn a_cfg_attr_before_a_mod_does_not_confuse_the_path_lookup() {
        let p = rs("#[cfg(test)]\nmod tests;");
        assert_eq!(specs(&p), ["tests"]);
    }

    #[test]
    fn a_use_inside_an_inline_module_records_that_scope() {
        let p = rs("#[cfg(test)]\nmod tests {\n    use super::*;\n}");
        assert_eq!(p.imports.len(), 1);
        assert_eq!(p.imports[0].specifier, "super");
        assert_eq!(p.imports[0].scope.as_deref(), Some("tests"));
    }

    #[test]
    fn nested_inline_modules_accumulate_their_scope() {
        let p = rs("mod a {\n  mod b {\n    use super::super::Thing;\n  }\n}");
        assert_eq!(p.imports[0].scope.as_deref(), Some("a::b"));
    }

    #[test]
    fn a_file_level_use_has_no_scope() {
        let p = rs("use crate::a::B;");
        assert_eq!(p.imports[0].scope, None);
    }

    #[test]
    fn public_items_are_exports_and_private_ones_are_not() {
        let p = rs("pub fn a() {}\nfn b() {}\npub struct C;\nstruct D;");
        let mut names: Vec<&str> = p.exports.iter().map(|e| e.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["C", "a"]);
    }

    #[test]
    fn methods_inside_an_impl_block_are_definitions() {
        let p = rs("pub struct S;\nimpl S {\n    pub fn go() {}\n    fn hidden() {}\n}");
        let names: Vec<&str> = p.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"go"), "got {names:?}");
        assert!(names.contains(&"hidden"), "got {names:?}");
    }

    #[test]
    fn a_trait_impl_still_records_its_methods() {
        let p = rs("impl Display for S {\n    fn fmt(&self) {}\n}");
        assert!(p.definitions.iter().any(|d| d.name == "fmt"), "{p:?}");
    }

    #[test]
    fn every_item_kind_is_classified() {
        let p = rs(r#"
pub fn f() {}
pub struct S;
pub enum E { A }
pub trait T {}
pub type A = u8;
pub const C: u8 = 1;
impl S {}
"#);
        let kinds: Vec<DefKind> = p.definitions.iter().map(|d| d.kind).collect();
        for expected in [
            DefKind::Function,
            DefKind::Struct,
            DefKind::Enum,
            DefKind::Trait,
            DefKind::TypeAlias,
            DefKind::Const,
            DefKind::Impl,
        ] {
            assert!(
                kinds.contains(&expected),
                "missing {expected:?} in {kinds:?}"
            );
        }
    }

    #[test]
    fn external_crate_paths_are_recorded_verbatim_for_the_resolver_to_classify() {
        let p = rs("use serde::Serialize;\nuse std::collections::HashMap;");
        let mut got = specs(&p);
        got.sort_unstable();
        assert_eq!(got, ["serde::Serialize", "std::collections::HashMap"]);
    }
}
