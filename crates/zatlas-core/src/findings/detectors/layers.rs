use std::collections::HashMap;

use crate::config::ZatlasConfig;
use crate::findings::{Evidence, Finding, FindingKind, FindingsInput, Severity};
use crate::model::{EdgeKind, FileId};

pub type LayerMap = HashMap<FileId, String>;

pub fn layer_of(graph: &crate::model::Graph, config: &ZatlasConfig) -> LayerMap {
    let mut matchers: Vec<(String, globset::GlobSet)> = Vec::new();
    for layer in &config.layers {
        let mut builder = globset::GlobSetBuilder::new();
        for p in &layer.paths {
            if let Ok(g) = globset::Glob::new(p) {
                builder.add(g);
            }
        }
        if let Ok(set) = builder.build() {
            matchers.push((layer.name.clone(), set));
        }
    }

    let mut out = LayerMap::new();
    for f in &graph.files {
        for (name, set) in &matchers {
            if set.is_match(&f.path) {
                out.insert(f.id, name.clone());
                break;
            }
        }
    }
    out
}

pub fn detect(input: &FindingsInput) -> Vec<Finding> {
    let rules = input.config.layer_rules();
    if rules.is_empty() {
        return Vec::new();
    }
    let layers = layer_of(input.graph, input.config);
    if layers.is_empty() {
        return Vec::new();
    }

    let allowed: Vec<(String, String)> = rules
        .iter()
        .map(|r| (r.from.clone(), r.to.clone()))
        .collect();

    let mut out = Vec::new();
    for edge in &input.graph.edges {
        if matches!(edge.kind, EdgeKind::CoChange | EdgeKind::Contains) {
            continue;
        }
        let (Some(from_layer), Some(to_layer)) = (layers.get(&edge.from), layers.get(&edge.to))
        else {
            continue;
        };

        if from_layer == to_layer {
            continue;
        }
        if allowed
            .iter()
            .any(|(f, t)| f == from_layer && t == to_layer)
        {
            continue;
        }

        let from_path = input
            .graph
            .file(edge.from)
            .map(|f| f.path.as_str())
            .unwrap_or("?");
        let to_path = input
            .graph
            .file(edge.to)
            .map(|f| f.path.as_str())
            .unwrap_or("?");

        out.push(Finding {
            kind: FindingKind::LayeringViolation,
            severity: Severity::High,
            score: 0.88,
            headline: format!("{from_layer} → {to_layer}: {from_path} imports {to_path}"),
            files: vec![edge.from, edge.to],
            evidence: vec![
                Evidence::new("From layer", from_layer),
                Evidence::new("To layer", to_layer),
                Evidence::new("Declared rules", format!("{} allowed", allowed.len())),
            ],
            why: format!(
                "Your zatlas.toml does not allow {from_layer} to depend on \
                 {to_layer}. Layering rules exist so that the lower layers can be \
                 changed, tested and reused without dragging the upper ones along; \
                 each violation erodes that."
            ),
            how_to_fix: format!(
                "Either invert the dependency — have {to_layer} expose what \
                 {from_layer} needs, or pass the value in from a layer allowed to \
                 see both — or, if this direction is genuinely intended, add \
                 \"{from_layer} -> {to_layer}\" to the rules in zatlas.toml."
            ),
        });
    }
    out
}
