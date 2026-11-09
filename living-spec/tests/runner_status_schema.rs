use living_spec::{
    adapters::parse_libtest,
    core::{
        annotations::Annotations,
        badge::{Badge, Reason},
        providers::{BoundaryGapProvider, NoBoundaryEvidence},
        quantifier::Report as QuantifierReport,
        spec::Spec,
        test_identity::{load_fingerprints, Fingerprints, TestOutcome},
        views::{BoundaryReport, QuantifierReport as ViewQuantifierReport},
    },
    runner::rescore::{
        rescore_with_context, NoQualification, RescoreContext, Result, StatusDocument,
    },
};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path, sync::LazyLock};

static CONTRACTS: LazyLock<std::collections::BTreeMap<&str, Value>> = LazyLock::new(|| {
    [
        (
            "living-spec-boundary-coverage-v1.schema.json",
            include_str!("../../contracts/living-spec-boundary-coverage-v1.schema.json"),
        ),
        (
            "living-spec-quantifier-edition-v1.schema.json",
            include_str!("../../contracts/living-spec-quantifier-edition-v1.schema.json"),
        ),
        (
            "living-spec-annotations-v1.schema.json",
            include_str!("../../contracts/living-spec-annotations-v1.schema.json"),
        ),
    ]
    .into_iter()
    .map(|(name, text)| (name, serde_json::from_str(text).unwrap()))
    .collect()
});

// Validate the keywords used by these checked-in contracts. Unknown keywords
// and references fail closed; this helper never fetches remote schemas.
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
        "minimum",
        "minLength",
        "pattern",
        "description",
        "title",
        "allOf",
        "not",
        "if",
        "then",
        "maximum",
        "maxLength",
        "minItems",
        "maxItems",
        "uniqueItems",
        "contains",
        "minContains",
        "propertyNames",
    ];
    assert!(
        schema
            .as_object()
            .unwrap()
            .keys()
            .all(|key| supported.contains(&key.as_str())),
        "unsupported schema keyword: {schema}"
    );
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let (file, pointer) = reference.split_once('#').expect("reference has a fragment");
        let root = if file.is_empty() {
            root
        } else {
            &CONTRACTS[file]
        };
        return validate(
            root.pointer(pointer).expect("known contract reference"),
            value,
            root,
        );
    }
    if let Some(options) = schema.get("allOf").and_then(Value::as_array) {
        if !options.iter().all(|s| validate(s, value, root)) {
            return false;
        }
    }
    if schema.get("not").is_some_and(|s| validate(s, value, root)) {
        return false;
    }
    if let Some(condition) = schema.get("if") {
        if validate(condition, value, root)
            && schema
                .get("then")
                .is_some_and(|s| !validate(s, value, root))
        {
            return false;
        }
    }
    if let Some(options) = schema.get("oneOf").and_then(Value::as_array) {
        if options.iter().filter(|s| validate(s, value, root)).count() != 1 {
            return false;
        }
    }
    if schema.get("const").is_some_and(|v| v != value) {
        return false;
    }
    if schema
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|values| !values.contains(value))
    {
        return false;
    }
    if let Some(kind) = schema.get("type") {
        let kinds = kind
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![kind.clone()]);
        if !kinds.iter().any(|kind| match kind.as_str().unwrap() {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            other => panic!("unsupported schema type {other}"),
        }) {
            return false;
        }
    }
    let integer = |v: &Value| {
        v.as_i64()
            .map(i128::from)
            .or_else(|| v.as_u64().map(i128::from))
    };
    if let Some(number) = integer(value) {
        if schema
            .get("minimum")
            .and_then(integer)
            .is_some_and(|min| number < min)
            || schema
                .get("maximum")
                .and_then(integer)
                .is_some_and(|max| number > max)
        {
            return false;
        }
    }
    if let Some(text) = value.as_str() {
        let len = text.chars().count() as u64;
        if schema
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|min| len < min)
            || schema
                .get("maxLength")
                .and_then(Value::as_u64)
                .is_some_and(|max| len > max)
        {
            return false;
        }
    }
    if let (Some(pattern), Some(text)) = (
        schema.get("pattern").and_then(Value::as_str),
        value.as_str(),
    ) {
        if !regex::Regex::new(pattern).unwrap().is_match(text) {
            return false;
        }
    }
    if let Some(object) = value.as_object() {
        if schema
            .get("required")
            .and_then(Value::as_array)
            .is_some_and(|keys| {
                keys.iter()
                    .any(|k| !object.contains_key(k.as_str().unwrap()))
            })
        {
            return false;
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        for (key, child) in object {
            if schema
                .get("propertyNames")
                .is_some_and(|s| !validate(s, &Value::String(key.clone()), root))
            {
                return false;
            }
            if let Some(child_schema) = properties.and_then(|p| p.get(key)) {
                if !validate(child_schema, child, root) {
                    return false;
                }
            } else if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                return false;
            } else if let Some(s) = schema.get("additionalProperties").filter(|s| s.is_object()) {
                if !validate(s, child, root) {
                    return false;
                }
            }
        }
    }
    if let Some(array) = value.as_array() {
        let len = array.len() as u64;
        if schema
            .get("minItems")
            .and_then(Value::as_u64)
            .is_some_and(|min| len < min)
            || schema
                .get("maxItems")
                .and_then(Value::as_u64)
                .is_some_and(|max| len > max)
        {
            return false;
        }
        if schema.get("uniqueItems") == Some(&Value::Bool(true))
            && array
                .iter()
                .enumerate()
                .any(|(i, item)| array[..i].contains(item))
        {
            return false;
        }
        if let Some(s) = schema.get("contains") {
            let count = array.iter().filter(|item| validate(s, item, root)).count() as u64;
            if count
                < schema
                    .get("minContains")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
            {
                return false;
            }
        }
        if let Some(item_schema) = schema.get("items") {
            if array.iter().any(|item| !validate(item_schema, item, root)) {
                return false;
            }
        }
    }
    true
}

struct WithBoundaryReport;
impl BoundaryGapProvider for WithBoundaryReport {
    fn gaps(&self, _: &Spec, _: &Annotations, _: &[TestOutcome]) -> Result<BTreeSet<String>> {
        Ok(BTreeSet::new())
    }
    fn reports(&self, _: &Spec, _: &Annotations, _: &[TestOutcome]) -> Result<Vec<BoundaryReport>> {
        let bounds = vec![serde_json::from_value(serde_json::json!({
            "quantity": "delay", "kind": {"type":"numeric", "value":"30", "unit":"ms", "op":{"type":"le"}, "step":{"value":"1", "unit":"ms"}, "integer":true}
        }))?];
        Ok(vec![living_spec::core::boundary::coverage::coverage(
            "R-001",
            &bounds,
            &[],
            &[],
        )?])
    }
}

struct Qualified(Reason);
impl living_spec::runner::rescore::QualifiedProvider for Qualified {
    fn qualify(
        &self,
        _: &Spec,
        _: &Annotations,
        statuses: &mut [living_spec::core::badge::LineStatus],
    ) -> Result<()> {
        statuses[0].badge = Badge::Qualified;
        statuses[0].reasons = vec![self.0.clone()];
        Ok(())
    }
}

fn serialized_status(
    boundary: &dyn BoundaryGapProvider,
    qualified: &dyn living_spec::runner::rescore::QualifiedProvider,
    quantifier: &[ViewQuantifierReport],
) -> Value {
    let dir = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runner/spec");
    for file in ["spec.md", "annotations.json"] {
        fs::copy(fixture.join(file), dir.path().join(file)).unwrap();
    }
    let outcomes = parse_libtest(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runner/before.txt"),
        )
        .unwrap(),
    )
    .unwrap();
    let identities: Fingerprints = load_fingerprints(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runner/fingerprints.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let quantifier = quantifier.to_vec();
    let quantifier_fn = move |_: &Spec, _: &Annotations| -> Result<Vec<QuantifierReport>> {
        Ok(quantifier.clone())
    };
    let report = rescore_with_context(
        dir.path(),
        &outcomes,
        &identities,
        "fixture-commit",
        "schema regression",
        RescoreContext {
            boundary,
            qualified,
            reauditor: None,
            quantifier_reports: &quantifier_fn,
        },
    )
    .unwrap();
    let bytes = fs::read(dir.path().join("status.json")).unwrap();
    let decoded: StatusDocument = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded.statuses, report.statuses);
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn finding_3_every_persisted_status_document_matches_its_schema() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../contracts/living-spec-status-v1.schema.json"
    ))
    .unwrap();
    let ordinary = serialized_status(&NoBoundaryEvidence, &NoQualification, &[]);
    let boundary = serialized_status(&WithBoundaryReport, &NoQualification, &[]);
    let quantifier: QuantifierReport = serde_json::from_value(serde_json::json!({
        "id":"Q-001", "text":"The output is nonnegative", "quantifier": {
            "kind":"existential", "domain":{"type":"int-range", "lo":0, "hi":0},
            "subject":"clamp", "predicate":{"version":1, "expression":{"op":"ge", "left":{"kind":"field", "source":"output", "path":[]}, "right":{"kind":"literal", "value":{"type":"int", "value":0}}}}
        }, "coverage":{"mode":"exhaustive", "tested_cases":1, "distinct_tested":1, "finite_total":1, "seed":null, "untested_remainder":"none"},
        "result":"witness-found",
        "observations":[{"input":{"type":"int","value":0},"output":{"type":"int","value":0},"satisfied":true}],
        "failing_subdomains":[],
        "witness":{"input":{"type":"int","value":0},"output":{"type":"int","value":0},"satisfied":true}
    })).unwrap();
    let quantified = serialized_status(&NoBoundaryEvidence, &NoQualification, &[quantifier]);
    for (label, document) in [
        ("passing", ordinary.clone()),
        ("boundary", boundary.clone()),
        ("quantifier", quantified.clone()),
    ] {
        assert!(
            validate(&schema, &document, &schema),
            "{label} document failed schema: {document}"
        );
    }
    for reason in [
        Reason::PrerequisiteBroken {
            path: vec!["R-001".into(), "R-002".into()],
        },
        Reason::PrerequisiteUnproven {
            path: vec!["R-001".into(), "R-002".into()],
        },
        Reason::PrerequisiteQualified {
            path: vec!["R-001".into(), "R-002".into()],
        },
        Reason::DependencyCycle {
            path: std::sync::Arc::new(vec!["R-001".into(), "R-002".into(), "R-001".into()]),
        },
    ] {
        let document = serialized_status(&NoBoundaryEvidence, &Qualified(reason), &[]);
        assert!(
            validate(&schema, &document, &schema),
            "qualified document: {document}"
        );
    }
    // New persisted reports require input bindings. Legacy documents still load
    // so the file view can label them stale rather than inventing a verdict.
    let mut unbound = ordinary.clone();
    unbound.as_object_mut().unwrap().remove("inputs");
    assert!(!validate(&schema, &unbound, &schema));
    let legacy: StatusDocument = serde_json::from_value(unbound).unwrap();
    assert!(legacy.inputs.is_none());
    // Older saved documents omit report arrays; the Rust reader defaults them.
    let mut older = ordinary.clone();
    older.as_object_mut().unwrap().remove("quantifier_reports");
    older.as_object_mut().unwrap().remove("boundary_reports");
    assert!(validate(&schema, &older, &schema));
    let mut unknown = ordinary.clone();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(
        !validate(&schema, &unknown, &schema),
        "unknown root properties must be rejected"
    );
    let mut wrong_value = quantified.clone();
    wrong_value["quantifier_reports"][0]["observations"][0]["input"]["value"] =
        serde_json::json!(true);
    assert!(
        !validate(&schema, &wrong_value, &schema),
        "typed int values reject booleans"
    );
    let mut wrong_report = quantified;
    wrong_report["quantifier_reports"] = serde_json::json!([{"id": 4}]);
    assert!(
        !validate(&schema, &wrong_report, &schema),
        "malformed quantifier report must be rejected"
    );
    let mut wrong_case = boundary.clone();
    wrong_case["boundary_reports"][0]["bounds"][0]["cases"][0]["expected"] =
        serde_json::json!("maybe");
    assert!(
        !validate(&schema, &wrong_case, &schema),
        "boundary expectation is typed"
    );
    let mut wrong_boundary = boundary;
    wrong_boundary["boundary_reports"] = serde_json::json!(["not a report"]);
    assert!(
        !validate(&schema, &wrong_boundary, &schema),
        "wrong boundary report type must be rejected"
    );
    assert!(
        std::panic::catch_unwind(|| validate(
            &serde_json::json!({"unsupported-keyword":true}),
            &serde_json::json!(null),
            &schema
        ))
        .is_err(),
        "unknown schema keywords must fail closed"
    );
}
