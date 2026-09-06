use serde_json::json;
use std::path::Path;

use zatlas_core::baseline;
use zatlas_core::export::diagram::{to_d2, to_mermaid, DiagramOptions};
use zatlas_core::export::report::to_markdown;
use zatlas_core::findings::{Finding, FindingKind, Severity};
use zatlas_core::graph::{self, tangles};
use zatlas_core::model::{FileId, UnresolvedReason};
use zatlas_core::tour::{self, TourOptions};

use crate::load::{load, LoadOptions, Loaded};
use crate::style::{severity_word, Style};
use crate::{FormatArg, ScopeArg};

pub struct FindingsArgs {
    pub kinds: Vec<FindingKind>,
    pub severity: Option<Severity>,
    pub limit: usize,
    pub explain: bool,
    pub fail_on: Option<Severity>,
    pub max_unresolved: Option<usize>,

    pub include_accepted: bool,
}

pub fn scan(path: &Path, opts: &LoadOptions, as_json: bool, style: Style) -> Result<bool, String> {
    let l = load(path, opts)?;
    let g = &l.analysis.graph;
    let loc: u64 = g.files.iter().map(|f| f.loc as u64).sum();
    let cycles = tangles(&l.deps).len();
    let external = g
        .unresolved
        .iter()
        .filter(|u| matches!(u.reason, UnresolvedReason::External(_)))
        .count();

    if as_json {
        println!(
            "{}",
            json!({
                "root": l.info.root,
                "name": l.info.name,
                "branch": l.info.branch,
                "head": l.info.head,
                "isGit": l.info.is_git,
                "files": g.files.len(),
                "modules": g.modules.len(),
                "edges": g.edges.len(),
                "loc": loc,
                "unresolved": g.unresolved_failure_count(),
                "external": external,
                "cycles": cycles,
                "findings": l.findings.len(),
                "languages": language_counts(&l),
            })
        );
        return Ok(true);
    }

    println!("{}", style.bold(&l.info.name));
    println!("{}", style.dim(&l.info.root));
    if let Some(branch) = &l.info.branch {
        let head = l.info.head.clone().unwrap_or_default();
        println!("{}", style.dim(&format!("{branch} @ {head}")));
    }
    println!();
    field(style, "files", &g.files.len().to_string());
    field(style, "modules", &g.modules.len().to_string());
    field(style, "dependencies", &g.edges.len().to_string());
    field(style, "lines", &loc.to_string());
    field(style, "cycles", &cycles.to_string());
    field(style, "findings", &l.findings.len().to_string());
    field(
        style,
        "unresolved",
        &format!(
            "{} ({external} external, not a failure)",
            g.unresolved_failure_count()
        ),
    );
    println!();
    for (language, count) in language_counts(&l) {
        field(style, &language, &count.to_string());
    }
    Ok(true)
}

pub fn findings(
    path: &Path,
    opts: &LoadOptions,
    as_json: bool,
    style: Style,
    args: FindingsArgs,
) -> Result<bool, String> {
    let l = load(path, opts)?;
    let root = Path::new(&l.info.root);
    let baseline = baseline::load(root).map_err(|e| e.to_string())?;

    let (considered, accepted_count) = match (&baseline, args.include_accepted) {
        (Some(b), false) => {
            let c = baseline::compare(&l.findings, &l.analysis.graph, b);
            let fresh: Vec<Finding> = c.fresh.into_iter().cloned().collect();
            let n = c.accepted.len();
            (fresh, n)
        }
        _ => (l.findings.clone(), 0),
    };

    let rows: Vec<&Finding> = considered
        .iter()
        .filter(|f| args.kinds.is_empty() || args.kinds.contains(&f.kind))
        .filter(|f| args.severity.is_none_or(|min| f.severity >= min))
        .collect();

    let breached = args
        .fail_on
        .is_some_and(|min| considered.iter().any(|f| f.severity >= min))
        || args
            .max_unresolved
            .is_some_and(|max| l.analysis.graph.unresolved_failure_count() > max);

    if as_json {
        let payload: Vec<serde_json::Value> = rows
            .iter()
            .take(args.limit)
            .map(|f| {
                json!({
                    "kind": f.kind,
                    "severity": f.severity,
                    "score": f.score,
                    "headline": f.headline,
                    "files": f.files.iter().map(|id| l.path_of(*id)).collect::<Vec<_>>(),
                    "evidence": f.evidence,
                    "why": f.why,
                    "howToFix": f.how_to_fix,
                })
            })
            .collect();
        println!(
            "{}",
            json!({
                "total": rows.len(),
                "accepted": accepted_count,
                "unresolved": l.analysis.graph.unresolved_failure_count(),
                "passed": !breached,
                "findings": payload,
            })
        );
        return Ok(!breached);
    }

    if rows.is_empty() {
        println!(
            "{}",
            style.dim(if accepted_count > 0 {
                "Nothing new. Every finding is in the baseline."
            } else {
                "No findings. The map is clean."
            })
        );
        return Ok(!breached);
    }

    for f in rows.iter().take(args.limit) {
        println!(
            "{}  {}  {}",
            style.severity(f.severity, severity_word(f.severity)),
            style.dim(&format!("{:<20}", f.kind.label())),
            f.headline
        );
        if args.explain {
            for e in &f.evidence {
                println!("      {}", style.dim(&format!("{}: {}", e.label, e.value)));
            }
            println!("      {}", wrap(&f.why, 6));

            println!(
                "      {}",
                style.accent(&format!("→ {}", wrap(&f.how_to_fix, 8)))
            );
            println!();
        }
    }
    if rows.len() > args.limit {
        println!(
            "{}",
            style.dim(&format!(
                "… and {} more. Raise --limit to see them.",
                rows.len() - args.limit
            ))
        );
    }
    if accepted_count > 0 {
        println!(
            "{}",
            style.dim(&format!(
                "{accepted_count} more accepted by {} — pass --all to include them.",
                baseline::FILE_NAME
            ))
        );
    }
    if breached {
        eprintln!();
        eprintln!(
            "{}",
            style.severity(Severity::High, "Threshold breached — exiting 1.")
        );
    }
    Ok(!breached)
}

pub fn compare(
    path: &Path,
    as_json: bool,
    style: Style,
    base: &str,
    head: &str,
    fail_on_cycle: bool,
) -> Result<bool, String> {
    let info = zatlas_core::repo::open(path).map_err(|e| e.to_string())?;
    if !info.is_git {
        return Err("comparing refs needs a git repository".to_owned());
    }
    let root = std::path::PathBuf::from(&info.root);
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let out = zatlas_core::git::compare::compare(&root, base, head, &cancel)
        .map_err(|e| e.to_string())?;

    let breached = fail_on_cycle && !out.cycles_introduced.is_empty();

    if as_json {
        println!("{}", serde_json::to_string(&out).unwrap_or_default());
        return Ok(!breached);
    }

    println!(
        "{} {} {}",
        style.bold(&format!("{} ({})", out.base.reference, out.base.commit)),
        style.dim("→"),
        style.bold(&format!("{} ({})", out.head.reference, out.head.commit)),
    );
    println!();
    delta(style, "files", out.base.files as i64, out.head.files as i64);
    delta(style, "lines", out.base.loc as i64, out.head.loc as i64);
    delta(
        style,
        "dependencies",
        out.base.edges as i64,
        out.head.edges as i64,
    );
    delta(
        style,
        "cycles",
        out.base.cycles as i64,
        out.head.cycles as i64,
    );
    delta(
        style,
        "unresolved",
        out.base.unresolved as i64,
        out.head.unresolved as i64,
    );
    println!();

    for cycle in &out.cycles_introduced {
        println!(
            "{} {}",
            style.severity(Severity::High, "cycle +"),
            cycle.join(" → ")
        );
    }
    for cycle in &out.cycles_resolved {
        println!(
            "{} {}",
            style.accent("cycle -"),
            style.dim(&cycle.join(" → "))
        );
    }
    if !out.cycles_introduced.is_empty() || !out.cycles_resolved.is_empty() {
        println!();
    }

    section(style, "added", &out.added);
    section(style, "removed", &out.removed);
    for (path, d) in out.grown.iter().take(10) {
        println!(
            "{} {path} {}",
            style.dim("grew"),
            style.dim(&format!("+{d}"))
        );
    }
    for (path, d) in out.shrunk.iter().take(10) {
        println!(
            "{} {path} {}",
            style.dim("shrank"),
            style.dim(&format!("{d}"))
        );
    }

    if !out.findings_new.is_empty() {
        println!();
        for headline in out.findings_new.iter().take(20) {
            println!("{} {headline}", style.severity(Severity::Medium, "new"));
        }
    }

    if breached {
        eprintln!();
        eprintln!(
            "{}",
            style.severity(
                Severity::High,
                "This branch introduces a cycle — exiting 1."
            )
        );
    }
    Ok(!breached)
}

fn delta(style: Style, label: &str, before: i64, after: i64) {
    let change = after - before;
    let arrow = match change.cmp(&0) {
        std::cmp::Ordering::Greater => style.severity(Severity::Medium, &format!("+{change}")),
        std::cmp::Ordering::Less => style.accent(&format!("{change}")),

        std::cmp::Ordering::Equal => style.dim("—"),
    };
    println!("{}  {after:<8} {arrow}", style.dim(&format!("{label:>14}")));
}

fn section(style: Style, label: &str, paths: &[String]) {
    for path in paths.iter().take(20) {
        println!("{} {path}", style.dim(label));
    }
    if paths.len() > 20 {
        println!(
            "{}",
            style.dim(&format!("  … and {} more", paths.len() - 20))
        );
    }
}

pub fn baseline_status(
    path: &Path,
    opts: &LoadOptions,
    as_json: bool,
    style: Style,
) -> Result<bool, String> {
    let l = load(path, opts)?;
    let root = Path::new(&l.info.root);
    let Some(existing) = baseline::load(root).map_err(|e| e.to_string())? else {
        if as_json {
            println!(
                "{}",
                json!({ "baseline": false, "findings": l.findings.len() })
            );
        } else {
            println!(
                "{}",
                style.dim(&format!(
                    "No {}. {} finding(s) would be accepted by `zatlas baseline accept`.",
                    baseline::FILE_NAME,
                    l.findings.len()
                ))
            );
        }
        return Ok(true);
    };

    let c = baseline::compare(&l.findings, &l.analysis.graph, &existing);
    if as_json {
        println!(
            "{}",
            json!({
                "baseline": true,
                "created": existing.created,
                "new": c.fresh.iter().map(|f| &f.headline).collect::<Vec<_>>(),
                "acceptedCount": c.accepted.len(),
                "fixed": c.fixed.iter().map(|e| &e.headline).collect::<Vec<_>>(),
            })
        );
        return Ok(true);
    }

    println!(
        "{}",
        style.dim(&format!(
            "{} · accepted {}",
            baseline::FILE_NAME,
            existing.created
        ))
    );
    println!();
    for f in &c.fresh {
        println!("{} {}", style.severity(f.severity, "new "), f.headline);
    }

    for e in &c.fixed {
        println!("{} {}", style.accent("gone"), style.dim(&e.headline));
    }
    if c.fresh.is_empty() && c.fixed.is_empty() {
        println!("{}", style.dim("Unchanged."));
    }
    println!();
    println!(
        "{}",
        style.dim(&format!(
            "{} new · {} accepted · {} no longer occur",
            c.fresh.len(),
            c.accepted.len(),
            c.fixed.len()
        ))
    );
    if !c.fixed.is_empty() {
        println!(
            "{}",
            style.dim("Run `zatlas baseline prune` to drop those.")
        );
    }
    Ok(true)
}

pub fn baseline_accept(
    path: &Path,
    opts: &LoadOptions,
    style: Style,
    below: Option<Severity>,
) -> Result<bool, String> {
    let l = load(path, opts)?;
    let root = Path::new(&l.info.root);
    let accepting: Vec<Finding> = l
        .findings
        .iter()
        .filter(|f| below.is_none_or(|max| f.severity < max))
        .cloned()
        .collect();
    let skipped = l.findings.len() - accepting.len();

    let new = baseline::from_findings(&accepting, &l.analysis.graph, &baseline::today());
    baseline::save(root, &new).map_err(|e| e.to_string())?;

    println!(
        "{}",
        style.bold(&format!(
            "Accepted {} finding(s) into {}.",
            new.entries.len(),
            baseline::FILE_NAME
        ))
    );
    if skipped > 0 {
        println!(
            "{}",
            style.dim(&format!(
                "{skipped} left visible, above the severity you asked to waive."
            ))
        );
    }
    println!(
        "{}",
        style.dim("Commit it. Later runs report only what appeared since.")
    );
    Ok(true)
}

pub fn baseline_drop(
    path: &Path,
    opts: &LoadOptions,
    style: Style,
    pattern: &str,
) -> Result<bool, String> {
    let l = load(path, opts)?;
    let root = Path::new(&l.info.root);
    let Some(mut existing) = baseline::load(root).map_err(|e| e.to_string())? else {
        return Err(format!("there is no {} to drop from", baseline::FILE_NAME));
    };

    let needle = pattern.to_lowercase();
    let going: Vec<String> = existing
        .entries
        .iter()
        .filter(|e| e.headline.to_lowercase().contains(&needle))
        .map(|e| e.headline.clone())
        .collect();

    if going.is_empty() {
        println!(
            "{}",
            style.dim(&format!("No accepted finding matches {pattern:?}."))
        );
        return Ok(true);
    }

    existing
        .entries
        .retain(|e| !e.headline.to_lowercase().contains(&needle));
    baseline::save(root, &existing).map_err(|e| e.to_string())?;

    for headline in &going {
        println!("{} {}", style.accent("back"), headline);
    }
    println!(
        "{}",
        style.dim(&format!(
            "{} back in the list · {} still accepted",
            going.len(),
            existing.entries.len()
        ))
    );
    Ok(true)
}

pub fn baseline_prune(path: &Path, opts: &LoadOptions, style: Style) -> Result<bool, String> {
    let l = load(path, opts)?;
    let root = Path::new(&l.info.root);
    let Some(mut existing) = baseline::load(root).map_err(|e| e.to_string())? else {
        return Err(format!("there is no {} to prune", baseline::FILE_NAME));
    };

    let stale: Vec<String> = baseline::compare(&l.findings, &l.analysis.graph, &existing)
        .fixed
        .iter()
        .map(|e| e.id.clone())
        .collect();
    if stale.is_empty() {
        println!(
            "{}",
            style.dim("Nothing to prune; every entry still occurs.")
        );
        return Ok(true);
    }

    existing.entries.retain(|e| !stale.contains(&e.id));
    baseline::save(root, &existing).map_err(|e| e.to_string())?;
    println!(
        "{}",
        style.bold(&format!(
            "Dropped {} entry(s) that no longer occur. {} remain.",
            stale.len(),
            existing.entries.len()
        ))
    );
    Ok(true)
}

pub fn cycles(
    path: &Path,
    opts: &LoadOptions,
    as_json: bool,
    style: Style,
    fail: bool,
) -> Result<bool, String> {
    let l = load(path, opts)?;
    let found = tangles(&l.deps);

    if as_json {
        let payload: Vec<serde_json::Value> = found
            .iter()
            .map(|t| {
                json!({
                    "members": t.members.iter().map(|id| l.path_of(*id)).collect::<Vec<_>>(),
                    "chain": t.example.iter().map(|id| l.path_of(*id)).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", json!({ "cycles": payload, "count": found.len() }));
        return Ok(!(fail && !found.is_empty()));
    }

    if found.is_empty() {
        println!("{}", style.dim("No import cycles."));
        return Ok(true);
    }

    for t in &found {
        println!(
            "{} {}",
            style.severity(Severity::High, "cycle"),
            style.dim(&format!("({} files)", t.members.len()))
        );

        for (i, id) in t.example.iter().enumerate() {
            let arrow = if i == 0 { "  " } else { "→ " };
            println!("  {arrow}{}", l.path_of(*id));
        }
        println!();
    }
    Ok(!fail)
}

pub fn unresolved(
    path: &Path,
    opts: &LoadOptions,
    as_json: bool,
    style: Style,
    all: bool,
) -> Result<bool, String> {
    let l = load(path, opts)?;
    let rows: Vec<_> = l
        .analysis
        .graph
        .unresolved
        .iter()
        .filter(|u| all || is_failure(&u.reason))
        .collect();

    if as_json {
        let payload: Vec<serde_json::Value> = rows
            .iter()
            .map(|u| {
                json!({
                    "file": l.path_of(u.from),
                    "line": u.line,
                    "specifier": u.specifier,
                    "reason": u.reason,
                    "failure": is_failure(&u.reason),
                })
            })
            .collect();
        println!("{}", json!({ "count": rows.len(), "unresolved": payload }));
        return Ok(true);
    }

    if rows.is_empty() {
        println!(
            "{}",
            style.dim(if all {
                "Every import resolved to a file in the scan."
            } else {
                "No failed imports."
            })
        );
        return Ok(true);
    }

    for u in rows {
        println!(
            "{}:{}  {}",
            l.path_of(u.from),
            u.line,
            style.dim(&describe(&u.reason))
        );
        println!("    {}", u.specifier);
    }
    Ok(true)
}

pub fn impact(
    path: &Path,
    opts: &LoadOptions,
    as_json: bool,
    style: Style,
    files: &[String],
) -> Result<bool, String> {
    let l = load(path, opts)?;
    let seeds: Vec<FileId> = files
        .iter()
        .map(|f| l.find_file(f))
        .collect::<Result<_, _>>()?;
    let affected = graph::impact(&l.deps, &seeds);

    if as_json {
        println!(
            "{}",
            json!({
                "seeds": seeds.iter().map(|id| l.path_of(*id)).collect::<Vec<_>>(),
                "count": affected.len(),
                "affected": affected.iter().map(|id| l.path_of(*id)).collect::<Vec<_>>(),
            })
        );
        return Ok(true);
    }

    println!(
        "{}",
        style.bold(&format!(
            "{} file(s) depend on {} transitively",
            affected.len(),
            seeds
                .iter()
                .map(|id| l.path_of(*id))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    );

    let mut paths: Vec<&str> = affected.iter().map(|id| l.path_of(*id)).collect();
    paths.sort_unstable();
    for p in paths {
        println!("  {p}");
    }
    Ok(true)
}

pub fn tour(
    path: &Path,
    opts: &LoadOptions,
    as_json: bool,
    style: Style,
    stops: usize,
) -> Result<bool, String> {
    let l = load(path, opts)?;
    let entrypoints = zatlas_core::findings::prepare(
        &l.analysis.graph,
        &l.deps,
        &l.structural,
        &l.config,
        &l.history,
        &l.coupling,
    )
    .entrypoints;
    let route = tour::build(
        &l.analysis.graph,
        &entrypoints,
        &l.history,
        &TourOptions {
            max_stops: stops,
            ..Default::default()
        },
    );

    if as_json {
        println!("{}", serde_json::to_string(&route).unwrap_or_default());
        return Ok(true);
    }

    if route.is_empty() {
        println!("{}", style.dim("Nothing substantial enough to read first."));
        return Ok(true);
    }
    for (i, stop) in route.iter().enumerate() {
        println!(
            "{} {}",
            style.accent(&format!("{:>2}.", i + 1)),
            style.bold(&stop.path)
        );
        println!(
            "    {}",
            style.dim(&format!("{} LOC · unlocks {}", stop.loc, stop.unlocks))
        );
        println!("    {}", stop.reason);
    }
    Ok(true)
}

pub fn export(
    path: &Path,
    opts: &LoadOptions,
    format: FormatArg,
    scope: ScopeArg,
    out: Option<&Path>,
) -> Result<bool, String> {
    let l = load(path, opts)?;
    let cycle_files = tangles(&l.deps)
        .iter()
        .flat_map(|t| t.members.iter().copied())
        .collect();
    let diagram = DiagramOptions {
        scope: scope.into(),
        ..Default::default()
    };

    let text = match format {
        FormatArg::D2 => to_d2(&l.analysis.graph, &diagram, &cycle_files),
        FormatArg::Mermaid => to_mermaid(&l.analysis.graph, &diagram, &cycle_files),
        FormatArg::Markdown => to_markdown(
            &l.analysis.graph,
            &l.deps,
            &l.findings,
            &l.history,
            &l.info.name,
        ),
    };

    match out {
        Some(file) => {
            std::fs::write(file, &text).map_err(|e| format!("{}: {e}", file.display()))?;
            eprintln!("wrote {}", file.display());
        }
        None => print!("{text}"),
    }
    Ok(true)
}

fn field(style: Style, label: &str, value: &str) {
    println!("{}  {value}", style.dim(&format!("{label:>14}")));
}

fn language_counts(l: &Loaded) -> Vec<(String, usize)> {
    let mut counts: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::new();
    for f in &l.analysis.graph.files {
        *counts.entry(f.language.label()).or_insert(0) += 1;
    }
    let mut out: Vec<(String, usize)> =
        counts.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();

    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

fn is_failure(reason: &UnresolvedReason) -> bool {
    !matches!(
        reason,
        UnresolvedReason::External(_)
            | UnresolvedReason::Asset(_)
            | UnresolvedReason::ExcludedFromScan(_)
    )
}

fn describe(reason: &UnresolvedReason) -> String {
    match reason {
        UnresolvedReason::External(name) => format!("external package: {name}"),
        UnresolvedReason::NoSuchFile(p) => format!("no such file: {p}"),
        UnresolvedReason::UnmatchedAlias(a) => format!("alias matched nothing: {a}"),
        UnresolvedReason::DynamicSpecifier => "computed at runtime".to_owned(),
        UnresolvedReason::ReExportChainTooDeep(p) => format!("re-export chain too deep: {p}"),
        UnresolvedReason::NoResolverForLanguage(l) => format!("no resolver for {}", l.label()),
        UnresolvedReason::ExcludedFromScan(p) => format!("excluded from the scan: {p}"),
        UnresolvedReason::FileNotInModuleTree => {
            "this file is not reachable from any crate root".to_owned()
        }
        UnresolvedReason::Asset(p) => format!("asset: {p}"),
    }
}

fn wrap(text: &str, indent: usize) -> String {
    const WIDTH: usize = 80;
    let limit = WIDTH.saturating_sub(indent).max(20);
    let mut out = String::new();
    let mut line = 0usize;
    for word in text.split_whitespace() {
        let len = word.chars().count();
        if line > 0 && line + 1 + len > limit {
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            line = 0;
        } else if line > 0 {
            out.push(' ');
            line += 1;
        }
        out.push_str(word);
        line += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_and_asset_outcomes_are_not_counted_as_failures() {
        assert!(!is_failure(&UnresolvedReason::External("serde".into())));
        assert!(!is_failure(&UnresolvedReason::Asset("a.css".into())));
        assert!(!is_failure(&UnresolvedReason::ExcludedFromScan("g".into())));
        assert!(is_failure(&UnresolvedReason::NoSuchFile("./x".into())));
        assert!(is_failure(&UnresolvedReason::FileNotInModuleTree));
    }

    #[test]
    fn every_unresolved_reason_has_a_sentence_rather_than_a_debug_dump() {
        for reason in [
            UnresolvedReason::External("serde".into()),
            UnresolvedReason::NoSuchFile("./x".into()),
            UnresolvedReason::UnmatchedAlias("@app/x".into()),
            UnresolvedReason::DynamicSpecifier,
            UnresolvedReason::ReExportChainTooDeep("./b".into()),
            UnresolvedReason::NoResolverForLanguage(zatlas_core::model::Language::Go),
            UnresolvedReason::ExcludedFromScan("gen/x".into()),
            UnresolvedReason::FileNotInModuleTree,
            UnresolvedReason::Asset("a.css".into()),
        ] {
            let text = describe(&reason);
            assert!(!text.is_empty());
            assert!(
                !text.contains('"') || text.contains(':'),
                "reads like a debug dump: {text}"
            );
        }
    }

    #[test]
    fn wrapping_never_exceeds_the_line_budget() {
        let text = "The heuristic resolver and the language server disagree about \
                    where this import goes, so at least one edge on the map is wrong.";
        for line in wrap(text, 6).lines() {
            assert!(line.chars().count() <= 74, "too long: {line:?}");
        }
    }

    #[test]
    fn wrapping_does_not_break_a_word_in_half() {
        let out = wrap("supercalifragilistic and short", 0);
        assert!(out.contains("supercalifragilistic"));
    }

    #[test]
    fn wrapping_leaves_a_short_sentence_on_one_line() {
        assert_eq!(wrap("Short enough.", 6), "Short enough.");
    }
}
