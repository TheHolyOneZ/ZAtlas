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
            "import_statement" => import_statement(node, source, out),
            "export_statement" => export_statement(node, source, out),
            _ => statement_definitions(node, source, out, false),
        }
    }

    deep_scan_calls(root, source, out);
}

fn import_statement(node: Node, source: &str, out: &mut ParsedFile) {
    let Some(spec_node) = node.child_by_field_name("source") else {
        return;
    };
    let specifier = unquote(node_text(spec_node, source)).to_string();
    let text = node_text(node, source);
    let type_only = text.starts_with("import type") || text.starts_with("import  type");

    let mut names = ImportedNames::SideEffect;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "import_clause" {
            continue;
        }
        names = import_clause_names(child, source);
    }

    out.imports.push(RawImport {
        specifier,
        line: node_line(node),
        kind: ImportKind::Static,
        names,
        type_only,
        scope: None,
    });
}

fn import_clause_names(clause: Node, source: &str) -> ImportedNames {
    let mut named = Vec::new();
    let mut has_default = false;
    let mut cursor = clause.walk();

    for child in clause.children(&mut cursor) {
        match child.kind() {
            "namespace_import" => return ImportedNames::Namespace,
            "named_imports" => {
                let mut inner = child.walk();
                for spec in child.children(&mut inner) {
                    if spec.kind() != "import_specifier" {
                        continue;
                    }

                    let name = spec
                        .child_by_field_name("name")
                        .map(|n| node_text(n, source))
                        .unwrap_or_default();
                    if !name.is_empty() {
                        named.push(name.to_string());
                    }
                }
            }
            "identifier" => has_default = true,
            _ => {}
        }
    }

    if !named.is_empty() {
        ImportedNames::Named(named)
    } else if has_default {
        ImportedNames::Default
    } else {
        ImportedNames::SideEffect
    }
}

fn export_statement(node: Node, source: &str, out: &mut ParsedFile) {
    let line = node_line(node);
    let source_node = node.child_by_field_name("source");
    let from = source_node.map(|n| unquote(node_text(n, source)).to_string());

    if let Some(spec) = &from {
        let text = node_text(node, source);
        let star = text.contains('*');
        let mut named = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != "export_clause" {
                continue;
            }
            let mut inner = child.walk();
            for s in child.children(&mut inner) {
                if s.kind() != "export_specifier" {
                    continue;
                }
                let name = s
                    .child_by_field_name("name")
                    .map(|n| node_text(n, source))
                    .unwrap_or_default();
                let alias = s
                    .child_by_field_name("alias")
                    .map(|n| node_text(n, source))
                    .unwrap_or(name);
                if !name.is_empty() {
                    named.push(name.to_string());
                    out.exports.push(ExportItem {
                        name: alias.to_string(),
                        from: Some(spec.clone()),
                        line,
                    });
                }
            }
        }

        out.imports.push(RawImport {
            specifier: spec.clone(),
            line,
            kind: ImportKind::ReExport,
            names: if star || named.is_empty() {
                ImportedNames::Namespace
            } else {
                ImportedNames::Named(named)
            },
            type_only: node_text(node, source).starts_with("export type"),
            scope: None,
        });

        if star {
            out.exports.push(ExportItem {
                name: "*".into(),
                from: Some(spec.clone()),
                line,
            });
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "export_clause" => {
                let mut inner = child.walk();
                for s in child.children(&mut inner) {
                    if s.kind() != "export_specifier" {
                        continue;
                    }
                    let name = s
                        .child_by_field_name("alias")
                        .or_else(|| s.child_by_field_name("name"))
                        .map(|n| node_text(n, source))
                        .unwrap_or_default();
                    if !name.is_empty() {
                        out.exports.push(ExportItem {
                            name: name.to_string(),
                            from: None,
                            line,
                        });
                    }
                }
            }
            _ => statement_definitions(child, source, out, true),
        }
    }

    if node_text(node, source).starts_with("export default") {
        out.exports.push(ExportItem {
            name: "default".into(),
            from: None,
            line,
        });
    }
}

fn statement_definitions(node: Node, source: &str, out: &mut ParsedFile, exported: bool) {
    let line = node_line(node);
    let mut push = |name: &str, kind: DefKind| {
        if name.is_empty() {
            return;
        }
        out.definitions.push(Definition {
            name: name.to_string(),
            kind,
            line,
            exported,
        });
        if exported {
            out.exports.push(ExportItem {
                name: name.to_string(),
                from: None,
                line,
            });
        }
    };

    let named = |n: Node| -> String {
        n.child_by_field_name("name")
            .map(|x| node_text(x, source).to_string())
            .unwrap_or_default()
    };

    match node.kind() {
        "function_declaration" | "generator_function_declaration" => {
            push(&named(node), DefKind::Function)
        }
        "class_declaration" => push(&named(node), DefKind::Class),
        "interface_declaration" => push(&named(node), DefKind::Interface),
        "type_alias_declaration" => push(&named(node), DefKind::TypeAlias),
        "enum_declaration" => push(&named(node), DefKind::Enum),
        "lexical_declaration" | "variable_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "variable_declarator" {
                    continue;
                }
                let name = named(child);

                let kind = match child.child_by_field_name("value").map(|v| v.kind()) {
                    Some("arrow_function") | Some("function_expression") => DefKind::Function,
                    Some("class") => DefKind::Class,
                    _ => DefKind::Const,
                };
                push(&name, kind);
            }
        }
        _ => {}
    }
}

fn deep_scan_calls(root: Node, source: &str, out: &mut ParsedFile) {
    let mut stack = vec![root];
    let mut cursor = root.walk();

    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            if let Some(f) = node.child_by_field_name("function") {
                let kind = match f.kind() {
                    "import" => Some(ImportKind::Dynamic),
                    _ if node_text(f, source) == "require" => Some(ImportKind::Require),
                    _ => None,
                };
                if let Some(kind) = kind {
                    if let Some(args) = node.child_by_field_name("arguments") {
                        let mut inner = args.walk();
                        let literal = args
                            .children(&mut inner)
                            .find(|c| c.kind() == "string")
                            .map(|c| unquote(node_text(c, source)).to_string());

                        if let Some(specifier) = literal {
                            out.imports.push(RawImport {
                                specifier,
                                line: node_line(node),
                                kind,
                                names: ImportedNames::Namespace,
                                type_only: false,
                                scope: None,
                            });
                        }
                    }
                }
            }
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;
    use crate::model::Language;

    fn ts(src: &str) -> ParsedFile {
        parse(Language::TypeScript, src)
    }

    fn specs(p: &ParsedFile) -> Vec<&str> {
        p.imports.iter().map(|i| i.specifier.as_str()).collect()
    }

    #[test]
    fn reads_every_import_form() {
        let p = ts(r#"
import a from "./a";
import { b, c } from "./bc";
import * as ns from "./ns";
import "./side-effect";
import type { T } from "./types";
"#);
        assert_eq!(
            specs(&p),
            ["./a", "./bc", "./ns", "./side-effect", "./types"]
        );
        assert_eq!(p.imports[0].names, ImportedNames::Default);
        assert_eq!(
            p.imports[1].names,
            ImportedNames::Named(vec!["b".into(), "c".into()])
        );
        assert_eq!(p.imports[2].names, ImportedNames::Namespace);
        assert_eq!(p.imports[3].names, ImportedNames::SideEffect);
        assert!(p.imports[4].type_only);
    }

    #[test]
    fn an_aliased_import_records_the_name_in_the_source_module() {
        let p = ts(r#"import { a as b } from "./x";"#);
        assert_eq!(p.imports[0].names, ImportedNames::Named(vec!["a".into()]));
    }

    #[test]
    fn a_re_export_is_both_an_export_and_an_import() {
        let p = ts(r#"export { a, b } from "./ab";"#);
        assert_eq!(specs(&p), ["./ab"]);
        assert_eq!(p.imports[0].kind, ImportKind::ReExport);
        assert_eq!(p.exports.len(), 2);
        assert_eq!(p.exports[0].from.as_deref(), Some("./ab"));
    }

    #[test]
    fn a_star_re_export_is_recorded_so_the_chain_can_be_followed() {
        let p = ts(r#"export * from "./everything";"#);
        assert_eq!(p.imports[0].kind, ImportKind::ReExport);
        assert_eq!(p.imports[0].names, ImportedNames::Namespace);
        assert!(p.exports.iter().any(|e| e.name == "*"));
    }

    #[test]
    fn an_aliased_re_export_records_the_outward_facing_name() {
        let p = ts(r#"export { internal as public } from "./x";"#);
        assert_eq!(p.exports[0].name, "public");
        assert_eq!(p.exports[0].from.as_deref(), Some("./x"));
    }

    #[test]
    fn local_exports_are_found_in_every_declaration_form() {
        let p = ts(r#"
export const a = 1;
export function b() {}
export class C {}
export interface I {}
export type T = string;
export enum E { X }
const hidden = 2;
export { hidden };
"#);
        let mut names: Vec<&str> = p.exports.iter().map(|e| e.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["C", "E", "I", "T", "a", "b", "hidden"]);
    }

    #[test]
    fn export_default_is_recorded_under_that_name() {
        let p = ts("export default function main() {}");
        assert!(p.exports.iter().any(|e| e.name == "default"));
    }

    #[test]
    fn an_arrow_function_assigned_to_a_const_counts_as_a_function() {
        let p = ts("export const go = () => {};");
        assert_eq!(p.definitions[0].kind, DefKind::Function);
    }

    #[test]
    fn dynamic_import_and_require_are_found_at_any_depth() {
        let p = ts(r#"
async function load() {
  if (cond) {
    const m = await import("./lazy");
    const n = require("./legacy");
    return [m, n];
  }
}
"#);
        assert_eq!(specs(&p), ["./lazy", "./legacy"]);
        assert_eq!(p.imports[0].kind, ImportKind::Dynamic);
        assert_eq!(p.imports[1].kind, ImportKind::Require);
    }

    #[test]
    fn a_computed_dynamic_import_yields_no_false_specifier() {
        let p = ts(r#"const m = await import(`./locales/${lang}`);"#);
        assert!(p.imports.is_empty());
    }

    #[test]
    fn tsx_parses_with_the_tsx_grammar() {
        let p = parse(
            Language::Tsx,
            r#"import React from "react";
export const V = () => <div className="x">hi</div>;"#,
        );
        assert_eq!(p.imports[0].specifier, "react");
        assert!(p.is_clean(), "TSX should parse cleanly, got {p:?}");
    }

    #[test]
    fn a_barrel_records_each_forwarded_module_separately() {
        let p = ts(r#"
export { a } from "./a";
export { b } from "./b";
export * from "./c";
"#);
        assert_eq!(specs(&p), ["./a", "./b", "./c"]);

        assert_eq!(p.reexport_specifiers(), ["./a", "./b", "./c"]);
    }

    #[test]
    fn a_file_that_does_not_parse_reports_damage_rather_than_pretending() {
        let p = ts("import { from './broken");
        assert!(!p.is_clean());
    }
}
