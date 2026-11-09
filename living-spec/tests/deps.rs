use living_spec::{
    core::{
        annotations::Annotations,
        badge::{score_line_at_revision, Badge, LineStatus, Reason},
        spec::Spec,
        test_identity::{fingerprint, Fingerprints, TestIdentity, TestOutcome, TestStatus},
    },
    deps::{
        model::DepGraph,
        propagate::{apply_qualified, nearest_non_proven},
        report,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

fn report_fixture() -> tempfile::TempDir {
    use living_spec::core::freshness::{ResultsSnapshot, ScoreInputs};
    let dir = tempfile::tempdir_in("target").unwrap();
    let spec = "- [R-001] First requirement.\n- [R-002] Second requirement.\n";
    let annotations = serde_json::json!({"schema":"living-spec.annotations/v1", "requirements": {
        "R-002": {"depends_on":[{"id":"R-001","evidence":"authored"}]}
    }});
    let results = include_str!("fixtures/runner/before.txt");
    fs::write(dir.path().join("spec.md"), spec).unwrap();
    fs::write(
        dir.path().join("annotations.json"),
        serde_json::to_vec(&annotations).unwrap(),
    )
    .unwrap();
    let result_path = dir.path().join("results.txt");
    fs::write(&result_path, results).unwrap();
    let snapshot = ResultsSnapshot::from_bytes(
        dir.path(),
        &result_path,
        results.as_bytes().to_vec(),
        "libtest",
        BTreeMap::new(),
    )
    .unwrap();
    let lines = vec![
        LineStatus {
            id: "R-001".into(),
            badge: Badge::Broken,
            reasons: vec![Reason::LinkUnresolved],
            tests: vec![],
        },
        LineStatus {
            id: "R-002".into(),
            badge: Badge::Qualified,
            reasons: vec![Reason::PrerequisiteBroken {
                path: vec!["R-002".into(), "R-001".into()],
            }],
            tests: vec![],
        },
    ];
    let outcomes = living_spec::adapters::parse_libtest(results).unwrap();
    let inputs = ScoreInputs::capture(
        spec.as_bytes(),
        &annotations,
        &snapshot,
        &outcomes,
        &lines,
        &[],
        &[],
    )
    .unwrap();
    fs::write(
        dir.path().join("status.json"),
        serde_json::to_vec(&serde_json::json!({"inputs":inputs,"statuses":lines})).unwrap(),
    )
    .unwrap();
    dir
}

fn deps_report(dir: &std::path::Path, allow_stale: bool) -> std::process::Output {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"));
    command
        .arg("deps")
        .arg("report")
        .arg(dir)
        .arg("--status")
        .arg(dir.join("status.json"));
    if allow_stale {
        command.arg("--allow-stale");
    }
    command.output().unwrap()
}

#[test]
fn removed_dependency_is_not_reported() {
    let dir = report_fixture();
    fs::write(
        dir.path().join("annotations.json"),
        r#"{"schema":"living-spec.annotations/v1","requirements":{}}"#,
    )
    .unwrap();
    let rejected = deps_report(dir.path(), false);
    assert_eq!(rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("stale"));
    let allowed = deps_report(dir.path(), true);
    assert!(allowed.status.success());
    let output = String::from_utf8(allowed.stdout).unwrap();
    assert!(output.contains("R-002 — stale"));
    assert!(!output.contains("R-002 → R-001"));
}

#[test]
fn edited_requirement_is_stale_in_report() {
    let dir = report_fixture();
    fs::write(
        dir.path().join("spec.md"),
        "- [R-001] Edited requirement.\n- [R-002] Second requirement.\n",
    )
    .unwrap();
    assert_eq!(deps_report(dir.path(), false).status.code(), Some(1));
    let output = deps_report(dir.path(), true);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("R-001 — stale") && text.contains("R-002 — stale"));
    assert!(!text.contains("R-002 → R-001"));
}

#[test]
fn unchanged_status_reports_current_paths() {
    let dir = report_fixture();
    let output = deps_report(dir.path(), false);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("R-002 → R-001"));
}
#[derive(Deserialize)]
struct Fixture {
    spec: String,
    annotations: Annotations,
    statuses: Vec<LineStatus>,
    expected: BTreeMap<String, Vec<String>>,
    cycles: Vec<Vec<String>>,
}
fn load(name: &str) -> Fixture {
    serde_json::from_str(
        &fs::read_to_string(format!("tests/fixtures/deps/graphs/{name}.json")).unwrap(),
    )
    .unwrap()
}
fn graph(f: &Fixture) -> DepGraph {
    DepGraph::new(&Spec::parse(&f.spec).unwrap(), &f.annotations).unwrap()
}

fn dependency_report_schema(name: &str) -> Value {
    match name {
        "living-spec-dependency-report-v1.schema.json" => serde_json::from_str(include_str!(
            "../../contracts/living-spec-dependency-report-v1.schema.json"
        ))
        .unwrap(),
        "living-spec-status-v1.schema.json" => serde_json::from_str(include_str!(
            "../../contracts/living-spec-status-v1.schema.json"
        ))
        .unwrap(),
        _ => panic!("unexpected local schema reference: {name}"),
    }
}

fn validates_dependency_report(schema: &Value, value: &Value, root: &Value) -> bool {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let (document, pointer) = reference
            .split_once('#')
            .expect("schema reference fragment");
        let referenced = if document.is_empty() {
            root.clone()
        } else {
            dependency_report_schema(document)
        };
        return referenced
            .pointer(pointer)
            .is_some_and(|schema| validates_dependency_report(schema, value, &referenced));
    }
    if let Some(kind) = schema.get("type") {
        let matches_type = |kind: &str| match kind {
            "array" => value.is_array(),
            "object" => value.is_object(),
            "string" => value.is_string(),
            "null" => value.is_null(),
            other => panic!("unsupported dependency report schema type: {other}"),
        };
        let matches = match kind {
            Value::String(kind) => matches_type(kind),
            Value::Array(kinds) => kinds
                .iter()
                .any(|kind| matches_type(kind.as_str().expect("schema type name"))),
            _ => panic!("invalid schema type"),
        };
        if !matches {
            return false;
        }
    }
    if schema
        .get("const")
        .is_some_and(|expected| expected != value)
    {
        return false;
    }
    if schema
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|allowed| !allowed.contains(value))
    {
        return false;
    }
    if let Some(alternatives) = schema.get("oneOf").and_then(Value::as_array) {
        if alternatives
            .iter()
            .filter(|alternative| validates_dependency_report(alternative, value, root))
            .count()
            != 1
        {
            return false;
        }
    }
    if let Some(object) = value.as_object() {
        if schema
            .get("required")
            .and_then(Value::as_array)
            .is_some_and(|required| {
                required
                    .iter()
                    .any(|key| !object.contains_key(key.as_str().unwrap()))
            })
        {
            return false;
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        for (key, child) in object {
            if let Some(child_schema) = properties.and_then(|properties| properties.get(key)) {
                if !validates_dependency_report(child_schema, child, root) {
                    return false;
                }
            } else if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                return false;
            }
        }
    }
    if let Some(array) = value.as_array() {
        if let Some(item_schema) = schema.get("items") {
            if array
                .iter()
                .any(|item| !validates_dependency_report(item_schema, item, root))
            {
                return false;
            }
        }
    }
    true
}
#[test]
fn deps_reference_set() {
    let paths: Vec<_> = fs::read_dir("tests/fixtures/deps/graphs")
        .unwrap()
        .collect();
    assert_eq!(paths.len(), 20);
    for entry in paths {
        let f: Fixture = serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
        let g = graph(&f);
        assert_eq!(g.detect_cycles(), f.cycles);
        let initially_proven: std::collections::BTreeSet<_> = f
            .statuses
            .iter()
            .filter(|s| s.badge == Badge::Proven)
            .map(|s| s.id.clone())
            .collect();
        let original_tests: BTreeMap<_, _> = f
            .statuses
            .iter()
            .map(|status| (status.id.clone(), status.tests.clone()))
            .collect();
        let mut lines = f.statuses;
        apply_qualified(&mut lines, &g);
        assert!(lines
            .iter()
            .all(|status| original_tests.get(&status.id) == Some(&status.tests)));
        let actual: BTreeMap<_, _> = lines
            .into_iter()
            .filter(|s| s.badge == Badge::Qualified && initially_proven.contains(&s.id))
            .map(|s| {
                let path = match &s.reasons[0] {
                    Reason::PrerequisiteBroken { path }
                    | Reason::PrerequisiteUnproven { path }
                    | Reason::PrerequisiteQualified { path } => path.clone(),
                    Reason::DependencyCycle { path } => path.as_ref().clone(),
                    _ => panic!("unexpected reason"),
                };
                (s.id, path)
            })
            .collect();
        assert_eq!(actual, f.expected);
    }
    println!("deps_reference_set: 20/20");
}
#[test]
fn cycle_fixture_reported() {
    assert!(!graph(&load("18")).detect_cycles().is_empty());
}
#[test]
fn prerequisite_red_marks_dependents_qualified() {
    let f = load("01");
    let g = graph(&f);
    let mut s = f.statuses;
    let failing_test = s[0].tests[0].test_id.clone();
    apply_qualified(&mut s, &g);
    let md = report::markdown(&report::impacts(&g, &s));
    assert!(md.contains(&format!(
        "R-002 → R-001 (broken: test {failing_test} failed)"
    )));
}
#[test]
fn dependent_tests_untouched() {
    let f = load("01");
    let g = graph(&f);
    let mut s = f.statuses;
    assert_eq!(s[0].tests[0].status, TestStatus::Failed);
    assert_eq!(s[1].tests[0].status, TestStatus::Passed);
    let before = s[1].tests.clone();
    let prerequisite_before = s[0].tests.clone();
    apply_qualified(&mut s, &g);
    assert_eq!(s[1].tests, before);
    assert_eq!(s[0].tests, prerequisite_before);
    assert_eq!(s[1].badge, Badge::Qualified);
}
#[test]
fn missing_evidence_is_error() {
    let expected_true = true;

    let mut f = load("01");
    f.annotations
        .requirements
        .get_mut("R-002")
        .unwrap()
        .depends_on[0]
        .evidence
        .clear();
    assert_eq!(
        DepGraph::new(&Spec::parse(&f.spec).unwrap(), &f.annotations).is_err(),
        expected_true
    );
    let raw = r#"{"id":"R-001"}"#;
    assert_eq!(
        serde_json::from_str::<living_spec::core::annotations::Dependency>(raw).is_err(),
        expected_true
    );
}

#[test]
fn invalid_edges_are_rejected() {
    let mut f = load("01");
    let dep = &mut f
        .annotations
        .requirements
        .get_mut("R-002")
        .unwrap()
        .depends_on[0];
    dep.id = "R-999".into();
    assert!(DepGraph::new(&Spec::parse(&f.spec).unwrap(), &f.annotations).is_err());
    f.annotations
        .requirements
        .get_mut("R-002")
        .unwrap()
        .depends_on[0]
        .id = "R-002".into();
    assert!(DepGraph::new(&Spec::parse(&f.spec).unwrap(), &f.annotations).is_err());
}

// The review's two-link prerequisite: the first failure is challenged, while
// the second link is either unresolved or an admissible failure.
fn diagnostic_prerequisite(
    second: &str,
    status: Option<TestStatus>,
) -> (DepGraph, Vec<LineStatus>) {
    let spec =
        Spec::parse("- [R-001] Synthetic prerequisite.\n- [R-002] Synthetic dependent.\n").unwrap();
    let digest = fingerprint("synthetic test source");
    let test_id = |name: &str| format!("rust:synthetic:src/lib.rs:{name}");
    let link = |name: &str, revision: &str, verdict: &str| {
        serde_json::json!({
            "test_id": test_id(name), "fingerprint": digest, "revision": revision,
            "explanation": "Synthetic evidence", "proposed_by": "human",
            "audit": {"verdict": verdict, "revision": revision, "notes": []}
        })
    };
    let annotations: Annotations = serde_json::from_value(serde_json::json!({
        "schema": "living-spec.annotations/v1",
        "requirements": {
            "R-001": {"links": [
                link("diagnostic", &spec.lines[0].revision, "challenged"),
                link(second, &spec.lines[0].revision, "accepted")
            ]},
            "R-002": {
                "links": [link("dependent", &spec.lines[1].revision, "accepted")],
                "depends_on": [{"id": "R-001", "evidence": "authored"}]
            }
        }
    }))
    .unwrap();
    let mut outcomes = vec![
        TestOutcome {
            test_id: test_id("diagnostic"),
            status: TestStatus::Failed,
            duration_ms: Some(1),
            failure_message: None,
        },
        TestOutcome {
            test_id: test_id("dependent"),
            status: TestStatus::Passed,
            duration_ms: Some(1),
            failure_message: None,
        },
    ];
    if let Some(status) = status {
        outcomes.push(TestOutcome {
            test_id: test_id(second),
            status,
            duration_ms: Some(1),
            failure_message: None,
        });
    }
    let identities: Fingerprints = outcomes
        .iter()
        .map(|outcome| {
            (
                outcome.test_id.clone(),
                TestIdentity {
                    fingerprint: digest.clone(),
                    rename_hint: None,
                },
            )
        })
        .collect();
    let statuses = spec
        .lines
        .iter()
        .map(|line| {
            score_line_at_revision(
                &line.id,
                &line.revision,
                &annotations.requirements[&line.id],
                &outcomes,
                &identities,
                &BTreeSet::new(),
            )
        })
        .collect();
    (DepGraph::new(&spec, &annotations).unwrap(), statuses)
}

#[test]
fn finding_1_diagnostic_failures_are_not_the_cause_of_a_broken_prerequisite() {
    let (graph, mut statuses) = diagnostic_prerequisite("missing", None);
    assert_eq!(statuses[0].badge, Badge::Broken);
    assert_eq!(
        statuses[0].reasons,
        [Reason::LinkChallenged, Reason::LinkUnresolved]
    );
    assert!(!statuses[0].tests[0].admissible);
    assert!(statuses[1].tests[0].admissible);
    let original_tests: Vec<_> = statuses.iter().map(|status| status.tests.clone()).collect();
    apply_qualified(&mut statuses, &graph);
    let reports = report::impacts(&graph, &statuses);
    assert_eq!(reports[0].broken_prerequisites[0].failing_test, None);
    assert!(reports[0].explanation.contains("link_unresolved"));
    let json = serde_json::to_value(&reports).unwrap();
    assert_eq!(
        json[0]["selected_prerequisite"]["reasons"],
        serde_json::json!(["link_challenged", "link_unresolved"])
    );
    assert!(json[0]["selected_prerequisite"]["failing_test"].is_null());
    let markdown = report::markdown(&reports);
    assert!(markdown.contains("link_unresolved"));
    assert!(!markdown.contains("diagnostic failed"));
    assert_eq!(
        statuses
            .iter()
            .map(|status| status.tests.clone())
            .collect::<Vec<_>>(),
        original_tests
    );
}

#[test]
fn finding_1_diagnostic_failure_does_not_mask_an_admissible_failure() {
    for status in [TestStatus::Failed, TestStatus::Errored] {
        let (graph, mut statuses) = diagnostic_prerequisite("causal", Some(status));
        assert!(!statuses[0].tests[0].admissible);
        assert!(statuses[0].tests[1].admissible);
        apply_qualified(&mut statuses, &graph);
        let reports = report::impacts(&graph, &statuses);
        let cause = &reports[0].broken_prerequisites[0];
        assert_eq!(
            cause.failing_test.as_deref(),
            Some("rust:synthetic:src/lib.rs:causal")
        );
        assert_eq!(cause.failing_test_status, Some(status));
        assert!(!reports[0].explanation.contains("diagnostic failed"));
        assert!(reports[0]
            .explanation
            .contains(if status == TestStatus::Failed {
                "causal failed"
            } else {
                "causal errored"
            }));
    }
}

#[test]
fn finding_1_old_status_without_provenance_leaves_causal_failure_unknown() {
    let f = load("01");
    let graph = graph(&f);
    let mut json = serde_json::to_value(&f.statuses).unwrap();
    json[0]["tests"][0]
        .as_object_mut()
        .unwrap()
        .remove("admissible");
    let mut statuses: Vec<LineStatus> = serde_json::from_value(json.clone()).unwrap();
    assert!(!statuses[0].tests[0].admissible);
    apply_qualified(&mut statuses, &graph);
    let reports = report::impacts(&graph, &statuses);
    assert_eq!(reports[0].broken_prerequisites[0].failing_test, None);
    assert!(reports[0].explanation.contains("test_failed"));
    // CLI status loading must retain new provenance and default old files too.
    let dir = tempfile::tempdir_in("target").unwrap();
    let path = dir.path().join("status.json");
    fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(!living_spec::web::load_status(&path).unwrap()[0].tests[0].admissible);
    fs::write(&path, serde_json::to_vec(&f.statuses).unwrap()).unwrap();
    assert!(living_spec::web::load_status(&path).unwrap()[0].tests[0].admissible);
}

#[test]
fn finding_3_dependency_exports_preserve_the_untested_boundary_reason() {
    let f = load("01");
    let graph = graph(&f);
    for reason in [Reason::BoundaryGap, Reason::NoLink, Reason::NotExecuted] {
        let mut statuses = f.statuses.clone();
        statuses[0].badge = Badge::Unproven;
        statuses[0].reasons = vec![reason.clone()];
        statuses[0].tests[0].status = TestStatus::Passed;
        apply_qualified(&mut statuses, &graph);
        assert_eq!(statuses[1].badge, Badge::Qualified);
        let reports = report::impacts(&graph, &statuses);
        let json = serde_json::to_value(&reports).unwrap();
        let selected = &json[0]["selected_prerequisite"];
        assert_eq!(selected["id"], "R-001");
        assert_eq!(selected["text"], graph.texts["R-001"]);
        assert_eq!(selected["badge"], "unproven");
        assert_eq!(selected["reasons"], serde_json::json!([reason]));
        let reason_text = serde_json::to_value(&reason).unwrap();
        let markdown = report::markdown(&reports);
        assert!(markdown.contains(&format!("unproven: {}", reason_text.as_str().unwrap())));
        assert!(markdown.contains(&graph.texts["R-001"]));
        assert!(reports[0].broken_prerequisites.is_empty());
        if reason == Reason::BoundaryGap {
            assert_eq!(
                selected["boundary_reference"],
                "annotations.json#/requirements/R-001/bounds"
            );
            assert!(markdown.contains("annotations.json#/requirements/R-001/bounds"));
            assert!(markdown.contains("Case execution details are not included"));
        } else {
            assert!(selected["boundary_reference"].is_null());
        }
    }
}

#[test]
fn finding_drill2_dependency_report_schema_accepts_nonempty_prerequisites() {
    let spec = Spec::parse(
        "- [R-001] Synthetic broken prerequisite.\n- [R-002] Synthetic unproven prerequisite.\n- [R-003] Synthetic dependent.\n- [R-004] Synthetic qualified prerequisite.\n- [R-005] Synthetic transitive unproven prerequisite.\n",
    )
    .unwrap();
    let annotations: Annotations = serde_json::from_value(serde_json::json!({
        "schema": "living-spec.annotations/v1",
        "requirements": {
            "R-003": {"depends_on": [
                {"id": "R-001", "evidence": "authored"},
                {"id": "R-002", "evidence": "authored"},
                {"id": "R-004", "evidence": "authored"}
            ]},
            "R-004": {"depends_on": [
                {"id": "R-005", "evidence": "authored"}
            ]}
        }
    }))
    .unwrap();
    let graph = DepGraph::new(&spec, &annotations).unwrap();
    let mut statuses = vec![
        LineStatus {
            id: "R-001".into(),
            badge: Badge::Broken,
            reasons: vec![Reason::TestFailed],
            tests: vec![],
        },
        LineStatus {
            id: "R-002".into(),
            badge: Badge::Unproven,
            reasons: vec![Reason::BoundaryGap],
            tests: vec![],
        },
        LineStatus {
            id: "R-003".into(),
            badge: Badge::Proven,
            reasons: vec![],
            tests: vec![],
        },
        LineStatus {
            id: "R-004".into(),
            badge: Badge::Qualified,
            reasons: vec![Reason::PrerequisiteUnproven {
                path: vec!["R-004".into(), "R-005".into()],
            }],
            tests: vec![],
        },
        LineStatus {
            id: "R-005".into(),
            badge: Badge::Unproven,
            reasons: vec![Reason::BoundaryGap],
            tests: vec![],
        },
    ];
    apply_qualified(&mut statuses, &graph);
    let reports = report::impacts(&graph, &statuses);
    let markdown = report::markdown(&reports);
    let json = serde_json::to_value(&reports).unwrap();
    let schema = dependency_report_schema("living-spec-dependency-report-v1.schema.json");
    assert!(
        validates_dependency_report(&schema, &json, &schema),
        "serialized dependency reports with unproven and qualified prerequisites must satisfy the published schema"
    );
    // These controls exercise both the original undeclared-field defect and
    // the nullable prerequisite fields in the actual published schema.
    let mut old_schema = schema.clone();
    old_schema["items"]["oneOf"][0]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("unproven_prerequisites");
    assert!(!validates_dependency_report(
        &old_schema,
        &json,
        &old_schema
    ));
    let mut malformed = json.clone();
    malformed[0]["qualified_prerequisites"][0]["boundary_reference"] = 42.into();
    assert!(!validates_dependency_report(&schema, &malformed, &schema));

    assert!(markdown.contains("R-003 → R-001 (broken"));
    assert!(markdown.contains("- Unproven: R-002"));
    assert!(markdown.contains("- Qualified: R-004"));
    assert!(markdown.contains("- Unproven: R-005"));
    assert!(markdown.contains("boundary_gap"));
    assert!(markdown.contains("annotations.json#/requirements/R-002/bounds"));
    assert_eq!(json[0]["path"], serde_json::json!(["R-003", "R-001"]));
    assert_eq!(json[0]["unproven_prerequisites"][0]["id"], "R-002");
    assert_eq!(json[0]["unproven_prerequisites"][0]["badge"], "unproven");
    assert_eq!(
        json[0]["unproven_prerequisites"][0]["boundary_reference"],
        "annotations.json#/requirements/R-002/bounds"
    );
    assert_eq!(json[0]["unproven_prerequisites"][1]["id"], "R-005");
    assert_eq!(json[0]["qualified_prerequisites"][0]["id"], "R-004");
    assert_eq!(json[0]["broken_prerequisites"][0]["id"], "R-001");
}

#[test]
fn finding_drill2_nearest_prerequisite_tie_uses_first_hop_then_excludes_self() {
    let graph = DepGraph {
        edges: BTreeMap::from([
            ("A".into(), vec!["B".into(), "C".into()]),
            ("B".into(), vec!["Z".into()]),
            ("C".into(), vec!["D".into()]),
            ("D".into(), vec![]),
            ("Z".into(), vec![]),
        ]),
        texts: BTreeMap::new(),
    };
    // D sorts before Z as a terminal, but the lexicographically first path
    // from A starts with B, so terminal iteration order must not decide ties.
    let statuses = BTreeMap::from([
        ("A".into(), Badge::Unproven),
        ("D".into(), Badge::Unproven),
        ("Z".into(), Badge::Broken),
    ]);

    assert_eq!(
        nearest_non_proven("A", &graph, &statuses),
        Some((vec!["A".into(), "B".into(), "Z".into()], Badge::Broken))
    );
}

#[test]
fn finding_07_deps_2_fifty_thousand_proven_chain_finishes_without_qualification() {
    const NODES: usize = 50_000;
    let ids: Vec<_> = (0..NODES).map(|index| format!("R-{index:05}")).collect();
    let mut edges: BTreeMap<_, _> = ids.iter().map(|id| (id.clone(), Vec::new())).collect();
    for pair in ids.windows(2) {
        edges.get_mut(&pair[0]).unwrap().push(pair[1].clone());
    }
    let graph = DepGraph {
        edges,
        texts: BTreeMap::new(),
    };
    let mut statuses: Vec<_> = ids
        .into_iter()
        .map(|id| LineStatus {
            id,
            badge: Badge::Proven,
            reasons: vec![],
            tests: vec![],
        })
        .collect();

    apply_qualified(&mut statuses, &graph);

    assert!(statuses.iter().all(|status| status.badge == Badge::Proven));
    assert!(statuses.iter().all(|status| status.reasons.is_empty()));
}
