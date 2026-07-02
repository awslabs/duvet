// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Host-side embedder test: instantiate the `duvet` component with wasmtime,
//! stage a known project in through the `run` API, and assert on the returned
//! report. This proves the checks produce the correct, inspectable pass/fail
//! verdict inside the sandbox.

use wasmtime::{
    component::{Component, Linker, ResourceTable},
    Config, Engine, Store,
};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

// Generate host-side bindings from the same WIT the component exports.
wasmtime::component::bindgen!({
    world: "duvet",
    path: "wit",
});

struct Host {
    table: ResourceTable,
    wasi: WasiCtx,
}

impl WasiView for Host {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

fn load() -> (Store<Host>, Duvet) {
    // The xtask builds this before invoking the test; fall back to a helpful
    // message if run directly without building the component first.
    let wasm = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../target/wasm32-wasip2/debug/duvet_wasm.wasm"
    );
    assert!(
        std::path::Path::new(wasm).exists(),
        "component not built — run `cargo build -p duvet-wasm --target wasm32-wasip2` first \
         (or use `cargo xtask wasm`)"
    );

    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config).unwrap();

    let component = Component::from_file(&engine, wasm).unwrap();

    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker).unwrap();

    let host = Host {
        table: ResourceTable::new(),
        wasi: WasiCtxBuilder::new().build(),
    };
    let mut store = Store::new(&engine, host);

    let bindings = Duvet::instantiate(&mut store, &component, &linker).unwrap();
    (store, bindings)
}

fn file(path: &str, contents: &str) -> File {
    File {
        path: path.to_string(),
        contents: contents.as_bytes().to_vec(),
    }
}

/// The `report-markdown` integration fixture: a spec, a source file citing it,
/// and a config wiring them together. It should pass cleanly.
fn markdown_fixture() -> Vec<File> {
    vec![
        file(
            ".duvet/config.toml",
            "'$schema' = \"https://awslabs.github.io/duvet/config/v0.4.0.json\"\n\n\
             [[source]]\npattern = \"src/my-code.rs\"\n\n\
             [[specification]]\nsource = \"my-spec.md\"\n",
        ),
        file(
            "src/my-code.rs",
            "//= my-spec.md#testing\n//# This quote MUST work\n//# * with\n//# * bullets\n",
        ),
        file(
            "my-spec.md",
            "# My spec\n\nhere is a spec\n\n## Testing\n\nThis quote MUST work\n* with\n* bullets\n",
        ),
    ]
}

fn run(files: Vec<File>) -> CheckReport {
    let (mut store, bindings) = load();
    let input = RunInput {
        files,
        root: "/project".to_string(),
        require_citations: true,
        require_tests: true,
    };
    bindings
        .call_run(&mut store, &input)
        .expect("component trap")
        .expect("analysis error")
}

#[test]
fn passing_fixture() {
    let report = run(markdown_fixture());

    let json: serde_json::Value =
        serde_json::from_str(&report.report_json).expect("report-json is valid JSON");
    assert_eq!(json["version"], "2.0", "expected a v2 report");

    // The report must reference the spec we fed in.
    assert!(
        report.report_json.contains("my-spec.md"),
        "report should mention the specification"
    );
}

/// Parity: the v2 JSON the component produces for the `report-markdown` fixture
/// must be *identical* to what the native CLI produces (captured as the
/// committed integration snapshot). Content-hash entity IDs make this a
/// deterministic equality check, proving the pure pipeline behaves the same
/// inside the sandbox.
#[test]
fn json_v2_matches_native() {
    let report = run(markdown_fixture());
    let actual: serde_json::Value =
        serde_json::from_str(&report.report_json).expect("report-json is valid JSON");

    let snapshot = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../integration/snapshots/report-markdown_json_v2.snap"
    );
    let raw = std::fs::read_to_string(snapshot).expect("read committed native snapshot");
    // insta snapshots prefix a `---`-delimited YAML header before the body.
    let body = raw
        .split_once("---\n")
        .and_then(|(_, rest)| rest.split_once("---\n"))
        .map(|(_, body)| body)
        .unwrap_or(&raw);
    let expected: serde_json::Value =
        serde_json::from_str(body).expect("committed snapshot is valid JSON");

    assert_eq!(
        actual, expected,
        "wasm component json_v2 diverged from the native CLI output"
    );
}

#[test]
fn missing_citation_fails() {
    // A spec with a MUST requirement but no citation in the source: the checks
    // should report `ok == false` with an inspectable reason, not trap.
    let files = vec![
        file(
            ".duvet/config.toml",
            "'$schema' = \"https://awslabs.github.io/duvet/config/v0.4.0.json\"\n\n\
             [[source]]\npattern = \"src/my-code.rs\"\n\n\
             [[specification]]\nsource = \"my-spec.md\"\n",
        ),
        file("src/my-code.rs", "// nothing cited here\n"),
        file(
            "my-spec.md",
            "# My spec\n\n## Testing\n\nThis quote MUST work.\n",
        ),
    ];

    let report = run(files);
    assert!(
        !report.ok,
        "expected the check to fail with an uncited MUST requirement"
    );
    assert!(
        !report.violations.is_empty(),
        "a failing check should enumerate its violations"
    );
    assert!(
        report
            .violations
            .iter()
            .any(|v| matches!(v.kind, ViolationKind::MissingCitation)),
        "expected a missing-citation violation, got {:?}",
        report.violations
    );
}

/// The verdict must enumerate *every* violation, not just the first: a spec
/// with multiple uncited requirements should yield multiple violations.
#[test]
fn all_violations_are_reported() {
    let files = vec![
        file(
            ".duvet/config.toml",
            "'$schema' = \"https://awslabs.github.io/duvet/config/v0.4.0.json\"\n\n\
             [[source]]\npattern = \"src/my-code.rs\"\n\n\
             [[specification]]\nsource = \"my-spec.md\"\n",
        ),
        file("src/my-code.rs", "// nothing cited\n"),
        file(
            "my-spec.md",
            // three distinct MUST requirements on separate lines, none cited
            "# Spec\n\n## A\n\nThe system MUST do the first thing.\n\n\
             ## B\n\nThe system MUST do the second thing.\n\n\
             ## C\n\nThe system MUST do the third thing.\n",
        ),
    ];

    let report = run(files);
    assert!(!report.ok);
    let missing: Vec<_> = report
        .violations
        .iter()
        .filter(|v| matches!(v.kind, ViolationKind::MissingCitation))
        .collect();
    assert!(
        missing.len() >= 3,
        "expected all three uncited requirements to be reported, got {}: {:?}",
        missing.len(),
        report.violations
    );

    // Every violation should carry a message and a target.
    for v in &report.violations {
        assert!(!v.message.is_empty(), "violation missing message: {v:?}");
        assert_eq!(v.target, "my-spec.md");
    }
}
