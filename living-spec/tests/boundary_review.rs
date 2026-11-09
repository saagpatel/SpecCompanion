use living_spec::{
    boundary::{
        coverage::{gaps_for_annotations, BoundaryEvidence, CaseStatus},
        parse_bounds,
        parser::{resolve_requirement, BoundsSource},
    },
    core::{
        annotations::{Annotations, RequirementAnnotations},
        badge::{score_line_at_revision, Badge, Reason},
        providers::BoundaryGapProvider,
        spec::Spec,
        test_identity::{TestIdentity, TestOutcome, TestStatus},
    },
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path, process::Command};

#[test]
fn integer_step_in_declared_unit_kb_accepted() {
    for (unit, step_unit, step, values) in [
        ("KB", "KB", "1", ["9000", "10000", "11000"]),
        ("KB", "B", "1000", ["9000", "10000", "11000"]),
        ("s", "ms", "1000", ["9000", "10000", "11000"]),
        ("ms", "s", "0.001", ["9", "10", "11"]),
    ] {
        let bound = serde_json::from_value(json!({"quantity":"limit", "kind": {
            "type":"numeric", "value":"10", "unit":unit, "op":{"type":"le"},
            "step":{"value":step, "unit":step_unit}, "integer":true
        }}))
        .unwrap();
        let cases = living_spec::core::boundary::generate(&bound).unwrap();
        assert_eq!(
            cases.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(),
            values
        );
    }
}

#[test]
fn fractional_step_in_declared_unit_rejected() {
    for (unit, step_unit, step) in [
        ("KB", "KB", "0.001"),
        ("KB", "B", "1"),
        ("s", "ms", "1"),
        ("ms", "s", "0.000001"),
    ] {
        let bound = serde_json::from_value(json!({"quantity":"limit", "kind": {
            "type":"numeric", "value":"10", "unit":unit, "op":{"type":"le"},
            "step":{"value":step, "unit":step_unit}, "integer":true
        }}))
        .unwrap();
        assert!(living_spec::core::boundary::generate(&bound).is_err());
    }
}

fn cli(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .args(["boundary", args[0]])
        .arg(dir)
        .args(&args[1..])
        .output()
        .unwrap()
}

#[test]
fn generated_bounds_become_permanent_authored_overrides_after_a_requirement_edit() {
    // Review reproduction, plus an operator-only edit: numbers alone cannot
    // establish that the old passing assertions cover new acceptance semantics.
    for (edited, values) in [
        ("Delay is at most 5 ms.", ["4", "5", "6"]),
        ("Delay is less than 10 ms.", ["9", "10", "11"]),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let spec_path = dir.path().join("spec.md");
        let annotations_path = dir.path().join("annotations.json");
        fs::write(&spec_path, "- [R-001] Delay is at most 10 ms.\n").unwrap();
        fs::write(
            &annotations_path,
            r#"{"schema":"living-spec.annotations/v1","requirements":{}}"#,
        )
        .unwrap();
        assert!(cli(dir.path(), &["gen"]).status.success());
        let mut annotations: Value =
            serde_json::from_slice(&fs::read(&annotations_path).unwrap()).unwrap();
        assert_eq!(
            annotations["requirements"]["R-001"]["bounds_source"]["origin"],
            "inferred"
        );
        let cached = Annotations::load(&annotations_path).unwrap();
        assert!(gaps_for_annotations(&cached.requirements, &[]).is_err());
        annotations["requirements"]["R-001"]["boundary_links"] = json!([
            {"quantity":"Delay","position":"below","value":"9","unit":"ms","test_id":"case_below"},
            {"quantity":"Delay","position":"at","value":"10","unit":"ms","test_id":"case_at"},
            {"quantity":"Delay","position":"above","value":"11","unit":"ms","test_id":"case_above"}
        ]);
        fs::write(&annotations_path, serde_json::to_vec(&annotations).unwrap()).unwrap();
        let receipt = dir.path().join("receipt.xml");
        fs::write(&receipt, r#"<testsuite tests="3"><testcase name="case_below"/><testcase name="case_at"/><testcase name="case_above"/></testsuite>"#).unwrap();
        let export = dir.path().join("coverage.json");
        let args = [
            "coverage",
            "--results",
            receipt.to_str().unwrap(),
            "--format",
            "junit",
            "--json",
            export.to_str().unwrap(),
        ];
        assert!(cli(dir.path(), &args).status.success());
        fs::write(&spec_path, format!("- [R-001] {edited}\n")).unwrap();
        // No generation prerequisite: coverage must already use the current line.
        assert_eq!(cli(dir.path(), &args).status.code(), Some(3));
        assert!(cli(dir.path(), &["gen"]).status.success());
        assert_eq!(cli(dir.path(), &args).status.code(), Some(3));
        let map: Value = serde_json::from_slice(&fs::read(export).unwrap()).unwrap();
        let cases = map[0]["bounds"][0]["cases"].as_array().unwrap();
        assert_eq!(
            cases
                .iter()
                .map(|c| c["value"].as_str().unwrap())
                .collect::<Vec<_>>(),
            values
        );
        assert!(cases.iter().all(|c| c["status"] == "untested"));
        assert_eq!(
            cases[1]["expected"],
            if edited.contains("less than") {
                "rejected"
            } else {
                "accepted"
            }
        );
        let saved = Annotations::load(&annotations_path).unwrap();
        assert_eq!(
            saved.requirements["R-001"].extra["boundary_links"],
            json!([])
        );
        assert_eq!(
            saved.requirements["R-001"].extra["bounds_source"]["revision"],
            Spec::parse(&format!("- [R-001] {edited}\n")).unwrap().lines[0].revision
        );
        let before = fs::read(&annotations_path).unwrap();
        assert!(cli(dir.path(), &["gen"]).status.success());
        assert_eq!(before, fs::read(&annotations_path).unwrap());
    }
}

#[test]
fn runner_provider_silently_omits_boundaries_that_the_cli_discovers() {
    let spec = Spec::parse("- [R-001] Delay is at most 30 s.\n").unwrap();
    let revision = &spec.lines[0].revision;
    let test_id = "libtest:sample:tests/cases.rs:happy";
    let mut annotations: Annotations = serde_json::from_value(json!({
        "schema":"living-spec.annotations/v1", "requirements":{"R-001":{"links":[{
            "test_id":test_id, "fingerprint":"sha256:synthetic", "revision":revision,
            "explanation":"15-second happy path", "proposed_by":"human",
            "audit":{"verdict":"accepted", "revision":revision, "notes":[]}
        }]}}
    }))
    .unwrap();
    let outcomes = [TestOutcome {
        test_id: test_id.into(),
        status: TestStatus::Passed,
        duration_ms: None,
        failure_message: None,
    }];
    let gaps = BoundaryEvidence
        .gaps(&spec, &annotations, &outcomes)
        .unwrap();
    assert!(gaps.contains("R-001"));
    let reports = BoundaryEvidence
        .reports(&spec, &annotations, &outcomes)
        .unwrap();
    let cases = &reports[0].bounds[0].cases;
    assert_eq!(
        cases.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(),
        ["29999", "30000", "30001"]
    );
    assert!(cases.iter().all(|c| c.status == CaseStatus::Untested));
    let identities = BTreeMap::from([(
        test_id.into(),
        TestIdentity {
            fingerprint: "sha256:synthetic".into(),
            rename_hint: None,
        },
    )]);
    let scored = score_line_at_revision(
        "R-001",
        revision,
        &annotations.requirements["R-001"],
        &outcomes,
        &identities,
        &gaps,
    );
    assert_eq!(scored.badge, Badge::Unproven);
    assert_eq!(scored.reasons, [Reason::BoundaryGap]);
    // No annotation entry must not hide the same bound either.
    annotations.requirements.clear();
    assert!(BoundaryEvidence
        .gaps(&spec, &annotations, &outcomes)
        .unwrap()
        .contains("R-001"));
    assert_eq!(
        BoundaryEvidence
            .reports(&spec, &annotations, &outcomes)
            .unwrap(),
        reports
    );
}

#[test]
fn authored_bounds_and_empty_overrides_survive_annotation_round_trips() {
    let spec = Spec::parse("- [R-001] Delay is at most 5 ms.\n").unwrap();
    for bounds in [
        json!([]),
        serde_json::to_value(parse_bounds("Delay is at most 10 ms.").unwrap()).unwrap(),
    ] {
        let a: RequirementAnnotations = serde_json::from_value(json!({"bounds":bounds})).unwrap();
        let saved = serde_json::to_value(&a).unwrap();
        assert_eq!(saved["bounds"], bounds);
        let reloaded = serde_json::from_value(saved).unwrap();
        let resolved = resolve_requirement(&spec.lines[0], Some(&reloaded)).unwrap();
        assert_eq!(resolved.source, BoundsSource::Authored);
        assert_eq!(serde_json::to_value(resolved.bounds).unwrap(), bounds);
        let annotations: Annotations = serde_json::from_value(json!({"schema":"living-spec.annotations/v1","requirements":{"R-001":{"bounds":bounds}}})).unwrap();
        assert_eq!(
            BoundaryEvidence
                .gaps(&spec, &annotations, &[])
                .unwrap()
                .is_empty(),
            bounds.as_array().unwrap().is_empty()
        );
    }
    let absent: RequirementAnnotations = serde_json::from_value(json!({})).unwrap();
    assert!(serde_json::to_value(&absent)
        .unwrap()
        .get("bounds")
        .is_none());
    assert!(!resolve_requirement(&spec.lines[0], Some(&absent))
        .unwrap()
        .bounds
        .is_empty());
    assert!(serde_json::from_value::<RequirementAnnotations>(json!({"bounds":null})).is_err());
}

#[test]
fn ambiguous_legacy_generation_and_unsupported_boundary_prose_are_input_errors() {
    let spec = Spec::parse("- [R-001] Delay is at most 5 ms.\n").unwrap();
    let legacy: RequirementAnnotations = serde_json::from_value(json!({
        "bounds":parse_bounds("Delay is at most 10 ms.").unwrap(), "boundary_cases":[]
    }))
    .unwrap();
    assert!(resolve_requirement(&spec.lines[0], Some(&legacy)).is_err());
    for source in [
        json!({"origin":"inferred"}),
        json!({"origin":"unknown"}),
        json!({"origin":"inferred","revision":""}),
    ] {
        let a = serde_json::from_value(json!({"bounds":parse_bounds("Delay is at most 10 ms.").unwrap(),"bounds_source":source})).unwrap();
        assert!(resolve_requirement(&spec.lines[0], Some(&a)).is_err());
    }
    let annotations =
        serde_json::from_value(json!({"schema":"living-spec.annotations/v1","requirements":{}}))
            .unwrap();
    for text in [
        "Delay is at most 5 ms unless busy.",
        "Delay is at most 1.5 ms.",
        "Delay is not at most 5 ms.",
        "Response is fast.",
    ] {
        let spec = Spec::parse(&format!("- [R-001] {text}\n")).unwrap();
        assert!(
            BoundaryEvidence.gaps(&spec, &annotations, &[]).is_err(),
            "{text}"
        );
        assert!(
            BoundaryEvidence.reports(&spec, &annotations, &[]).is_err(),
            "{text}"
        );
    }
}
