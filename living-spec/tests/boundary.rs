use living_spec::{
    boundary::{
        coverage::{
            boundary_gaps, coverage, gaps_for_annotations, CaseLink, CaseStatus, CoverageMap,
            GapReason,
        },
        gen::{generate, Example},
        model::{Bound, BoundaryError, Unit},
        parse_bounds, resolve_bounds,
    },
    core::{
        annotations::{Audit, AuditVerdict, Link, RequirementAnnotations},
        badge::{score_line_at_revision, Badge, Reason},
        test_identity::{Fingerprints, TestIdentity, TestOutcome, TestStatus},
    },
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs};

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
    serde_json::from_str(
        &fs::read_to_string(format!("tests/fixtures/boundary/requirements/{n:02}.json")).unwrap(),
    )
    .unwrap()
}

#[test]
fn delay_boundary_has_adjacent_cases_around_thirty_seconds() {
    let bounds = parse_bounds("Delay is at most 30 s.").unwrap();
    let cases = generate(&bounds[0]).unwrap();
    assert_eq!(cases.len(), 3);
    assert_eq!(cases[0].value.parse::<i64>().unwrap(), 30 * 1000 - 1);
    assert_eq!(cases[1].value.parse::<i64>().unwrap(), 30 * 1000);
    assert_eq!(cases[2].value.parse::<i64>().unwrap(), 30 * 1000 + 1);
    assert_eq!(cases[0].unit, Some(Unit::Ms));
    assert_eq!(cases[1].unit, Some(Unit::Ms));
    assert_eq!(cases[2].unit, Some(Unit::Ms));
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

#[test]
fn boundary_reference_set() {
    let files = fs::read_dir("tests/fixtures/boundary/requirements")
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(files.len(), 30);
    let mut mix = BTreeMap::<String, usize>::new();
    let mut rejections = Vec::new();
    let mut ranges = 0;
    let annotation_schema: Value = serde_json::from_str(include_str!(
        "../../contracts/living-spec-annotations-v1.schema.json"
    ))
    .unwrap();
    for n in 1..=30 {
        let f = fixture(n);
        *mix.entry(f.category.clone()).or_default() += 1;
        match f.parse_result.as_str() {
            "parsed" => assert_eq!(
                parse_bounds(&f.text).unwrap(),
                f.expected_bounds,
                "fixture {n}"
            ),
            "needs_annotation" => {
                assert!(
                    matches!(
                        parse_bounds(&f.text),
                        Err(BoundaryError::NeedsAnnotation(_))
                    ),
                    "fixture {n}"
                );
                rejections.push(f.rejection_form.clone().unwrap());
            }
            _ => panic!("unknown parse expectation"),
        }
        let bounds = resolve_bounds(&f.text, f.annotation.as_deref()).unwrap();
        assert_eq!(bounds, f.expected_bounds, "fixture {n}");
        let generated: Vec<_> = bounds.iter().map(|b| generate(b).unwrap()).collect();
        assert_eq!(generated, f.expected_cases, "fixture {n}");
        ranges += generated
            .iter()
            .filter(|cs| cs.iter().any(|c| c.position == "edge-lo-at"))
            .count();
        let annotations = json!({"schema":"living-spec.annotations/v1","requirements":{&f.requirement: {"bounds":bounds,"boundary_links":f.links}}});
        assert!(
            validate(&annotation_schema, &annotations, &annotation_schema),
            "annotation schema {n}"
        );
        let result = coverage(&f.requirement, &bounds, &f.links, &f.outcomes);
        if f.expected_error.as_deref() == Some("unit_contradiction") {
            assert!(matches!(
                result,
                Err(BoundaryError::UnitContradiction { .. })
            ));
            continue;
        }
        let map = result.unwrap();
        let statuses: Vec<Vec<_>> = map
            .bounds
            .iter()
            .map(|b| b.cases.iter().map(|c| c.status.clone()).collect())
            .collect();
        assert_eq!(statuses, f.expected_statuses, "fixture {n}");
        for (bound, expected) in map.bounds.iter().zip(&f.expected_cases) {
            assert_eq!(bound.cases.len(), expected.len());
            for case in &bound.cases {
                assert!(case.test_id.is_some() || case.status == CaseStatus::Untested);
                if case.status == CaseStatus::Untested {
                    assert!(case.reason.is_some());
                }
            }
        }
        let json: Value = serde_json::from_str(&map.to_json().unwrap()).unwrap();
        assert!(validate(&schema(), &json, &schema()), "coverage schema {n}");
        assert_eq!(serde_json::from_value::<CoverageMap>(json).unwrap(), map);
    }
    assert!(
        mix["time"] >= 10
            && mix["size"] >= 6
            && mix["count"] >= 8
            && mix["domain"] >= 3
            && ranges >= 3
    );
    for form in [
        "fast",
        "percentile",
        "normally",
        "negation",
        "relational",
        "disjunction",
        "date",
        "locale",
        "ambiguous-between",
        "compound",
    ] {
        assert!(rejections.iter().any(|s| s == form));
    }
    println!("boundary_reference_set: 30/30");
}

#[test]
fn unit_contradiction_rejected() {
    let f = fixture(2);
    assert!(matches!(
        coverage(&f.requirement, &f.expected_bounds, &f.links, &f.outcomes),
        Err(BoundaryError::UnitContradiction {
            requirement_unit: Unit::S,
            test_unit: Unit::Ms,
            ..
        })
    ));
    let mut links = f.links;
    links[0].unit = Some(Unit::B);
    assert!(matches!(
        coverage(&f.requirement, &f.expected_bounds, &links, &f.outcomes),
        Err(BoundaryError::UnitContradiction {
            test_unit: Unit::B,
            ..
        })
    ));
    let size = fixture(12);
    assert!(matches!(
        coverage(
            &size.requirement,
            &size.expected_bounds,
            &size.links,
            &size.outcomes
        ),
        Err(BoundaryError::UnitContradiction {
            requirement_unit: Unit::MB,
            test_unit: Unit::MiB,
            ..
        })
    ));
}

#[test]
fn happy_path_only_is_not_proven() {
    let f = fixture(1);
    let map = coverage(&f.requirement, &f.expected_bounds, &f.links, &f.outcomes).unwrap();
    assert_eq!(
        map.bounds[0]
            .cases
            .iter()
            .map(|c| c.position.as_str())
            .collect::<Vec<_>>(),
        ["below", "at", "above"]
    );
    assert!(map.bounds[0]
        .cases
        .iter()
        .all(|c| c.status == CaseStatus::Untested));
    let revision =
        living_spec::core::spec::Spec::parse(&format!("- [{}] {}\n", f.requirement, f.text))
            .unwrap()
            .lines[0]
            .revision
            .clone();
    let a = RequirementAnnotations {
        bounds: f
            .expected_bounds
            .iter()
            .map(|b| serde_json::to_value(b).unwrap())
            .collect(),
        links: vec![Link {
            test_id: f.outcomes[0].test_id.clone(),
            fingerprint: "sha256:synthetic".into(),
            rename_hint: Some("sha256:synthetic-body".into()),
            revision: revision.clone(),
            explanation: "assert expected rejection or acceptance".into(),
            proposed_by: "human".into(),
            extra: BTreeMap::new(),
            audit: Audit {
                verdict: AuditVerdict::Accepted,
                revision: revision.clone(),
                notes: vec![],
                extra: BTreeMap::new(),
            },
        }],
        ..Default::default()
    };
    let gaps = gaps_for_annotations(
        &BTreeMap::from([(f.requirement.clone(), a.clone())]),
        &f.outcomes,
    )
    .unwrap();
    assert_eq!(gaps, boundary_gaps(&[map]));
    let identities: Fingerprints = BTreeMap::from([(
        f.outcomes[0].test_id.clone(),
        TestIdentity {
            fingerprint: "sha256:synthetic".into(),
            rename_hint: Some("sha256:synthetic-body".into()),
        },
    )]);
    let scored = score_line_at_revision(
        &f.requirement,
        &revision,
        &a,
        &f.outcomes,
        &identities,
        &gaps,
    );
    assert_eq!(scored.badge, Badge::Unproven);
    assert_eq!(scored.reasons, [Reason::BoundaryGap]);
}

#[test]
fn markdown_names_every_untested_boundary_and_schema_rejects_unknowns() {
    let f = fixture(21);
    let map = coverage(&f.requirement, &f.expected_bounds, &[], &[]).unwrap();
    let md = map.to_markdown();
    assert_eq!(md.lines().count(), 8);
    for case in &map.bounds[0].cases {
        assert!(md.contains(&format!("| Batch | {} |", case.position)));
    }
    let mut json = serde_json::to_value(&map).unwrap();
    assert!(validate(&schema(), &json, &schema()));
    json["bounds"][0]["cases"][0]["hidden"] = json!(true);
    assert!(!validate(&schema(), &json, &schema()));
    assert!(serde_json::from_value::<CoverageMap>(json).is_err());
    let mut invalid = serde_json::to_value(&map).unwrap();
    invalid["bounds"][0]["cases"][0]["status"] = json!("tested");
    assert!(!validate(&schema(), &invalid, &schema()));
}

#[test]
fn unknown_units_and_typed_input_binding() {
    let f = fixture(3);
    let map = coverage(&f.requirement, &f.expected_bounds, &f.links, &f.outcomes).unwrap();
    assert!(map.bounds[0]
        .cases
        .iter()
        .all(|c| c.reason == Some(GapReason::UnitUnknown)));
    let mut f = fixture(4);
    let map = coverage(&f.requirement, &f.expected_bounds, &f.links, &f.outcomes).unwrap();
    assert!(boundary_gaps(&[map]).is_empty());
    f.links[0].test_body = Some("Duration::from_millis(3)".into());
    let map = coverage(&f.requirement, &f.expected_bounds, &f.links, &f.outcomes).unwrap();
    assert_eq!(
        map.bounds[0].cases[0].reason,
        Some(GapReason::ValueMismatch)
    );
    for body in [
        "let duration_ms = 3;",
        "let value = 3 * 1000;",
        "// Duration::from_millis(119999)",
        "let s = \"Duration::from_millis(119999)\";",
        "Duration::from_millis(119999); Duration::from_secs(9)",
    ] {
        f.links[0].test_body = Some(body.into());
        assert_eq!(
            coverage(&f.requirement, &f.expected_bounds, &f.links, &f.outcomes)
                .unwrap()
                .bounds[0]
                .cases[0]
                .reason,
            Some(GapReason::UnitUnknown)
        );
    }
    f.links[0].value = "119999".into();
    f.links[0].test_body = Some("timedelta(milliseconds=119999)".into());
    assert_eq!(
        coverage(&f.requirement, &f.expected_bounds, &f.links, &f.outcomes)
            .unwrap()
            .bounds[0]
            .cases[0]
            .status,
        CaseStatus::TestedPass
    );
    let f = fixture(1);
    for body in ["Duration::from_secs(30)", "timedelta(seconds=30)"] {
        let link = CaseLink {
            quantity: "Delay".into(),
            position: "at".into(),
            value: "30".into(),
            unit: None,
            test_id: "typed-seconds".into(),
            test_body: Some(body.into()),
        };
        let outcomes = [TestOutcome {
            test_id: "typed-seconds".into(),
            status: TestStatus::Passed,
            duration_ms: None,
            failure_message: None,
        }];
        assert_eq!(
            coverage(&f.requirement, &f.expected_bounds, &[link], &outcomes)
                .unwrap()
                .bounds[0]
                .cases[1]
                .status,
            CaseStatus::TestedPass
        );
    }
}

#[test]
fn skipped_missing_duplicate_and_wrong_value_stay_visible() {
    let mut f = fixture(18);
    f.outcomes[0].status = TestStatus::Skipped;
    f.outcomes.pop();
    let map = coverage(&f.requirement, &f.expected_bounds, &f.links, &f.outcomes).unwrap();
    assert_eq!(map.bounds[0].cases[0].reason, Some(GapReason::NotExecuted));
    assert_eq!(map.bounds[0].cases[2].reason, Some(GapReason::NotExecuted));
    f.links[1].value = "12".into();
    assert_eq!(
        coverage(&f.requirement, &f.expected_bounds, &f.links, &f.outcomes)
            .unwrap()
            .bounds[0]
            .cases[1]
            .reason,
        Some(GapReason::ValueMismatch)
    );
    f.links.push(f.links[0].clone());
    assert!(coverage(&f.requirement, &f.expected_bounds, &f.links, &f.outcomes).is_err());
}

#[test]
fn annotation_wins_and_invalid_bounds_fail() {
    let f = fixture(1);
    assert_eq!(
        resolve_bounds("Delay is at most 999 ms", Some(&f.expected_bounds)).unwrap(),
        f.expected_bounds
    );
    assert!(resolve_bounds("fast", Some(&[])).unwrap().is_empty());
    for source in [
        "Delay is at most 1.000 ms",
        "Delay is at most 1.5 ms",
        "Delay is at most 30 ms and at least 2 ms",
        "Delay is not at most 30 ms",
        "Delay is between 1 and 3 ms",
        "Delay is at most 3 ms/s",
    ] {
        assert!(matches!(
            parse_bounds(source),
            Err(BoundaryError::NeedsAnnotation(_))
        ));
    }
    let mut b = serde_json::to_value(&f.expected_bounds[0]).unwrap();
    b["kind"]["step"]["value"] = json!("0");
    assert!(generate(&serde_json::from_value(b).unwrap()).is_err());
    assert!("1.0000000001"
        .parse::<living_spec::boundary::model::Decimal>()
        .is_err());
    assert!("99999999999999999999999999999999999999"
        .parse::<living_spec::boundary::model::Decimal>()
        .is_err());
}

#[test]
fn bounds_source_schema_requires_an_inferred_revision() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../contracts/living-spec-annotations-v1.schema.json"
    ))
    .unwrap();
    let line = living_spec::core::spec::Spec::parse("- [R-001] Delay is at most 10 ms.")
        .unwrap()
        .lines
        .remove(0);
    for source in [
        json!({"origin":"authored"}),
        json!({"origin":"inferred","revision":line.revision}),
    ] {
        let annotation = json!({"schema":"living-spec.annotations/v1","requirements":{"R-001":{
            "bounds":parse_bounds(&line.text).unwrap(), "bounds_source":source
        }}});
        assert!(validate(&schema, &annotation, &schema));
    }
    for source in [
        json!({"origin":"inferred"}),
        json!({"origin":"inferred","revision":""}),
        json!({"origin":"unknown"}),
    ] {
        let annotation = json!({"schema":"living-spec.annotations/v1","requirements":{"R-001":{
            "bounds":[], "bounds_source":source
        }}});
        assert!(!validate(&schema, &annotation, &schema));
    }
}
