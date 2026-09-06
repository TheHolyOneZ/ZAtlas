#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use zatlas_core::analyse::{analyse, AnalyseOptions};
use zatlas_core::config::ZatlasConfig;
use zatlas_core::lsp::servers::{Discovered, ServerSpec};
use zatlas_core::lsp::{apply_with, LspMode, LspSettings};
use zatlas_core::model::Language;

static FAKE: ServerSpec = ServerSpec {
    id: "fake",
    label: "Fake Server",
    command: "fake",
    args: &[],
    languages: &[Language::TypeScript],
    root_markers: &[],
};

const SERVER: &str = r#"#!/usr/bin/env python3
import json, os, sys

def read():
    length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        line = line.strip()
        if not line:
            break
        name, _, value = line.decode().partition(":")
        if name.strip().lower() == "content-length":
            length = int(value.strip())
    if length is None:
        return None
    return json.loads(sys.stdin.buffer.read(length))

def write(message):
    body = json.dumps(message).encode()
    sys.stdout.buffer.write(b"Content-Length: %d\r\n\r\n" % len(body))
    sys.stdout.buffer.write(body)
    sys.stdout.buffer.flush()

target = "@@TARGET@@"
shape = "@@SHAPE@@"

while True:
    msg = read()
    if msg is None:
        break
    method = msg.get("method")
    if method == "initialize":
        write({"jsonrpc": "2.0", "id": msg["id"],
               "result": {"capabilities": {"definitionProvider": True}}})
        # Unprompted work the client has to answer, or a real server stalls.
        write({"jsonrpc": "2.0", "id": 9001,
               "method": "window/workDoneProgress/create",
               "params": {"token": "t"}})
    elif method == "textDocument/definition":
        if not target:
            result = None
        elif shape == "single":
            result = {"uri": target, "range": {}}
        elif shape == "link":
            result = [{"targetUri": target}]
        else:
            result = [{"uri": target, "range": {}}]
        write({"jsonrpc": "2.0", "id": msg["id"], "result": result})
    elif method == "shutdown":
        write({"jsonrpc": "2.0", "id": msg["id"], "result": None})
    elif method == "exit":
        break
"#;

fn write_server(dir: &Path, target: &str, shape: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(format!("fake-lsp-{shape}-{}", target.len()));
    let body = SERVER
        .replace("@@TARGET@@", target)
        .replace("@@SHAPE@@", shape);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn fixture(dir: &Path) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/app.ts"),
        "import { thing } from './generated/thing';\nexport const x = thing;\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/thing.ts"), "export const thing = 1;\n").unwrap();
}

fn scan(root: &Path) -> zatlas_core::analyse::Analysis {
    let cancel = Arc::new(AtomicBool::new(false));
    let opts = AnalyseOptions {
        use_cache: false,
        ..Default::default()
    };
    analyse(
        root,
        &ZatlasConfig::default(),
        &opts,
        &cancel,
        &|_, _, _| {},
    )
    .unwrap()
}

fn discovered(executable: PathBuf) -> Vec<Discovered> {
    vec![Discovered {
        spec: &FAKE,
        executable,
    }]
}

fn settings(mode: LspMode) -> LspSettings {
    LspSettings {
        mode,

        timeout_ms: 4_000,
        startup_ms: 4_000,
        budget_ms: 20_000,
        servers: Vec::new(),
    }
}

#[test]
fn repair_mode_turns_an_unresolvable_import_into_an_edge() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fixture(&root);
    let target = zatlas_core::lsp::uri::path_to_uri(&root.join("src/thing.ts"));
    let server = write_server(&root, &target, "array");

    let mut analysis = scan(&root);
    assert_eq!(
        analysis.graph.unresolved_failure_count(),
        1,
        "the fixture's whole point is one import the heuristics cannot place"
    );
    assert!(analysis.graph.edges.is_empty());

    let report = apply_with(
        &root,
        &mut analysis.graph,
        &analysis.parsed,
        &settings(LspMode::Repair),
        &discovered(server),
    )
    .unwrap();

    assert_eq!(report.servers, ["Fake Server"]);
    assert_eq!(report.queries, 1);
    assert_eq!(report.repaired, 1, "{report:?}");
    assert_eq!(report.disagreements, 0);

    assert_eq!(analysis.graph.edges.len(), 1);
    assert_eq!(analysis.graph.unresolved_failure_count(), 0);
    assert_eq!(analysis.graph.lsp_repairs.len(), 1);
    assert_eq!(analysis.graph.lsp_repairs[0].server, "Fake Server");
    assert_eq!(analysis.graph.lsp_repairs[0].specifier, "./generated/thing");
}

#[test]
fn a_server_that_answers_nothing_leaves_the_graph_exactly_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fixture(&root);
    let server = write_server(&root, "", "array");

    let mut analysis = scan(&root);
    let before = analysis.graph.unresolved.len();

    let report = apply_with(
        &root,
        &mut analysis.graph,
        &analysis.parsed,
        &settings(LspMode::Repair),
        &discovered(server),
    )
    .unwrap();

    assert_eq!(report.repaired, 0);
    assert!(analysis.graph.edges.is_empty());
    assert_eq!(analysis.graph.unresolved.len(), before);
}

#[test]
fn a_definition_pointing_outside_the_repository_never_becomes_an_edge() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fixture(&root);
    let server = write_server(&root, "file:///usr/lib/node_modules/thing.d.ts", "array");

    let mut analysis = scan(&root);
    let report = apply_with(
        &root,
        &mut analysis.graph,
        &analysis.parsed,
        &settings(LspMode::Repair),
        &discovered(server),
    )
    .unwrap();

    assert_eq!(report.queries, 1);
    assert_eq!(report.repaired, 0);
    assert!(analysis.graph.edges.is_empty());
}

#[test]
fn verify_mode_reports_a_disagreement_without_removing_the_heuristic_edge() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();

    std::fs::write(
        root.join("src/app.ts"),
        "import { a } from './a';\nexport const x = a;\n",
    )
    .unwrap();
    std::fs::write(root.join("src/a.ts"), "export const a = 1;\n").unwrap();
    std::fs::write(root.join("src/b.ts"), "export const b = 2;\n").unwrap();

    let target = zatlas_core::lsp::uri::path_to_uri(&root.join("src/b.ts"));
    let server = write_server(&root, &target, "array");

    let mut analysis = scan(&root);
    assert_eq!(analysis.graph.edges.len(), 1, "app -> a");

    let report = apply_with(
        &root,
        &mut analysis.graph,
        &analysis.parsed,
        &settings(LspMode::Verify),
        &discovered(server),
    )
    .unwrap();

    assert_eq!(report.disagreements, 1, "{report:?}");
    assert_eq!(report.repaired, 0);
    assert_eq!(analysis.graph.lsp_disagreements.len(), 1);

    assert_eq!(analysis.graph.edges.len(), 2);
}

#[test]
fn a_single_location_response_is_understood_as_well_as_an_array() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fixture(&root);
    let target = zatlas_core::lsp::uri::path_to_uri(&root.join("src/thing.ts"));

    for shape in ["single", "link"] {
        let server = write_server(&root, &target, shape);
        let mut analysis = scan(&root);
        let report = apply_with(
            &root,
            &mut analysis.graph,
            &analysis.parsed,
            &settings(LspMode::Repair),
            &discovered(server),
        )
        .unwrap();
        assert_eq!(report.repaired, 1, "shape {shape}: {report:?}");
    }
}

#[test]
fn a_server_that_cannot_start_is_reported_rather_than_failing_the_scan() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fixture(&root);

    let mut analysis = scan(&root);
    let report = apply_with(
        &root,
        &mut analysis.graph,
        &analysis.parsed,
        &settings(LspMode::Repair),
        &discovered(root.join("does-not-exist")),
    )
    .unwrap();

    assert!(report.servers.is_empty());
    assert_eq!(report.failed.len(), 1);

    assert_eq!(analysis.graph.files.len(), 2);
}
