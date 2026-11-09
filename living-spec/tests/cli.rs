mod common;
use living_spec::{
    agents::test_index::TestIndex,
    boundary::{
        coverage::{CaseLink, CaseStatus},
        gen::Example,
        model::Bound,
    },
    cli,
    core::{
        annotations::{Annotations, Audit, AuditVerdict, Link},
        history::History,
        spec::{revision, Spec},
        test_identity::{TestOutcome, TestStatus},
    },
    web::{load_status, render_spec},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Output},
};

const SEEDED: &str = "tests/fixtures/cli/web/seeded/living-spec";
const REGISTRY: &str = "tests/fixtures/cli/web/registry/portfolio-truth-mini.json";

fn seeded_html() -> String {
    let dir = Path::new(SEEDED);
    let spec = Spec::parse(&fs::read_to_string(dir.join("spec.md")).unwrap()).unwrap();
    let annotations = Annotations::load(dir.join("annotations.json")).unwrap();
    let status = load_status(&dir.join("status.json")).unwrap();
    let history = History::new(dir.join("history.jsonl")).read().unwrap();
    render_spec(&spec, &annotations, Some(&status), &history)
}

fn assert_no_urls(html: &str) {
    let lower = html.to_ascii_lowercase();
    assert!(!lower.contains("http://"), "page references http");
    assert!(!lower.contains("https://"), "page references https");
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    requirement: String,
    text: String,
    category: String,
    parse_result: String,
    annotation: Option<Vec<Bound>>,
    rejection_form: Option<String>,
    expected_bounds: Vec<Bound>,
    expected_cases: Vec<Vec<Example>>,
    links: Vec<CaseLink>,
    outcomes: Vec<TestOutcome>,
    expected_error: Option<String>,
    expected_statuses: Vec<Vec<CaseStatus>>,
}
fn fixture(n: usize) -> Fixture {
    let fixture: Fixture = serde_json::from_str(
        &fs::read_to_string(format!(
            "tests/fixtures/cli/boundary/requirements/{n:02}.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let _ = (
        &fixture.category,
        &fixture.parse_result,
        &fixture.annotation,
        &fixture.rejection_form,
        &fixture.expected_cases,
        &fixture.expected_error,
        &fixture.expected_statuses,
    );
    fixture
}

// Minimal in-crate draft-2020-12 validator for exactly the keywords used by
// these two schemas. Unknown keywords fail rather than being ignored.
fn validate(schema: &Value, value: &Value, root: &Value) -> bool {
    let supported = [
        "$schema",
        "$id",
        "$defs",
        "$ref",
        "type",
        "oneOf",
        "const",
        "enum",
        "required",
        "properties",
        "additionalProperties",
        "items",
        "minItems",
        "uniqueItems",
        "minLength",
        "pattern",
        "propertyNames",
    ];
    assert!(
        schema
            .as_object()
            .unwrap()
            .keys()
            .all(|k| supported.contains(&k.as_str())),
        "unsupported keyword {schema}"
    );
    if let Some(r) = schema.get("$ref") {
        return validate(
            root.pointer(r.as_str().unwrap().strip_prefix('#').unwrap())
                .unwrap(),
            value,
            root,
        );
    }
    if let Some(options) = schema.get("oneOf") {
        if options
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| validate(s, value, root))
            .count()
            != 1
        {
            return false;
        }
    }
    if let Some(c) = schema.get("const") {
        if c != value {
            return false;
        }
    }
    if let Some(e) = schema.get("enum") {
        if !e.as_array().unwrap().contains(value) {
            return false;
        }
    }
    if let Some(t) = schema.get("type") {
        let types = t.as_array().cloned().unwrap_or_else(|| vec![t.clone()]);
        if !types.iter().any(|t| match t.as_str().unwrap() {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => panic!("unsupported type"),
        }) {
            return false;
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required") {
            if required
                .as_array()
                .unwrap()
                .iter()
                .any(|k| !object.contains_key(k.as_str().unwrap()))
            {
                return false;
            }
        }
        for (k, v) in object {
            if let Some(names) = schema.get("propertyNames") {
                if !validate(names, &json!(k), root) {
                    return false;
                }
            }
            if let Some(s) = schema.get("properties").and_then(|p| p.get(k)) {
                if !validate(s, v, root) {
                    return false;
                }
            } else if let Some(extra) = schema.get("additionalProperties") {
                if extra == &json!(false) || (extra.is_object() && !validate(extra, v, root)) {
                    return false;
                }
            }
        }
    }
    if let Some(array) = value.as_array() {
        if schema
            .get("minItems")
            .is_some_and(|n| array.len() < n.as_u64().unwrap() as usize)
        {
            return false;
        }
        if schema.get("uniqueItems") == Some(&json!(true))
            && array
                .iter()
                .enumerate()
                .any(|(i, v)| array[..i].contains(v))
        {
            return false;
        }
        if let Some(s) = schema.get("items") {
            if !array.iter().all(|v| validate(s, v, root)) {
                return false;
            }
        }
    }
    if let Some(s) = value.as_str() {
        if schema
            .get("minLength")
            .is_some_and(|n| s.chars().count() < n.as_u64().unwrap() as usize)
        {
            return false;
        }
        if let Some(pattern) = schema.get("pattern") {
            if !regex::Regex::new(pattern.as_str().unwrap())
                .unwrap()
                .is_match(s)
            {
                return false;
            }
        }
    }
    true
}
fn schema() -> Value {
    serde_json::from_str(include_str!(
        "../../contracts/living-spec-boundary-coverage-v1.schema.json"
    ))
    .unwrap()
}

fn agent_cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .args(args)
        .output()
        .unwrap()
}

#[derive(Deserialize)]
struct DepsCliFixture {
    spec: String,
    annotations: Annotations,
}

fn deps_fixture(name: &str) -> DepsCliFixture {
    serde_json::from_slice(
        &fs::read(format!("tests/fixtures/cli/deps/graphs/{name}.json")).unwrap(),
    )
    .unwrap()
}

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
        .expect("temporary outputs stay inside the worktree")
}

fn link(entry: &living_spec::agents::test_index::TestEntry) -> Link {
    Link {
        extra: BTreeMap::new(),
        revision: String::new(),
        rename_hint: Some(entry.rename_hint.clone()),
        test_id: entry.id.clone(),
        fingerprint: entry.fingerprint.clone(),
        explanation: "fixture".into(),
        proposed_by: "linker".into(),
        audit: Audit {
            extra: BTreeMap::new(),
            revision: String::new(),
            verdict: AuditVerdict::Challenged,
            notes: vec![],
        },
    }
}

fn boundary_cli(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .arg("boundary")
        .arg(args[0])
        .arg(dir)
        .args(&args[1..])
        .output()
        .unwrap()
}

#[test]
fn usage_errors_have_a_distinct_exit_code() {
    for args in [
        vec![],
        vec!["rescore"],
        vec!["deps", "unknown", "missing"],
        vec!["boundary", "unknown", "missing"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_living-spec"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(64));
    }
    // Library dispatch returns the same typed error without terminating its caller.
    assert!(matches!(
        living_spec::cli::run(&[]),
        Err(living_spec::cli::Exit::Usage(_))
    ));
}

fn rollup_fixture_requirements(fern: &Path) -> serde_json::Map<String, Value> {
    let spec = living_spec::core::spec::Spec::parse(
        &fs::read_to_string(fern.join("living-spec/spec.md")).unwrap(),
    )
    .unwrap();
    let requirements = spec
        .lines
        .iter()
        .map(|line| {
            (
                line.id.clone(),
                serde_json::json!({"links": [{
                    "test_id": "fixture", "fingerprint": "sha256:fixture",
                    "revision": line.revision, "explanation": "Authored fixture evidence.",
                    "proposed_by": "human", "audit": {"verdict": "accepted", "notes": [],
                        "revision": line.revision}
                }]}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    requirements
}

#[test]
fn include_adds_a_root_and_registry_must_be_explicit() {
    let expected_true = true;
    let expected_false = false;
    let tmp = tempfile::tempdir().unwrap();
    let fern = tmp.path().join("fern-notes");
    fs::create_dir_all(fern.join("living-spec")).unwrap();
    fs::write(
        fern.join("living-spec/spec.md"),
        "- [FN-001] Keeps a note.\n- [FN-002] Preserves its title.\n",
    )
    .unwrap();
    fs::write(
        fern.join("living-spec").join("status.json"),
        r#"{"lines":[{"id":"FN-001","badge":"proven","tests":[{"test_id":"fixture", "status":"passed"}]},{"id":"FN-002","badge":"proven","tests":[{"test_id":"fixture", "status":"passed"}]}]}"#,
    )
    .unwrap();
    let requirements = rollup_fixture_requirements(&fern);
    fs::write(
        fern.join("living-spec/annotations.json"),
        serde_json::json!({
            "schema": "living-spec.annotations/v1", "requirements": requirements
        })
        .to_string(),
    )
    .unwrap();
    let out = tmp.path().join("rollup.html");
    let json = tmp.path().join("rollup.json");
    let fern_arg = fern.display().to_string();
    cli::run_with_registry_env(
        &[
            "rollup".into(),
            "--registry".into(),
            REGISTRY.into(),
            "--include".into(),
            fern_arg.clone(),
            "--include".into(),
            fern_arg,
            "--out".into(),
            out.display().to_string(),
            "--json".into(),
            json.display().to_string(),
        ],
        Some("does-not-exist.json"),
    )
    .unwrap();
    let document = read_json_file(&json);
    // These legacy fixture reports predate digest bindings. Includes still
    // deduplicate correctly, but cannot contribute earned badges until scored.
    assert_eq!(document["total_proven"].as_u64(), Some(0));
    assert_eq!(document["total_lines"].as_u64(), Some(10));
    assert_eq!(document["total_proven_pct"].as_f64(), Some(0.0));
    assert_eq!(document["rows"][2]["state"], "stale");
    let projects = project_names(&document);
    assert_eq!(
        projects,
        [
            "Amber Ledger",
            "Birch Timer",
            "fern-notes",
            "Elm Draft",
            "Cedar Sketch"
        ]
    );
    assert_eq!(
        fs::read_to_string(&out).unwrap().contains("Dune Relic"),
        expected_false
    );

    let env_out = tmp.path().join("from-env.html");
    cli::run_with_registry_env(
        &[
            "rollup".into(),
            "--out".into(),
            env_out.display().to_string(),
        ],
        Some(REGISTRY),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(env_out)
            .unwrap()
            .contains("Amber Ledger"),
        expected_true
    );

    let missing =
        cli::run_with_registry_env(&["rollup".into(), "--out".into(), "out.html".into()], None)
            .unwrap_err()
            .to_string();
    assert_eq!(missing.contains("--registry"), expected_true);
    assert_eq!(missing.contains("LIVING_SPEC_REGISTRY"), expected_true);
    assert_eq!(missing.contains("/Users/"), expected_false);
    assert_eq!(missing.contains("/home/"), expected_false);
}

fn project_names(document: &Value) -> Vec<&str> {
    document["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["project"].as_str().unwrap())
        .collect()
}

fn read_json_file(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn render_without_status_uses_gray_unscored_badges() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("living-spec");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("spec.md"),
        "# Draft\n\n- [R-001] Nothing scored yet.\n",
    )
    .unwrap();
    fs::write(
        dir.join("annotations.json"),
        r#"{"schema":"living-spec.annotations/v1","requirements":{}}"#,
    )
    .unwrap();
    let out = tmp.path().join("view.html");
    cli::run(&[
        "render".into(),
        dir.display().to_string(),
        "--out".into(),
        out.display().to_string(),
    ])
    .unwrap();
    let html = fs::read_to_string(out).unwrap();
    assert!(html.contains("not yet scored"));
    assert!(html.contains("data-scored=\"false\""));
    assert!(html.contains("data-badge=\"unscored\""));
    assert!(html.contains(">unscored<"));
    assert!(html.contains("badge-unscored"));
    assert!(!html.contains("promises nobody has proven yet"));
    assert_no_urls(&html);
}

#[test]
fn render_command_matches_the_library_page() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("spec");
    fs::create_dir_all(&dir).unwrap();
    for name in [
        "spec.md",
        "annotations.json",
        "status.json",
        "history.jsonl",
    ] {
        fs::copy(Path::new(SEEDED).join(name), dir.join(name)).unwrap();
    }
    let statuses = load_status(&dir.join("status.json")).unwrap();
    let outcomes: Vec<_> = statuses
        .iter()
        .flat_map(|line| line.tests.iter())
        .map(|test| TestOutcome {
            test_id: test.test_id.clone(),
            status: test.status,
            duration_ms: test.duration_ms,
            failure_message: None,
        })
        .collect();
    let results = dir.join("results.xml");
    fs::write(&results, common::junit(&outcomes)).unwrap();
    let snapshot = living_spec::core::freshness::ResultsSnapshot::read(
        &dir,
        results.to_str().unwrap(),
        &outcomes,
    )
    .unwrap();
    let spec_bytes = fs::read(dir.join("spec.md")).unwrap();
    let annotations =
        serde_json::from_slice(&fs::read(dir.join("annotations.json")).unwrap()).unwrap();
    let inputs = living_spec::core::freshness::ScoreInputs::capture(
        &spec_bytes,
        &annotations,
        &snapshot,
        &outcomes,
        &statuses,
        &[],
        &[],
    )
    .unwrap();
    let mut document: Value = read_json_file(&dir.join("status.json"));
    document["inputs"] = serde_json::to_value(inputs).unwrap();
    fs::write(dir.join("status.json"), document.to_string()).unwrap();
    let out = tmp.path().join("view.html");
    cli::run(&[
        "render".into(),
        dir.display().to_string(),
        "--out".into(),
        out.display().to_string(),
    ])
    .unwrap();
    assert_eq!(fs::read_to_string(out).unwrap(), seeded_html());
}

#[test]
fn check_command_still_reports_orphans() {
    let expected_true = true;

    cli::run(&["check".into(), "tests/fixtures/cli/core/basic".into()]).unwrap();
    let error = cli::run(&["check".into(), "tests/fixtures/cli/core/orphan".into()])
        .unwrap_err()
        .to_string();
    assert_eq!(error.contains("orphan"), expected_true);
}

#[test]
fn check_rejects_unsupported_annotation_schema() {
    let dir = tempfile::tempdir_in("target").unwrap();
    fs::write(dir.path().join("spec.md"), "- [R-001] A requirement.\n").unwrap();
    fs::write(
        dir.path().join("annotations.json"),
        r#"{"schema":"living-spec.annotations/v2","requirements":{}}"#,
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .arg("check")
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("invalid input: unsupported annotations schema"));
}

fn quantifier_annotation(kind: &str, domain: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "kind": kind,
        "domain": domain,
        "subject": "abs_i8",
        "predicate": {
            "version": 1,
            "expression": {
                "op": "ge",
                "left": { "kind": "field", "source": "output" },
                "right": { "kind": "literal", "value": { "type": "int", "value": 0 } }
            }
        }
    })
}

fn check_quantifier_load(quantifier: serde_json::Value, expected_path: &str, code: i32) {
    let dir = tempfile::tempdir_in("target").unwrap();
    fs::write(dir.path().join("spec.md"), "- [R-001] A requirement.\n").unwrap();
    let annotations = serde_json::json!({
        "schema": "living-spec.annotations/v1",
        "requirements": { "R-001": { "quantifier": quantifier } }
    });
    fs::write(
        dir.path().join("annotations.json"),
        serde_json::to_vec(&annotations).unwrap(),
    )
    .unwrap();
    let load =
        living_spec::core::annotations::Annotations::load(dir.path().join("annotations.json"));
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .arg("check")
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(code));
    if code == 1 {
        let error = load.unwrap_err().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains(expected_path),
            "load error missing path {expected_path}: {error}"
        );
        assert!(stderr.contains(expected_path), "{stderr}");
    } else {
        load.unwrap();
    }
}

#[test]
fn quantifier_zero_samples_rejected_on_load() {
    let mut quantifier = quantifier_annotation(
        "infinite-universal",
        serde_json::json!({ "type": "unbounded", "base": "int", "size_hint": 8 }),
    );
    quantifier["sampling"] = serde_json::json!({ "seed": 7, "samples": 0 });
    check_quantifier_load(quantifier, "sampling.samples", 1);
}

#[test]
fn quantifier_unknown_subject_rejected_on_load() {
    let mut quantifier = quantifier_annotation(
        "finite-universal",
        serde_json::json!({ "type": "int-range", "lo": 0, "hi": 1 }),
    );
    quantifier["subject"] = "not_in_catalog".into();
    check_quantifier_load(quantifier, "quantifier.subject", 1);
}

#[test]
fn quantifier_kind_domain_mismatch_rejected_on_load() {
    let mut quantifier = quantifier_annotation(
        "finite-universal",
        serde_json::json!({ "type": "unbounded", "base": "int", "size_hint": 8 }),
    );
    quantifier["sampling"] = serde_json::json!({ "seed": 7, "samples": 10 });
    check_quantifier_load(quantifier, "quantifier.domain", 1);
}

#[test]
fn existential_quantifier_sampling_matches_domain() {
    let finite = quantifier_annotation(
        "existential",
        serde_json::json!({ "type": "int-range", "lo": 0, "hi": 1 }),
    );
    check_quantifier_load(finite, "", 0);
    let mut finite_sampled = quantifier_annotation(
        "existential",
        serde_json::json!({ "type": "int-range", "lo": 0, "hi": 1 }),
    );
    finite_sampled["sampling"] = serde_json::json!({ "seed": 7, "samples": 10 });
    check_quantifier_load(finite_sampled, "quantifier.sampling", 1);
    let mut finite_null = quantifier_annotation(
        "existential",
        serde_json::json!({ "type": "int-range", "lo": 0, "hi": 1 }),
    );
    finite_null["sampling"] = serde_json::Value::Null;
    check_quantifier_load(finite_null, "quantifier.sampling", 1);

    let mut infinite = quantifier_annotation(
        "existential",
        serde_json::json!({ "type": "unbounded", "base": "int", "size_hint": 8 }),
    );
    infinite["sampling"] = serde_json::json!({ "seed": 7, "samples": 10 });
    check_quantifier_load(infinite, "", 0);
    let infinite_unsampled = quantifier_annotation(
        "existential",
        serde_json::json!({ "type": "unbounded", "base": "int", "size_hint": 8 }),
    );
    check_quantifier_load(infinite_unsampled, "quantifier.sampling", 1);
}

#[test]
fn result_readers_do_not_treat_process_success_as_case_success() {
    use living_spec::cli::boundary::parse_results;
    assert!(parse_results("test result: ok. 0 passed;", "libtest").is_err());
    assert!(parse_results("test a ... unknown", "libtest").is_err());
    let libtest = "running 2 tests\ntest alpha::case_a ... ok\ntest beta::case_b ... ignored, intentionally disabled\ntest result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
    let outcomes = parse_results(libtest, "libtest").unwrap();
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0].test_id, "alpha::case_a");
    assert_eq!(outcomes[0].failure_message, None);
    assert_eq!(outcomes[1].status, TestStatus::Skipped);
    let xml = "<testsuite><testcase name=\"case-a\"/><testcase name=\"case-b\"><failure/></testcase><testcase name=\"case-c\"><skipped/></testcase><testcase name=\"case-d\"><error>bad</error></testcase></testsuite>";
    let outcomes = parse_results(xml, "junit").unwrap();
    assert_eq!(
        outcomes.iter().map(|o| o.status).collect::<Vec<_>>(),
        [
            TestStatus::Passed,
            TestStatus::Failed,
            TestStatus::Skipped,
            TestStatus::Errored
        ]
    );
    for xml in [
        "<testsuite>",
        "<testsuite><testcase/></testsuite>",
        "<testsuite><testcase name=\"a\"/><testcase name=\"a\"/></testsuite>",
        "<testsuite><testcase name=\"a\"></testsuite>",
        "<testsuite></testsuite><testcase name=\"a\"/>",
        "<testsuite><testcase name=\"a\"/></testsuite>garbage",
    ] {
        assert!(parse_results(xml, "junit").is_err(), "{xml}");
    }
}

#[test]
fn audit_cli_reports_challenge_usage_and_input_errors() {
    let expected_true = true;
    let result = agent_cli(&[
        "audit",
        "tests/fixtures/cli/agents/seeded/living-spec",
        "--tests",
        "tests/fixtures/cli/agents/seeded",
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(result.stdout)
            .unwrap()
            .contains("challenged"),
        expected_true
    );
    assert_eq!(agent_cli(&["audit"]).status.code(), Some(64));
    let invalid = agent_cli(&[
        "audit",
        "missing-fixture",
        "--tests",
        "tests/fixtures/cli/agents/seeded",
    ]);
    assert_eq!(invalid.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(invalid.stderr)
            .unwrap()
            .starts_with("invalid input:"),
        expected_true
    );
}

#[test]
fn dependency_extension_fields_survive_link_edit() {
    let fixture = Path::new("tests/fixtures/cli/agents/seeded");
    let dir = tempfile::tempdir().unwrap();
    let spec_dir = dir.path().join("living-spec");
    fs::create_dir(&spec_dir).unwrap();
    fs::copy(
        fixture.join("living-spec/spec.md"),
        spec_dir.join("spec.md"),
    )
    .unwrap();
    let path = spec_dir.join("annotations.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.join("living-spec/annotations.json")).unwrap())
            .unwrap();
    value["requirements"]["R-001"]["depends_on"] = serde_json::json!([
        {"id": "R-002", "evidence": "reviewed", "receipt": {"id": 42}}
    ]);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let result = agent_cli(&[
        "link",
        spec_dir.to_str().unwrap(),
        "--tests",
        fixture.to_str().unwrap(),
        "--write",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        saved["requirements"]["R-001"]["depends_on"][0]["receipt"],
        serde_json::json!({"id": 42})
    );
}

#[test]
fn extension_fields_survive_link_and_boundary_writes() {
    fn extend(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                for child in object.values_mut() {
                    extend(child);
                }
                object.insert("x_extension".into(), serde_json::json!("retained"));
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(extend),
            _ => {}
        }
    }
    fn assert_extensions(expected: &serde_json::Value, actual: &serde_json::Value) {
        match expected {
            serde_json::Value::Object(object) => {
                assert_eq!(actual["x_extension"], object["x_extension"]);
                for (key, child) in object {
                    if key != "x_extension" && (child.is_object() || child.is_array()) {
                        let saved = if key == "override" && actual.get(key).is_none() {
                            &actual["invalidated_overrides"][0]["override"]
                        } else {
                            &actual[key]
                        };
                        assert_extensions(child, saved);
                    }
                }
            }
            serde_json::Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    assert_extensions(child, &actual[index]);
                }
            }
            _ => {}
        }
    }
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("spec.md"),
        "- [R-001] Duration is at most 30 ms.\n- [R-002] Count is at most 2 items.\n",
    )
    .unwrap();
    let mut requirement = serde_json::json!({
        "links": [{"test_id":"custom:retained", "fingerprint":"saved", "explanation":"reviewed", "proposed_by":"human", "audit":{"verdict":"accepted", "notes":[], "override":{"by":"human", "reason":"reviewed"}}}],
        "depends_on": [{"id":"R-002", "evidence":"reviewed"}],
        "bounds": [{"quantity":"Duration", "kind":{"type":"numeric", "value":"30", "unit":"ms", "op":{"type":"le"}, "step":{"value":"1", "unit":"ms"}, "integer":true}}],
        "quantifier": {"kind":"finite-universal", "domain":{"type":"int-range", "lo":0, "hi":1}, "subject":"abs_i8", "predicate":{"version":1, "expression":{"op":"eq", "left":{"kind":"field", "source":"input"}, "right":{"kind":"literal", "value":{"type":"int", "value":0}}}}}
    });
    for key in ["links", "depends_on"] {
        extend(&mut requirement[key]);
    }
    requirement["x_extension"] = serde_json::json!("retained");
    let path = dir.path().join("annotations.json");
    let original = serde_json::json!({"schema":"living-spec.annotations/v1", "x_extension":"retained", "requirements":{"R-001": requirement}});
    fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
    let fixture = "tests/fixtures/cli/agents/seeded";
    for args in [
        vec![
            "link",
            dir.path().to_str().unwrap(),
            "--tests",
            fixture,
            "--write",
        ],
        vec!["boundary", "gen", dir.path().to_str().unwrap()],
    ] {
        let result = agent_cli(&args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["x_extension"], original["x_extension"]);
        for key in ["links", "depends_on"] {
            assert_extensions(
                &original["requirements"]["R-001"][key],
                &saved["requirements"]["R-001"][key],
            );
        }
        assert_eq!(saved["requirements"]["R-001"]["x_extension"], "retained");
    }
}

#[test]
fn schema_and_loader_agree_on_additional_properties() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../contracts/living-spec-annotations-v1.schema.json"
    ))
    .unwrap();
    let mut fixture = json!({"schema":"living-spec.annotations/v1", "requirements":{"R-001":{
        "links":[{"test_id":"t", "fingerprint":"f", "revision":"r", "rename_hint":null, "explanation":"e", "proposed_by":"human", "audit":{"verdict":"accepted", "revision":"r", "notes":[], "override":{"by":"human", "reason":"reviewed"}}}],
        "depends_on":[{"id":"R-002", "evidence":"e"}],
        "bounds":[{"quantity":"q", "kind":{"type":"numeric", "value":"1", "unit":"ms", "op":{"type":"le"}, "step":{"value":"1", "unit":"ms"}, "integer":true}}],
        "boundary_cases":[{"quantity":"q", "cases":[{"position":"at", "value":"1", "unit":"ms", "expected":"accepted"}]}],
        "boundary_links":[{"quantity":"q", "position":"at", "value":"1", "test_id":"t"}],
        "quantifier":{"kind":"infinite-universal", "domain":{"type":"product", "parts":[{"type":"int-range", "lo":0, "hi":1},{"type":"unbounded", "base":"int", "size_hint":1}]}, "subject":"abs_i8", "sampling":{"seed":1,"samples":1}, "predicate":{"version":1,"expression":{"op":"eq", "left":{"kind":"field", "source":"input"}, "right":{"kind":"literal", "value":{"type":"int", "value":0}}}}}
    }}});
    let paths = [
        ("requirement", ""),
        ("link", "/links/0"),
        ("dependency", "/depends_on/0"),
        ("bound", "/bounds/0"),
        ("boundaryCase", "/boundary_cases/0/cases/0"),
        ("boundaryLink", "/boundary_links/0"),
        ("quantifier", "/quantifier"),
        ("sampling", "/quantifier/sampling"),
        ("domain", "/quantifier/domain"),
        ("finiteDomain", "/quantifier/domain/parts/0"),
        ("infiniteDomain", "/quantifier/domain/parts/1"),
        ("caseValue", "/quantifier/predicate/expression/right/value"),
        ("operand", "/quantifier/predicate/expression/left"),
        ("predicate", "/quantifier/predicate"),
        ("expression", "/quantifier/predicate/expression"),
        ("comparison", "/quantifier/predicate/expression"),
    ];
    let defs = schema["$defs"].as_object().unwrap();
    assert_eq!(paths.len(), defs.len());
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("annotations.json");
    fs::write(&file, serde_json::to_vec(&fixture).unwrap()).unwrap();
    Annotations::load(&file).unwrap();
    for (name, suffix) in paths {
        let def = &defs[name];
        // Union definitions place additionalProperties on each object alternative.
        let rejects = if let Some(options) = def.get("oneOf") {
            options.as_array().unwrap().iter().all(|option| {
                let option = option
                    .get("$ref")
                    .map(|r| {
                        schema
                            .pointer(r.as_str().unwrap().trim_start_matches('#'))
                            .unwrap()
                    })
                    .unwrap_or(option);
                option["additionalProperties"] == false
            })
        } else {
            def["additionalProperties"] == false
        };
        let pointer = format!("/requirements/R-001{suffix}");
        fixture
            .pointer_mut(&pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("x_extension".into(), json!("retained"));
        fs::write(&file, serde_json::to_vec(&fixture).unwrap()).unwrap();
        let result = Annotations::load(&file);
        if rejects {
            let error = result.unwrap_err().to_string();
            assert!(
                error.contains("x_extension")
                    && error.contains("R-001")
                    && error.contains(
                        &format!("{suffix}/x_extension")
                            .replace("/quantifier", ".quantifier")
                            .replace("/bounds/0", ".bounds[0]")
                            .replace("/boundary_cases", ".boundary_cases")
                            .replace("/boundary_links", ".boundary_links")
                    ),
                "{name}: {error}"
            );
        } else {
            let loaded = result.unwrap();
            loaded.save(&file).unwrap();
            let saved: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
            assert_eq!(
                saved.pointer(&format!("{pointer}/x_extension")),
                Some(&json!("retained")),
                "{name}"
            );
        }
        fixture
            .pointer_mut(&pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("x_extension");
    }
    for suffix in [
        "/bounds/0/kind",
        "/bounds/0/kind/op",
        "/bounds/0/kind/step",
        "/boundary_cases/0",
    ] {
        let pointer = format!("/requirements/R-001{suffix}");
        fixture.pointer_mut(&pointer).unwrap()["x_extension"] = json!(true);
        fs::write(&file, serde_json::to_vec(&fixture).unwrap()).unwrap();
        let error = Annotations::load(&file).unwrap_err().to_string();
        assert!(
            error.contains("x_extension") && error.contains("R-001"),
            "{suffix}: {error}"
        );
        fixture
            .pointer_mut(&pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("x_extension");
    }
    fixture["requirements"]["R-001"]["bounds_source"] =
        json!({"origin":"authored", "x_extension":true});
    fs::write(&file, serde_json::to_vec(&fixture).unwrap()).unwrap();
    let error = Annotations::load(&file).unwrap_err().to_string();
    assert!(error.contains("bounds_source/x_extension"), "{error}");
    fixture["requirements"]["R-001"]
        .as_object_mut()
        .unwrap()
        .remove("bounds_source");
    fixture
        .pointer_mut("/requirements/R-001/quantifier/domain/parts/0")
        .unwrap()["x_extension"] = json!(true);
    fs::write(&file, serde_json::to_vec(&fixture).unwrap()).unwrap();
    fs::write(dir.path().join("spec.md"), "- [R-001] A requirement.\n").unwrap();
    let output = agent_cli(&["check", dir.path().to_str().unwrap()]);
    let error = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        error.contains("invalid input:") && error.contains("quantifier/domain/parts/0/x_extension"),
        "{error}"
    );
}

#[test]
fn link_write_preserves_human_metadata_other_sections_and_is_idempotent() {
    let expected_true = true;
    let fixture = Path::new("tests/fixtures/cli/agents/seeded");
    let dir = tempfile::tempdir().unwrap();
    let spec_dir = dir.path().join("living-spec");
    fs::create_dir(&spec_dir).unwrap();
    fs::copy(
        fixture.join("living-spec/spec.md"),
        spec_dir.join("spec.md"),
    )
    .unwrap();
    let annotations = prepare_human_metadata(fixture);
    let human = annotations.requirements["R-001"].links[0].clone();
    annotations.save(spec_dir.join("annotations.json")).unwrap();
    let args = [
        "link",
        spec_dir.to_str().unwrap(),
        "--tests",
        fixture.to_str().unwrap(),
        "--write",
    ];
    let first = agent_cli(&args);
    assert_eq!(
        first.status.success(),
        expected_true,
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let once = fs::read(spec_dir.join("annotations.json")).unwrap();
    let saved = Annotations::load(spec_dir.join("annotations.json")).unwrap();
    assert_eq!(saved.requirements["R-001"].links[0], human);
    assert_eq!(
        saved.requirements["R-001"].extra["future"],
        serde_json::json!({"keep": true})
    );
    assert_eq!(
        any_link_challenged(&saved.requirements["R-006"].links),
        expected_true
    );
    assert_eq!(agent_cli(&args).status.success(), expected_true);
    assert_eq!(once, fs::read(spec_dir.join("annotations.json")).unwrap());
}

fn prepare_human_metadata(fixture: &Path) -> Annotations {
    let mut annotations = Annotations::load(fixture.join("living-spec/annotations.json")).unwrap();
    let human = &mut annotations.requirements.get_mut("R-001").unwrap().links[0];
    human.proposed_by = "human".into();
    human.explanation = "keep authored explanation".into();
    human.revision = "authored revision".into();
    human
        .extra
        .insert("unit".into(), serde_json::json!("seconds"));
    human
        .audit
        .extra
        .insert("reviewer".into(), serde_json::json!("synthetic-reviewer"));
    annotations
        .requirements
        .get_mut("R-001")
        .unwrap()
        .extra
        .insert("future".into(), serde_json::json!({"keep": true}));
    annotations
}

fn any_link_challenged(links: &[Link]) -> bool {
    links
        .iter()
        .any(|link| link.audit.verdict == AuditVerdict::Challenged)
}

#[test]
fn cli_challenge_exit_is_distinct_from_usage_and_data_errors() {
    let fixture = "tests/fixtures/cli/agents/seeded";
    let result = agent_cli(&[
        "audit",
        &format!("{fixture}/living-spec"),
        "--tests",
        fixture,
    ]);
    assert_eq!(result.status.code(), Some(1));
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("R-001 ->") && stdout.contains("placeholder"));
    assert!(stdout.contains("R-006 ->") && stdout.contains("unrelated"));
    assert!(stdout.contains("challenged"));
    for args in [
        vec!["audit"],
        vec!["audit", fixture, "--bad", fixture],
        vec!["link", fixture, "--tests"],
    ] {
        assert_eq!(agent_cli(&args).status.code(), Some(64));
    }
    let invalid = agent_cli(&["audit", "missing-fixture", "--tests", fixture]);
    assert_eq!(invalid.status.code(), Some(1));
    assert!(String::from_utf8(invalid.stderr)
        .unwrap()
        .starts_with("invalid input:"));
    assert!(String::from_utf8(result.stderr)
        .unwrap()
        .contains("audit challenged one or more links"));
}

#[test]
fn audit_write_rebinds_existing_links_only() {
    let dir = tempfile::tempdir().unwrap();
    let spec_dir = dir.path().join("living-spec");
    fs::create_dir(&spec_dir).unwrap();
    let spec = spec_dir.join("spec.md");
    fs::write(&spec, "- [R-001] Export timestamp is 30.\n").unwrap();
    let source = dir.path().join("test.py");
    fs::write(
        &source,
        "def test_export_timestamp():\n    assert export_timestamp(30) == 30\n",
    )
    .unwrap();
    let entry = TestIndex::build(dir.path()).unwrap().entries.remove(0);
    let mut authored = link(&entry);
    authored.proposed_by = "human".into();
    authored.audit.extra.insert(
        "override".into(),
        json!({"by":"human", "reason":"reviewed"}),
    );
    let file = spec_dir.join("annotations.json");
    let before = json!({"schema":"living-spec.annotations/v1", "other":{"keep":true}, "requirements":{"R-001":{"links":[authored], "bounds":[], "future":{"keep":true}}}});
    fs::write(&file, serde_json::to_vec(&before).unwrap()).unwrap();
    fs::write(&spec, "- [R-001] Export timestamp is at least 30.\n").unwrap();
    fs::write(&source, "def test_export_timestamp():\n    assert export_timestamp(30) >= 30\n\ndef test_export_timestamp_extra():\n    assert export_timestamp(30) == 30\n").unwrap();
    let args = [
        "audit",
        spec_dir.to_str().unwrap(),
        "--tests",
        dir.path().to_str().unwrap(),
        "--write",
    ];
    let result = agent_cli(&args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let once = fs::read(&file).unwrap();
    let after: Value = serde_json::from_slice(&once).unwrap();
    let updated = &after["requirements"]["R-001"]["links"][0];
    let current = TestIndex::build(dir.path())
        .unwrap()
        .entries
        .into_iter()
        .find(|e| e.id == entry.id)
        .unwrap();
    assert_eq!(updated["fingerprint"], current.fingerprint);
    assert_ne!(updated["fingerprint"], entry.fingerprint);
    let spec_revision = Spec::parse(&fs::read_to_string(spec).unwrap())
        .unwrap()
        .lines[0]
        .revision
        .clone();
    assert_eq!(updated["revision"], spec_revision);
    assert_eq!(updated["audit"]["revision"], spec_revision);
    assert_eq!(
        after["requirements"]["R-001"]["links"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut normalized = after.clone();
    for field in ["test_id", "fingerprint", "rename_hint", "revision"] {
        let saved = &mut normalized["requirements"]["R-001"]["links"][0];
        if let Some(value) = before["requirements"]["R-001"]["links"][0].get(field) {
            saved[field] = value.clone();
        } else {
            saved.as_object_mut().unwrap().remove(field);
        }
    }
    normalized["requirements"]["R-001"]["links"][0]["audit"] =
        before["requirements"]["R-001"]["links"][0]["audit"].clone();
    assert_eq!(normalized, before);
    assert!(updated["audit"].get("override").is_none());
    assert_eq!(
        updated["audit"]["invalidated_overrides"][0]["override"],
        before["requirements"]["R-001"]["links"][0]["audit"]["override"]
    );
    assert!(agent_cli(&args).status.success());
    assert_eq!(once, fs::read(&file).unwrap());
    let edited = fs::read_to_string(&source).unwrap();
    fs::write(
        &source,
        edited.replacen("test_export_timestamp()", "test_renamed_timestamp()", 1),
    )
    .unwrap();
    assert!(agent_cli(&args).status.success());
    let renamed = Annotations::load(file).unwrap();
    assert!(renamed.requirements["R-001"].links[0]
        .test_id
        .contains("test_renamed_timestamp"));
}

#[test]
fn audit_write_keeps_challenged_links() {
    let dir = tempfile::tempdir().unwrap();
    let spec_dir = dir.path().join("living-spec");
    fs::create_dir(&spec_dir).unwrap();
    fs::write(
        spec_dir.join("spec.md"),
        "- [R-001] Export timestamp is 30.\n",
    )
    .unwrap();
    let source = dir.path().join("test.py");
    fs::write(
        &source,
        "def test_export_timestamp():\n    assert export_timestamp(30) == 30\n",
    )
    .unwrap();
    let entry = TestIndex::build(dir.path()).unwrap().entries.remove(0);
    let file = spec_dir.join("annotations.json");
    fs::write(&file, serde_json::to_vec(&json!({"schema":"living-spec.annotations/v1", "requirements":{"R-001":{"links":[link(&entry)]}}})).unwrap()).unwrap();
    fs::write(source, "def test_export_timestamp():\n    assert True\n").unwrap();
    let result = agent_cli(&[
        "audit",
        spec_dir.to_str().unwrap(),
        "--tests",
        dir.path().to_str().unwrap(),
        "--write",
    ]);
    assert_eq!(result.status.code(), Some(1));
    let saved = Annotations::load(file).unwrap();
    assert_eq!(saved.requirements["R-001"].links.len(), 1);
    assert_eq!(
        saved.requirements["R-001"].links[0].audit.verdict,
        AuditVerdict::Challenged
    );
}

#[test]
fn cli_reaudits_changed_and_uniquely_renamed_sources_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let spec_dir = dir.path().join("living-spec");
    fs::create_dir(&spec_dir).unwrap();
    fs::write(
        spec_dir.join("spec.md"),
        "- [R-001]  Export timestamp is 30.\n",
    )
    .unwrap();
    let original = "def test_export_timestamp():\n    assert export_timestamp(30) == 30\n";
    fs::write(dir.path().join("test.py"), original).unwrap();
    let indexed = TestIndex::build(dir.path()).unwrap().entries.remove(0);
    let mut annotations = Annotations {
        schema: "living-spec.annotations/v1".into(),
        requirements: BTreeMap::new(),
        extra: BTreeMap::new(),
    };
    let mut authored = link(&indexed);
    authored.rename_hint = None; // Exact identity needs only ID + fingerprint.
    authored.proposed_by = "human".into();
    annotations
        .requirements
        .entry("R-001".into())
        .or_default()
        .links
        .push(authored);
    annotations.save(spec_dir.join("annotations.json")).unwrap();
    let saved = fs::read(spec_dir.join("annotations.json")).unwrap();
    let args = [
        "audit",
        spec_dir.to_str().unwrap(),
        "--tests",
        dir.path().to_str().unwrap(),
    ];
    let exact = agent_cli(&args);
    assert!(exact.status.success());
    assert_eq!(saved, fs::read(spec_dir.join("annotations.json")).unwrap());
    assert!(String::from_utf8(exact.stdout)
        .unwrap()
        .contains(&revision(" Export timestamp is 30.")));
    annotations.requirements.get_mut("R-001").unwrap().links[0].rename_hint =
        Some(indexed.rename_hint.clone());
    annotations.save(spec_dir.join("annotations.json")).unwrap();
    let with_hint = fs::read(spec_dir.join("annotations.json")).unwrap();
    fs::write(
        dir.path().join("test.py"),
        original.replace("test_export_timestamp", "test_renamed_timestamp"),
    )
    .unwrap();
    let renamed = agent_cli(&args);
    assert!(
        renamed.status.success(),
        "{}",
        String::from_utf8_lossy(&renamed.stderr)
    );
    assert!(String::from_utf8(renamed.stdout)
        .unwrap()
        .contains("unique rename_hint"));
    assert_eq!(
        with_hint,
        fs::read(spec_dir.join("annotations.json")).unwrap()
    );
    fs::write(
        dir.path().join("test.py"),
        original.replace("== 30", ">= 30"),
    )
    .unwrap();
    let changed = agent_cli(&args);
    assert!(changed.status.success());
    assert!(String::from_utf8(changed.stdout)
        .unwrap()
        .contains("source changed"));
    assert_eq!(
        with_hint,
        fs::read(spec_dir.join("annotations.json")).unwrap()
    );
    fs::write(
        dir.path().join("test.py"),
        "def test_export_timestamp():\n    assert True\n",
    )
    .unwrap();
    let cheated = agent_cli(&args);
    assert_eq!(cheated.status.code(), Some(1));
    assert!(String::from_utf8(cheated.stdout)
        .unwrap()
        .contains("placeholder"));
}

#[test]
fn cli_generates_preserves_sections_exports_and_exits_three() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("spec.md"),
        "- [R-001] Delay is at most 30 s.\n",
    )
    .unwrap();
    let f = fixture(1);
    fs::write(dir.path().join("annotations.json"), serde_json::to_vec(&json!({"schema":"living-spec.annotations/v1","future_root":42,"requirements":{"R-001":{"depends_on":[],"quantifier":{"kind":"finite-universal","domain":{"type":"int-range","lo":0,"hi":1},"subject":"abs_i8","predicate":{"version":1,"expression":{"op":"eq","left":{"kind":"field","source":"input"},"right":{"kind":"literal","value":{"type":"int","value":0}}}}},"foreign":{"retain":true},"boundary_links":f.links}}})).unwrap()).unwrap();
    assert!(boundary_cli(dir.path(), &["gen"]).status.success());
    let a = Annotations::load(dir.path().join("annotations.json")).unwrap();
    assert_eq!(a.extra["future_root"], 42);
    let raw: Value =
        serde_json::from_slice(&fs::read(dir.path().join("annotations.json")).unwrap()).unwrap();
    assert_eq!(raw["requirements"]["R-001"]["depends_on"], json!([]));
    assert_eq!(
        a.requirements["R-001"].extra["foreign"],
        json!({"retain":true})
    );
    assert!(a.requirements["R-001"].quantifier.is_some());
    assert_eq!(
        a.requirements["R-001"].extra["boundary_cases"][0]["cases"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let results = dir.path().join("results.txt");
    fs::write(
        &results,
        format!(
            "running 1 test\ntest {} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n",
            f.outcomes[0].test_id
        ),
    )
    .unwrap();
    let md = dir.path().join("out.md");
    let json = dir.path().join("out.json");
    let out = boundary_cli(
        dir.path(),
        &[
            "coverage",
            "--results",
            results.to_str().unwrap(),
            "--format",
            "libtest",
            "--md",
            md.to_str().unwrap(),
            "--json",
            json.to_str().unwrap(),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(fs::read_to_string(md)
        .unwrap()
        .contains("| Delay | above |"));
    let value: Value = serde_json::from_slice(&fs::read(json).unwrap()).unwrap();
    assert!(validate(&schema(), &value, &schema()));
    let before = fs::read(dir.path().join("annotations.json")).unwrap();
    fs::write(
        dir.path().join("spec.md"),
        "- [R-001] Delay is at most 30 s.\n- [R-002] Response is fast.\n",
    )
    .unwrap();
    assert_eq!(boundary_cli(dir.path(), &["gen"]).status.code(), Some(1));
    assert_eq!(
        before,
        fs::read(dir.path().join("annotations.json")).unwrap()
    );
}

#[test]
fn cli_zero_exit_with_junit_and_empty_override_survives() {
    let dir = tempfile::tempdir().unwrap();
    let f = fixture(18);
    fs::write(
        dir.path().join("spec.md"),
        format!("- [{}] {}\n", f.requirement, f.text),
    )
    .unwrap();
    let annotations = json!({"schema":"living-spec.annotations/v1","requirements":{&f.requirement:{"bounds":f.expected_bounds,"boundary_links":f.links}}});
    fs::write(
        dir.path().join("annotations.json"),
        serde_json::to_vec(&annotations).unwrap(),
    )
    .unwrap();
    let results = dir.path().join("results.xml");
    let xml = format!(
        "<testsuite>{}</testsuite>",
        f.outcomes
            .iter()
            .map(|o| format!("<testcase name=\"{}\"/>", o.test_id))
            .collect::<String>()
    );
    fs::write(&results, xml).unwrap();
    let out = boundary_cli(
        dir.path(),
        &[
            "coverage",
            "--results",
            results.to_str().unwrap(),
            "--format",
            "junit",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    fs::write(dir.path().join("spec.md"), "- [R-001] Response is fast.\n").unwrap();
    fs::write(dir.path().join("annotations.json"),r#"{"schema":"living-spec.annotations/v1","requirements":{"R-001":{"bounds":[],"foreign":1}}}"#).unwrap();
    for _ in 0..2 {
        let out = boundary_cli(dir.path(), &["gen"]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let raw: Value =
        serde_json::from_slice(&fs::read(dir.path().join("annotations.json")).unwrap()).unwrap();
    assert_eq!(raw["requirements"]["R-001"]["bounds"], json!([]));
    assert_eq!(raw["requirements"]["R-001"]["foreign"], 1);
}

#[test]
fn cli_reports_cycles_with_exit_four() {
    let dir = tempfile::tempdir().unwrap();
    let f = deps_fixture("18");
    fs::write(dir.path().join("spec.md"), f.spec).unwrap();
    f.annotations
        .save(dir.path().join("annotations.json"))
        .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .args(["deps", "check"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
    assert!(String::from_utf8(out.stderr)
        .unwrap()
        .contains("R-001 → R-002 → R-001"));
}

#[test]
fn quantifier_cli_dispatches_checks_and_writes_each_export() {
    let expected_true = true;
    let expected_false = false;
    let dir = tempdir();
    let binary = env!("CARGO_BIN_EXE_living-spec");
    let output = Command::new(binary)
        .args(["quantifier", "count"])
        .output()
        .unwrap();
    assert_eq!(output.status.success(), expected_true);
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "30");

    let finite = Command::new(binary)
        .args(["quantifier", "check", "reference", "--finite-only"])
        .output()
        .unwrap();
    assert_eq!(finite.status.success(), expected_true);
    let finite = parse_json_value(&finite.stdout);
    let finite = finite.as_array().unwrap();
    assert_eq!(finite.len(), 16);
    assert_eq!(all_coverage_mode(finite, "exhaustive"), expected_true);

    let sampled = Command::new(binary)
        .args(["quantifier", "check", "reference", "--sampled-only"])
        .output()
        .unwrap();
    assert_eq!(sampled.status.success(), expected_true);
    let sampled = parse_json_value(&sampled.stdout);
    let sampled = sampled.as_array().unwrap();
    assert_eq!(sampled.len(), 14);
    assert_eq!(all_coverage_mode(sampled, "sampled"), expected_true);

    let view_file = dir.path().join("coverage.html");
    let view = Command::new(binary)
        .args([
            "quantifier",
            "view",
            "reference",
            "--out",
            view_file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(view.status.success(), expected_true);
    assert_eq!(
        fs::read_to_string(view_file)
            .unwrap()
            .contains("Tested here"),
        expected_true
    );

    let edition_dir = dir.path().join("edition");
    let edition = Command::new(binary)
        .args([
            "quantifier",
            "edition",
            "reference",
            "--out-dir",
            edition_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(edition.status.success(), expected_true);
    assert_eq!(edition_dir.join("edition.json").is_file(), expected_true);
    assert_eq!(edition_dir.join("edition.md").is_file(), expected_true);

    let examples_dir = dir.path().join("counterexamples");
    let examples = Command::new(binary)
        .args([
            "quantifier",
            "counterexample",
            "reference",
            "--out-dir",
            examples_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(examples.status.success(), expected_true);
    assert_eq!(
        examples_dir.join("counterexamples.json").is_file(),
        expected_true
    );
    assert_eq!(
        examples_dir.join("counterexamples.md").is_file(),
        expected_true
    );

    let invalid = Command::new(binary)
        .args(["quantifier", "check", "reference", "--unsupported"])
        .output()
        .unwrap();
    assert_eq!(invalid.status.success(), expected_false);
    assert_eq!(invalid.stdout.is_empty(), expected_true);
}

fn all_coverage_mode(reports: &[serde_json::Value], expected: &str) -> bool {
    reports
        .iter()
        .all(|report| report["coverage"]["mode"] == expected)
}

fn parse_json_value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}
