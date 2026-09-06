use crate::findings::{Evidence, Finding, FindingKind, FindingsInput, Severity};
use crate::graph::orphans;

pub fn can_be_dead(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);

    if name.ends_with(".d.ts") {
        return false;
    }

    if name.contains(".config.") || matches!(name, "vite.config.ts" | "next.config.js") {
        return false;
    }

    if path.contains(".test.")
        || path.contains(".spec.")
        || path.contains("/tests/")
        || path.contains("/test/")
        || path.contains("/examples/")
        || path.contains("/benches/")
        || path.contains("__tests__")
    {
        return false;
    }

    if name.ends_with(".mjs") || name.ends_with(".cjs") {
        return false;
    }

    if !path.contains('/') && name.ends_with(".js") {
        return false;
    }

    true
}

pub fn detect(input: &FindingsInput) -> Vec<Finding> {
    orphans(input.structural, &input.entrypoints)
        .into_iter()
        .filter_map(|id| {
            let f = input.graph.file(id)?;

            let path = f.path.as_str();
            if !can_be_dead(path) {
                return None;
            }

            Some(Finding {
                kind: FindingKind::Orphan,
                severity: Severity::Low,
                score: 0.45,
                headline: format!("{path} is never imported ({} lines)", f.loc),
                files: vec![id],
                evidence: vec![
                    Evidence::new("Lines of code", f.loc),
                    Evidence::new("Files depending on it", 0),
                ],
                why: "Nothing imports this file and no entry point reaches it, so \
                      it is probably dead. Dead code still gets read, maintained \
                      and searched, and it misleads anyone learning the codebase."
                    .into(),
                how_to_fix: "Confirm it is unused, then delete it — git remembers. \
                             If it is reached in a way ZAtlas cannot see (a build \
                             script, a plugin loader, a dynamic import), add it to \
                             `entrypoints` in zatlas.toml."
                    .into(),
            })
        })
        .collect()
}
