use tree_sitter::{Parser, Query};

use crate::model::Language;

pub fn parser_for(language: Language) -> Option<Parser> {
    let mut parser = Parser::new();
    let lang: tree_sitter::Language = match language {
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Language::JavaScript | Language::Jsx => tree_sitter_javascript::LANGUAGE.into(),
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::Go => tree_sitter_go::LANGUAGE.into(),
    };
    parser.set_language(&lang).ok()?;
    Some(parser)
}

pub fn ts_language(language: Language) -> Option<tree_sitter::Language> {
    Some(match language {
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Language::JavaScript | Language::Jsx => tree_sitter_javascript::LANGUAGE.into(),
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::Go => tree_sitter_go::LANGUAGE.into(),
    })
}

pub fn compile_query(language: Language, source: &str) -> Option<Query> {
    let lang = ts_language(language)?;
    match Query::new(&lang, source) {
        Ok(q) => Some(q),
        Err(e) => {
            debug_assert!(false, "query failed to compile for {language:?}: {e}");
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Damage {
    pub errors: usize,
    pub missing: usize,
}

impl Damage {
    pub fn is_clean(&self) -> bool {
        self.errors == 0 && self.missing == 0
    }
}

pub fn damage(tree: &tree_sitter::Tree) -> Damage {
    let mut out = Damage::default();
    let mut cursor = tree.walk();
    let mut stack = vec![tree.root_node()];

    while let Some(node) = stack.pop() {
        if node.is_error() {
            out.errors += 1;

            continue;
        }
        if node.is_missing() {
            out.missing += 1;
        }
        if node.has_error() {
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_with_a_resolver_has_a_working_parser() {
        for language in Language::RESOLVED {
            let mut parser = parser_for(language)
                .unwrap_or_else(|| panic!("{language:?} claims a resolver but has no parser"));
            assert!(
                parser.parse("", None).is_some(),
                "{language:?} parser produced no tree"
            );
        }
    }

    #[test]
    fn every_supported_language_parses() {
        for language in [Language::Python, Language::Go] {
            let mut parser = parser_for(language).expect("grammar available");
            assert!(parser.parse("", None).is_some());
        }
    }

    #[test]
    fn clean_source_reports_no_damage() {
        let mut parser = parser_for(Language::Rust).unwrap();
        let tree = parser.parse("pub fn a() -> u32 { 1 }", None).unwrap();
        assert!(damage(&tree).is_clean());
    }

    #[test]
    fn broken_source_reports_damage() {
        let mut parser = parser_for(Language::Rust).unwrap();
        let tree = parser.parse("pub fn ( { { { ]]] ###", None).unwrap();
        assert!(!damage(&tree).is_clean());
    }

    #[test]
    fn a_deeply_nested_expression_does_not_blow_the_stack() {
        let src = format!("const x = {}1{};", "(".repeat(2000), ")".repeat(2000));
        let mut parser = parser_for(Language::TypeScript).unwrap();
        let tree = parser.parse(&src, None).unwrap();
        let _ = damage(&tree);
    }
}
