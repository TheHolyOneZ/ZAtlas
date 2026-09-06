use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use zatlas_core::analyse::{analyse, AnalyseOptions};
use zatlas_core::config::ZatlasConfig;
use zatlas_core::findings::{detect_all, Finding, FindingKind, FindingsOptions, Severity};
use zatlas_core::git::history::FileHistory;
use zatlas_core::git::CoupledPair;
use zatlas_core::walk::WalkOptions;

fn touch(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn scan(root: &Path, config: &ZatlasConfig) -> zatlas_core::analyse::Analysis {
    let opts = AnalyseOptions {
        walk: WalkOptions {
            threads: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    analyse(
        root,
        config,
        &opts,
        &Arc::new(AtomicBool::new(false)),
        &|_, _, _| {},
    )
    .unwrap()
}

fn findings_for(
    root: &Path,
    config: &ZatlasConfig,
    history: HashMap<String, FileHistory>,
    coupling: Vec<CoupledPair>,
) -> Vec<Finding> {
    let a = scan(root, config);
    detect_all(
        &a.graph,
        config,
        &history,
        &coupling,
        &FindingsOptions::default(),
    )
}

fn of_kind(f: &[Finding], kind: FindingKind) -> Vec<&Finding> {
    f.iter().filter(|x| x.kind == kind).collect()
}

fn hist(commits: u32, authors: u32, top: &str) -> FileHistory {
    FileHistory {
        commits,
        last_touched: 1_700_000_000,
        first_touched: 1_600_000_000,
        authors,
        top_author_share: 1.0 / authors as f32,
        top_author: top.into(),
    }
}

#[test]
fn every_finding_explains_itself_and_says_how_to_fix_it() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(
        p,
        "src/a.ts",
        "import { b } from './b';\nexport const a = b;",
    );
    touch(
        p,
        "src/b.ts",
        "import { a } from './a';\nexport const b = a;",
    );
    touch(p, "src/dead.ts", "export const dead = 1;");

    let found = findings_for(p, &ZatlasConfig::default(), HashMap::new(), Vec::new());
    assert!(!found.is_empty(), "the fixture should produce findings");
    for f in &found {
        assert!(!f.why.trim().is_empty(), "{:?} has no why", f.kind);
        assert!(!f.how_to_fix.trim().is_empty(), "{:?} has no fix", f.kind);
        assert!(
            !f.headline.trim().is_empty(),
            "{:?} has no headline",
            f.kind
        );
        assert!(!f.evidence.is_empty(), "{:?} has no evidence", f.kind);
        assert!(!f.files.is_empty(), "{:?} names no files", f.kind);
        assert!(
            (0.0..=1.0).contains(&f.score),
            "{:?} score out of range",
            f.kind
        );
    }
}

#[test]
fn a_healthy_repository_produces_no_alarming_findings() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(
        p,
        "src/index.ts",
        "import { greet } from './greet';\nexport const run = () => greet('x');",
    );
    touch(
        p,
        "src/greet.ts",
        "import { fmt } from './fmt';\nexport const greet = (n: string) => fmt(n);",
    );
    touch(
        p,
        "src/fmt.ts",
        "export const fmt = (s: string) => s.trim();",
    );

    let found = findings_for(p, &ZatlasConfig::default(), HashMap::new(), Vec::new());
    let serious: Vec<&Finding> = found
        .iter()
        .filter(|f| f.severity >= Severity::Medium)
        .collect();
    assert!(serious.is_empty(), "clean repo produced: {serious:#?}");
}

#[test]
fn a_mutual_dependency_is_reported_as_a_cycle_with_its_chain() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(
        p,
        "src/a.ts",
        "import { b } from './b';\nexport const a = b;",
    );
    touch(
        p,
        "src/b.ts",
        "import { a } from './a';\nexport const b = a;",
    );

    let found = findings_for(p, &ZatlasConfig::default(), HashMap::new(), Vec::new());
    let cycles = of_kind(&found, FindingKind::Cycle);
    assert_eq!(cycles.len(), 1);
    assert!(cycles[0].headline.contains("→"), "{}", cycles[0].headline);
    assert_eq!(cycles[0].files.len(), 2);
}

#[test]
fn a_larger_tangle_is_more_severe_than_a_two_file_cycle() {
    let two = {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        touch(
            p,
            "src/a.ts",
            "import { b } from './b';\nexport const a = b;",
        );
        touch(
            p,
            "src/b.ts",
            "import { a } from './a';\nexport const b = a;",
        );
        findings_for(p, &ZatlasConfig::default(), HashMap::new(), Vec::new())
            .into_iter()
            .find(|f| f.kind == FindingKind::Cycle)
            .unwrap()
    };
    let many = {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        for i in 0..8 {
            touch(
                p,
                &format!("src/f{i}.ts"),
                &format!(
                    "import {{ v{} }} from './f{}';\nexport const v{i} = v{};",
                    (i + 1) % 8,
                    (i + 1) % 8,
                    (i + 1) % 8
                ),
            );
        }
        findings_for(p, &ZatlasConfig::default(), HashMap::new(), Vec::new())
            .into_iter()
            .find(|f| f.kind == FindingKind::Cycle)
            .unwrap()
    };
    assert!(
        many.severity > two.severity,
        "an eight-file tangle is a redesign; a two-file cycle is a quick fix"
    );
}

#[test]
fn an_unimported_file_is_an_orphan_but_an_entry_point_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(
        p,
        "src/main.ts",
        "import { used } from './used';\nexport const go = used;",
    );
    touch(p, "src/used.ts", "export const used = 1;");
    touch(p, "src/forgotten.ts", "export const forgotten = 1;");

    let config = ZatlasConfig {
        entrypoints: vec!["src/main.ts".into()],
        ..Default::default()
    };
    let found = findings_for(p, &config, HashMap::new(), Vec::new());
    let orphans = of_kind(&found, FindingKind::Orphan);
    assert_eq!(orphans.len(), 1, "got {orphans:#?}");
    assert!(orphans[0].headline.contains("forgotten.ts"));
}

#[test]
fn test_files_are_not_reported_as_orphans() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(p, "src/main.ts", "export const go = 1;");
    touch(p, "src/thing.test.ts", "export const t = 1;");

    let found = findings_for(p, &ZatlasConfig::default(), HashMap::new(), Vec::new());
    assert!(
        of_kind(&found, FindingKind::Orphan).is_empty(),
        "{found:#?}"
    );
}

#[test]
fn a_layering_violation_is_reported_against_the_declared_rules() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(
        p,
        "src/ui/Chart.tsx",
        "import { q } from '../data/pg';\nexport const C = q;",
    );
    touch(p, "src/domain/rules.ts", "export const r = 1;");
    touch(p, "src/data/pg.ts", "export const q = 1;");

    let config = ZatlasConfig::from_toml(
        r#"
rules = ["ui -> domain", "domain -> data"]

[[layers]]
name = "ui"
paths = ["src/ui/**"]

[[layers]]
name = "domain"
paths = ["src/domain/**"]

[[layers]]
name = "data"
paths = ["src/data/**"]
"#,
    )
    .unwrap();

    let found = findings_for(p, &config, HashMap::new(), Vec::new());
    let violations = of_kind(&found, FindingKind::LayeringViolation);
    assert_eq!(violations.len(), 1, "got {violations:#?}");
    assert!(
        violations[0].headline.contains("ui → data"),
        "{}",
        violations[0].headline
    );
    assert!(violations[0].how_to_fix.contains("zatlas.toml"));
}

#[test]
fn an_allowed_dependency_direction_is_not_a_violation() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(
        p,
        "src/ui/View.tsx",
        "import { r } from '../domain/rules';\nexport const V = r;",
    );
    touch(p, "src/domain/rules.ts", "export const r = 1;");

    let config = ZatlasConfig::from_toml(
        r#"
rules = ["ui -> domain"]

[[layers]]
name = "ui"
paths = ["src/ui/**"]

[[layers]]
name = "domain"
paths = ["src/domain/**"]
"#,
    )
    .unwrap();

    let found = findings_for(p, &config, HashMap::new(), Vec::new());
    assert!(
        of_kind(&found, FindingKind::LayeringViolation).is_empty(),
        "{found:#?}"
    );
}

#[test]
fn without_layer_rules_no_layering_findings_are_invented() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(
        p,
        "src/ui/a.ts",
        "import { q } from '../data/pg';\nexport const A = q;",
    );
    touch(p, "src/data/pg.ts", "export const q = 1;");

    let found = findings_for(p, &ZatlasConfig::default(), HashMap::new(), Vec::new());
    assert!(of_kind(&found, FindingKind::LayeringViolation).is_empty());
}

#[test]
fn a_big_widely_used_churning_file_is_a_god_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let big: String = (0..600)
        .map(|i| format!("export const v{i} = {i};\n"))
        .collect();
    touch(p, "src/god.ts", &big);
    for i in 0..12 {
        touch(
            p,
            &format!("src/c{i}.ts"),
            &format!("import {{ v{i} }} from './god';\nexport const u{i} = v{i};"),
        );
    }

    let mut history = HashMap::new();
    history.insert("src/god.ts".to_string(), hist(90, 4, "Ada"));
    for i in 0..12 {
        history.insert(format!("src/c{i}.ts"), hist(1, 1, "Ada"));
    }

    let found = findings_for(p, &ZatlasConfig::default(), history, Vec::new());
    let gods = of_kind(&found, FindingKind::GodFile);
    assert_eq!(gods.len(), 1, "got {gods:#?}");
    assert!(gods[0].headline.contains("god.ts"));
    assert!(gods[0].severity >= Severity::Medium);
}

#[test]
fn a_large_file_nobody_imports_is_not_a_god_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    let big: String = (0..600)
        .map(|i| format!("export const v{i} = {i};\n"))
        .collect();
    touch(p, "src/big.ts", &big);
    for i in 0..8 {
        touch(p, &format!("src/small{i}.ts"), "export const s = 1;");
    }

    let mut history = HashMap::new();
    history.insert("src/big.ts".to_string(), hist(90, 1, "Ada"));
    let found = findings_for(p, &ZatlasConfig::default(), history, Vec::new());
    assert!(
        of_kind(&found, FindingKind::GodFile).is_empty(),
        "{found:#?}"
    );
}

#[test]
fn files_that_always_change_together_across_modules_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(p, "src/api/encode.ts", "export const enc = 1;");
    touch(p, "src/client/decode.ts", "export const dec = 1;");

    let coupling = vec![CoupledPair {
        a: "src/api/encode.ts".into(),
        b: "src/client/decode.ts".into(),
        together: 14,
        confidence: 0.93,
    }];

    let found = findings_for(p, &ZatlasConfig::default(), HashMap::new(), coupling);
    let coupled = of_kind(&found, FindingKind::DistantCoupling);
    assert_eq!(coupled.len(), 1, "got {coupled:#?}");
    assert!(
        coupled[0].headline.contains("93%"),
        "{}",
        coupled[0].headline
    );
}

#[test]
fn coupling_between_files_that_import_each_other_is_not_a_finding() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(
        p,
        "src/api/a.ts",
        "import { d } from '../client/b';\nexport const enc = d;",
    );
    touch(p, "src/client/b.ts", "export const d = 1;");

    let coupling = vec![CoupledPair {
        a: "src/api/a.ts".into(),
        b: "src/client/b.ts".into(),
        together: 14,
        confidence: 0.95,
    }];

    let found = findings_for(p, &ZatlasConfig::default(), HashMap::new(), coupling);
    assert!(
        of_kind(&found, FindingKind::DistantCoupling).is_empty(),
        "{found:#?}"
    );
}

#[test]
fn coupling_within_one_module_is_ordinary_cohesion() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(p, "src/api/a.ts", "export const a = 1;");
    touch(p, "src/api/b.ts", "export const b = 1;");

    let coupling = vec![CoupledPair {
        a: "src/api/a.ts".into(),
        b: "src/api/b.ts".into(),
        together: 20,
        confidence: 1.0,
    }];
    let found = findings_for(p, &ZatlasConfig::default(), HashMap::new(), coupling);
    assert!(of_kind(&found, FindingKind::DistantCoupling).is_empty());
}

#[test]
fn a_single_author_project_reports_no_bus_factor() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(p, "src/core.ts", "export const c = 1;");
    for i in 0..8 {
        touch(
            p,
            &format!("src/u{i}.ts"),
            "import { c } from './core';\nexport const x = c;",
        );
    }
    let mut history = HashMap::new();
    history.insert("src/core.ts".to_string(), hist(30, 1, "Ada"));
    for i in 0..8 {
        history.insert(format!("src/u{i}.ts"), hist(3, 1, "Ada"));
    }

    let found = findings_for(p, &ZatlasConfig::default(), history, Vec::new());
    assert!(
        of_kind(&found, FindingKind::BusFactor).is_empty(),
        "{found:#?}"
    );
}

#[test]
fn a_solely_owned_file_in_a_shared_project_is_a_bus_factor_finding() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(p, "src/core.ts", "export const c = 1;");
    for i in 0..8 {
        touch(
            p,
            &format!("src/u{i}.ts"),
            "import { c } from './core';\nexport const x = c;",
        );
    }
    let mut history = HashMap::new();
    history.insert("src/core.ts".to_string(), hist(30, 1, "Ada"));
    for i in 0..8 {
        history.insert(format!("src/u{i}.ts"), hist(3, 1, "Grace"));
    }

    let found = findings_for(p, &ZatlasConfig::default(), history, Vec::new());
    let bus = of_kind(&found, FindingKind::BusFactor);
    assert_eq!(bus.len(), 1, "got {bus:#?}");
    assert!(bus[0].headline.contains("Ada"));
}

#[test]
fn findings_are_ranked_most_severe_first() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(
        p,
        "src/ui/a.tsx",
        "import { q } from '../data/pg';\nexport const A = q;",
    );
    touch(p, "src/data/pg.ts", "export const q = 1;");
    touch(p, "src/orphan.ts", "export const o = 1;");

    let config = ZatlasConfig::from_toml(
        r#"
rules = ["ui -> domain"]

[[layers]]
name = "ui"
paths = ["src/ui/**"]

[[layers]]
name = "data"
paths = ["src/data/**"]

[[layers]]
name = "domain"
paths = ["src/domain/**"]
"#,
    )
    .unwrap();

    let found = findings_for(p, &config, HashMap::new(), Vec::new());
    assert!(found.len() >= 2);
    for pair in found.windows(2) {
        assert!(
            pair[0].severity >= pair[1].severity,
            "not sorted: {:?} before {:?}",
            pair[0].severity,
            pair[1].severity
        );
    }
}

#[test]
fn results_are_identical_across_runs() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    for i in 0..10 {
        touch(
            p,
            &format!("src/f{i}.ts"),
            &format!("export const v{i} = {i};"),
        );
    }
    let a = findings_for(p, &ZatlasConfig::default(), HashMap::new(), Vec::new());
    let b = findings_for(p, &ZatlasConfig::default(), HashMap::new(), Vec::new());
    assert_eq!(
        a, b,
        "a findings list that reshuffles between scans is unusable"
    );
}

#[test]
fn files_that_are_never_imported_by_design_are_not_orphans() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(
        p,
        "src/vite-env.d.ts",
        "/// <reference types=\"vite/client\" />",
    );
    touch(p, "vite.config.ts", "export default {};");
    touch(p, "strip-comments.mjs", "export const run = () => {};");
    touch(p, "nested/tool.mjs", "export const t = () => {};");
    touch(p, "apps/web/src/main.tsx", "export const boot = 1;");
    touch(p, "src/genuinely-dead.ts", "export const d = 1;");

    let found = findings_for(p, &ZatlasConfig::default(), HashMap::new(), Vec::new());
    let orphans = of_kind(&found, FindingKind::Orphan);
    let names: Vec<&str> = orphans.iter().map(|f| f.headline.as_str()).collect();
    assert_eq!(
        orphans.len(),
        1,
        "expected only the real orphan, got {names:#?}"
    );
    assert!(orphans[0].headline.contains("genuinely-dead.ts"));
}

#[test]
fn a_monorepo_package_entry_point_is_not_dead_code() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(
        p,
        "apps/desktop/src/main.tsx",
        "import { App } from './App';\nexport const b = App;",
    );
    touch(p, "apps/desktop/src/App.tsx", "export const App = 1;");

    let found = findings_for(p, &ZatlasConfig::default(), HashMap::new(), Vec::new());
    assert!(
        of_kind(&found, FindingKind::Orphan).is_empty(),
        "{found:#?}"
    );
}

#[test]
fn an_import_whose_case_does_not_match_the_file_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(p, "src/userService.ts", "export const svc = 1;");
    touch(
        p,
        "src/app.ts",
        "import { svc } from './UserService';\nexport const a = svc;",
    );

    let a = scan(p, &ZatlasConfig::default());
    assert_eq!(a.graph.edges.len(), 1, "the edge must still resolve");
    assert_eq!(a.graph.unresolved_failure_count(), 0, "it is not a failure");

    let found = detect_all(
        &a.graph,
        &ZatlasConfig::default(),
        &HashMap::new(),
        &[],
        &FindingsOptions::default(),
    );
    let cased = of_kind(&found, FindingKind::CaseMismatch);
    assert_eq!(cased.len(), 1, "got {found:#?}");
    assert!(
        cased[0].headline.contains("UserService"),
        "{}",
        cased[0].headline
    );
    assert!(
        cased[0].headline.contains("userService.ts"),
        "{}",
        cased[0].headline
    );
    assert!(cased[0].how_to_fix.contains("git mv"));
}

#[test]
fn an_import_whose_case_matches_exactly_is_not_reported() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    touch(p, "src/userService.ts", "export const svc = 1;");
    touch(
        p,
        "src/app.ts",
        "import { svc } from './userService';\nexport const a = svc;",
    );

    let a = scan(p, &ZatlasConfig::default());
    assert!(
        a.graph.case_mismatches.is_empty(),
        "{:?}",
        a.graph.case_mismatches
    );
}
