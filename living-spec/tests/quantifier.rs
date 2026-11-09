use living_spec::quantifier::{
    self,
    annotations::{Domain, Value},
    predicate::{Expr, Operand, Source},
    Outcome,
};
use regex::Regex;
use std::{collections::BTreeSet, fs, path::Path};

fn reference() -> quantifier::Loaded {
    quantifier::load(&quantifier::reference_dir()).expect("reference specs load")
}

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
        .expect("temporary outputs stay inside the worktree")
}

fn report_for(loaded: &quantifier::Loaded, id: &str) -> quantifier::Report {
    quantifier::check_claim(loaded.claims.iter().find(|claim| claim.id == id).unwrap()).unwrap()
}

#[test]
fn quantifier_specs_count() {
    let loaded = reference();
    assert_eq!(loaded.claims.len(), 30);
    assert_eq!(loaded.fixture_hashes.len(), 30);
    for claim in &loaded.claims {
        assert!(
            claim
                .quantifier
                .validate(quantifier::annotations::FINITE_CAP)
                .is_ok(),
            "{}",
            claim.id
        );
        assert!(claim.expected.is_some(), "{}", claim.id);
    }
}

#[test]
fn reference_expected_results_match_execution() {
    for claim in reference().claims {
        let expected = claim.expected.as_ref().unwrap();
        let report = quantifier::check_claim(&claim).unwrap();
        assert_eq!(report.result, expected.result, "{}", claim.id);
        assert!(
            report
                .observations
                .iter()
                .filter(|observation| !observation.satisfied)
                .count() as u64
                >= expected.min_failures,
            "{}",
            claim.id
        );
    }
}

#[test]
fn finite_exhaustive_reports_wrapping_failure_and_collapses_regions() {
    let loaded = reference();
    let abs = report_for(&loaded, "Q-011");
    assert_eq!(abs.coverage.tested_cases, 256);
    assert_eq!(abs.coverage.distinct_tested, 256);
    assert_eq!(abs.coverage.finite_total, Some(256));
    assert_eq!(
        abs.observations
            .iter()
            .filter(|o| !o.satisfied)
            .map(|o| &o.input)
            .collect::<Vec<_>>(),
        [&Value::Int(-128)]
    );
    assert_eq!(
        abs.failing_subdomains,
        [Domain::IntRange { lo: -128, hi: -128 }]
    );

    let mut product_claim = loaded
        .claims
        .iter()
        .find(|c| c.id == "Q-004")
        .unwrap()
        .clone();
    product_claim.quantifier.predicate.expression = Expr::Eq {
        left: Operand::Field {
            source: Source::Output,
            path: vec![],
        },
        right: Operand::Literal {
            value: Value::Int(999),
        },
    };
    let product = quantifier::check_claim(&product_claim).unwrap();
    assert_eq!(product.coverage.tested_cases, 25);
    assert!(product.failing_subdomains.iter().any(|domain| *domain
        == Domain::Product {
            parts: vec![
                Domain::IntRange { lo: -2, hi: 2 },
                Domain::IntRange { lo: -2, hi: 2 }
            ]
        }));

    let mut enum_claim = loaded
        .claims
        .iter()
        .find(|c| c.id == "Q-005")
        .unwrap()
        .clone();
    enum_claim.quantifier.predicate.expression = Expr::Eq {
        left: Operand::Field {
            source: Source::Output,
            path: vec![],
        },
        right: Operand::Literal {
            value: Value::String("never".into()),
        },
    };
    let enum_report = quantifier::check_claim(&enum_claim).unwrap();
    let values = match &enum_report.failing_subdomains[0] {
        Domain::Enum { values } => values,
        other => panic!("expected enum region, got {other:?}"),
    };
    assert_eq!(
        values.iter().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from([
            Value::String(String::new()),
            Value::String("a".into()),
            Value::String("é".into())
        ])
    );
}

#[test]
fn sampled_coverage_is_reproducible_distinct_and_explicit_about_remainder() {
    let loaded = reference();
    let claim = loaded.claims.iter().find(|c| c.id == "Q-013").unwrap();
    let first = quantifier::check_claim(claim).unwrap();
    let second = quantifier::check_claim(claim).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.coverage.tested_cases, 12);
    assert_eq!(first.coverage.distinct_tested, 12);
    assert_eq!(first.coverage.seed, Some(13));
    assert!(first.coverage.untested_remainder.contains("unbounded"));
    assert!(first
        .coverage
        .untested_remainder
        .contains("12 distinct values sampled"));
    assert!(first
        .coverage
        .untested_remainder
        .contains("all other values untested"));
    assert!(first
        .observations
        .iter()
        .any(|o| o.input == Value::Int(i64::MIN)));
    assert!(first
        .observations
        .iter()
        .any(|o| o.input == Value::Int(i64::MAX)));
    assert!(first.observations.iter().any(|o| o.input == Value::Int(0)));

    let failing = report_for(&loaded, "Q-020");
    assert_eq!(failing.result, Outcome::CounterexampleFound);
    assert!(failing
        .observations
        .iter()
        .any(|o| o.input == Value::Int(i64::MAX) && !o.satisfied));
    assert!(failing
        .coverage
        .untested_remainder
        .contains("all other values untested"));
}

#[test]
fn sampled_quantifier_records_seed_and_untested_remainder() {
    let expected_true = true;

    let loaded = reference();
    let report = quantifier::check_claim(&loaded.claims[12]).unwrap();
    assert_eq!(report.id, "Q-013");
    assert_eq!(report.coverage.tested_cases, 12);
    assert_eq!(report.coverage.seed, Some(13));
    assert_eq!(
        report.coverage.untested_remainder.contains("unbounded"),
        expected_true
    );
    assert_eq!(
        report
            .coverage
            .untested_remainder
            .contains("all other values untested"),
        expected_true
    );
}

#[test]
fn existential_outcomes_distinguish_witness_from_no_witness_found() {
    let loaded = reference();
    let witness = report_for(&loaded, "Q-030");
    assert_eq!(witness.result, Outcome::WitnessFound);
    assert!(witness.witness.is_some());
    let unknown = report_for(&loaded, "Q-029");
    assert_eq!(unknown.result, Outcome::NoWitnessAmongTestedCases);
    assert!(unknown.witness.is_none());
    assert!(unknown.observations.iter().all(|o| !o.satisfied));
}

fn render_reports() -> (String, String, String) {
    let loaded = reference();
    let reports = loaded
        .claims
        .iter()
        .map(quantifier::check_claim)
        .collect::<quantifier::Result<Vec<_>>>()
        .unwrap();
    let markdown = quantifier::view::markdown(&reports).unwrap();
    let html = quantifier::view::html(&reports).unwrap();
    let json = quantifier::view::json(&reports).unwrap();
    (markdown, html, json)
}

#[test]
fn coverage_view_no_proof() {
    let (markdown, html, json) = render_reports();
    for rendered in [&markdown, &html, &json] {
        quantifier::view::assert_proof_free(rendered).unwrap();
    }
    assert!(markdown.contains("Tested here"));
    assert!(markdown.contains("Claimed everywhere"));
    assert!(html.contains("Tested here"));
    assert!(html.contains("Claimed everywhere"));
}

fn validate_schema(
    instance: &serde_json::Value,
    schema: &serde_json::Value,
    location: &str,
    edition_schema: &serde_json::Value,
    annotation_schema: &serde_json::Value,
    current_schema: &serde_json::Value,
) -> Result<(), String> {
    const KEYWORDS: &[&str] = &[
        "$schema",
        "$id",
        "$defs",
        "title",
        "type",
        "required",
        "properties",
        "additionalProperties",
        "propertyNames",
        "items",
        "contains",
        "minContains",
        "maxContains",
        "minItems",
        "maxItems",
        "uniqueItems",
        "minLength",
        "maxLength",
        "minimum",
        "maximum",
        "pattern",
        "const",
        "enum",
        "$ref",
        "oneOf",
        "anyOf",
        "allOf",
        "not",
        "if",
        "then",
        "else",
    ];
    if let Some(object) = schema.as_object() {
        if let Some(keyword) = object.keys().find(|key| !KEYWORDS.contains(&key.as_str())) {
            return Err(format!("{location}: unsupported schema keyword {keyword}"));
        }
    }
    if let Some(reference) = schema.get("$ref").and_then(serde_json::Value::as_str) {
        let (document, pointer) = reference.split_once('#').unwrap_or(("", reference));
        let root = match document {
            "" => current_schema,
            "living-spec-quantifier-edition-v1.schema.json" => edition_schema,
            "living-spec-annotations-v1.schema.json" => annotation_schema,
            _ => return Err(format!("{location}: unsupported schema reference")),
        };
        let target = root
            .pointer(pointer)
            .ok_or_else(|| format!("{location}: unresolved schema reference"))?;
        return validate_schema(
            instance,
            target,
            location,
            edition_schema,
            annotation_schema,
            root,
        );
    }
    for keyword in ["allOf", "anyOf"] {
        if let Some(schemas) = schema.get(keyword).and_then(serde_json::Value::as_array) {
            let mut matches = 0;
            for child in schemas {
                let result = validate_schema(
                    instance,
                    child,
                    location,
                    edition_schema,
                    annotation_schema,
                    current_schema,
                );
                if keyword == "allOf" {
                    result?;
                } else if result.is_ok() {
                    matches += 1;
                }
            }
            if keyword == "anyOf" && matches == 0 {
                return Err(format!("{location}: no schema alternative matched"));
            }
        }
    }
    if let Some(alternatives) = schema.get("oneOf").and_then(serde_json::Value::as_array) {
        let accepted = alternatives
            .iter()
            .filter(|alternative| {
                validate_schema(
                    instance,
                    alternative,
                    location,
                    edition_schema,
                    annotation_schema,
                    current_schema,
                )
                .is_ok()
            })
            .count();
        if accepted != 1 {
            return Err(format!(
                "{location}: expected exactly one schema alternative"
            ));
        }
        return Ok(());
    }
    if let Some(child) = schema.get("not") {
        if validate_schema(
            instance,
            child,
            location,
            edition_schema,
            annotation_schema,
            current_schema,
        )
        .is_ok()
        {
            return Err(format!("{location}: matched forbidden schema"));
        }
    }
    if let Some(condition) = schema.get("if") {
        let branch = if validate_schema(
            instance,
            condition,
            location,
            edition_schema,
            annotation_schema,
            current_schema,
        )
        .is_ok()
        {
            schema.get("then")
        } else {
            schema.get("else")
        };
        if let Some(branch) = branch {
            validate_schema(
                instance,
                branch,
                location,
                edition_schema,
                annotation_schema,
                current_schema,
            )?;
        }
    }
    if let Some(expected) = schema.get("type").and_then(serde_json::Value::as_str) {
        let matches = match expected {
            "object" => instance.is_object(),
            "array" => instance.is_array(),
            "string" => instance.is_string(),
            "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
            "number" => instance.is_number(),
            "boolean" => instance.is_boolean(),
            "null" => instance.is_null(),
            _ => return Err(format!("{location}: unsupported schema type")),
        };
        if !matches {
            return Err(format!("{location}: expected {expected}"));
        }
    }
    if let Some(expected) = schema.get("const") {
        if instance != expected {
            return Err(format!("{location}: const mismatch"));
        }
    }
    if let Some(values) = schema.get("enum").and_then(serde_json::Value::as_array) {
        if !values.contains(instance) {
            return Err(format!("{location}: enum mismatch"));
        }
    }
    if let Some(pattern) = schema.get("pattern").and_then(serde_json::Value::as_str) {
        let regex =
            Regex::new(pattern).map_err(|_| format!("{location}: invalid schema pattern"))?;
        if !instance.as_str().is_some_and(|value| regex.is_match(value)) {
            return Err(format!("{location}: pattern mismatch"));
        }
    }
    if let Some(length) = instance.as_str().map(|value| value.chars().count() as u64) {
        if schema
            .get("minLength")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|min| length < min)
            || schema
                .get("maxLength")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|max| length > max)
        {
            return Err(format!("{location}: string length out of bounds"));
        }
    }
    let integer = |value: &serde_json::Value| {
        value
            .as_i64()
            .map(i128::from)
            .or_else(|| value.as_u64().map(i128::from))
    };
    if let Some(number) = integer(instance) {
        if schema
            .get("minimum")
            .and_then(integer)
            .is_some_and(|min| number < min)
            || schema
                .get("maximum")
                .and_then(integer)
                .is_some_and(|max| number > max)
        {
            return Err(format!("{location}: number out of bounds"));
        }
    }
    if let Some(required) = schema.get("required").and_then(serde_json::Value::as_array) {
        let object = instance
            .as_object()
            .ok_or_else(|| format!("{location}: required on non-object"))?;
        for key in required.iter().filter_map(serde_json::Value::as_str) {
            if !object.contains_key(key) {
                return Err(format!("{location}.{key}: required property missing"));
            }
        }
    }
    if let (Some(object), Some(name_schema)) = (instance.as_object(), schema.get("propertyNames")) {
        for key in object.keys() {
            validate_schema(
                &serde_json::Value::String(key.clone()),
                name_schema,
                location,
                edition_schema,
                annotation_schema,
                current_schema,
            )?;
        }
    }
    if let Some(object) = instance.as_object() {
        let properties = schema
            .get("properties")
            .and_then(serde_json::Value::as_object);
        for (key, value) in object {
            if let Some(property_schema) = properties.and_then(|properties| properties.get(key)) {
                validate_schema(
                    value,
                    property_schema,
                    &format!("{location}.{key}"),
                    edition_schema,
                    annotation_schema,
                    current_schema,
                )?;
            } else if schema.get("additionalProperties") == Some(&serde_json::Value::Bool(false)) {
                return Err(format!("{location}.{key}: additional property"));
            } else if let Some(child) = schema
                .get("additionalProperties")
                .filter(|value| value.is_object())
            {
                validate_schema(
                    value,
                    child,
                    &format!("{location}.{key}"),
                    edition_schema,
                    annotation_schema,
                    current_schema,
                )?;
            }
        }
    }
    if let Some(items) = instance.as_array() {
        if schema
            .get("minItems")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|min| (items.len() as u64) < min)
            || schema
                .get("maxItems")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|max| (items.len() as u64) > max)
        {
            return Err(format!("{location}: array length out of bounds"));
        }
        if schema.get("uniqueItems") == Some(&serde_json::Value::Bool(true)) {
            for (index, item) in items.iter().enumerate() {
                if items[..index].contains(item) {
                    return Err(format!("{location}: duplicate array item"));
                }
            }
        }
        if let Some(item_schema) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                validate_schema(
                    item,
                    item_schema,
                    &format!("{location}[{index}]"),
                    edition_schema,
                    annotation_schema,
                    current_schema,
                )?;
            }
        }
        if let Some(contains) = schema.get("contains") {
            let count = items
                .iter()
                .filter(|item| {
                    validate_schema(
                        item,
                        contains,
                        location,
                        edition_schema,
                        annotation_schema,
                        current_schema,
                    )
                    .is_ok()
                })
                .count() as u64;
            let min = schema
                .get("minContains")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1);
            let max = schema
                .get("maxContains")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(u64::MAX);
            if count < min || count > max {
                return Err(format!("{location}: contains count out of bounds"));
            }
        }
    }
    Ok(())
}

#[test]
fn export_edition_all_reference_specs_matches_schema() {
    let loaded = reference();
    let reports = loaded
        .claims
        .iter()
        .map(quantifier::check_claim)
        .collect::<quantifier::Result<Vec<_>>>()
        .unwrap();
    let edition = quantifier::export::edition::build(&loaded, reports);
    let encoded = serde_json::to_value(&edition).unwrap();
    let contracts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts");
    let schema: serde_json::Value = serde_json::from_slice(
        &fs::read(contracts.join("living-spec-quantifier-edition-v1.schema.json")).unwrap(),
    )
    .unwrap();
    let annotations_schema: serde_json::Value = serde_json::from_slice(
        &fs::read(contracts.join("living-spec-annotations-v1.schema.json")).unwrap(),
    )
    .unwrap();
    validate_schema(
        &encoded,
        &schema,
        "edition",
        &schema,
        &annotations_schema,
        &schema,
    )
    .expect("edition validates against its schema");
    let mut missing = encoded.clone();
    missing["requirements"][0]["coverage"]
        .as_object_mut()
        .unwrap()
        .remove("tested_cases");
    assert!(validate_schema(
        &missing,
        &schema,
        "edition",
        &schema,
        &annotations_schema,
        &schema
    )
    .is_err());
    let mut invalid_sampling = encoded.clone();
    let sampled = invalid_sampling["requirements"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|report| report["quantifier"]["kind"] == "infinite-universal")
        .unwrap();
    sampled["quantifier"]["sampling"]["samples"] = serde_json::json!(0);
    assert!(validate_schema(
        &invalid_sampling,
        &schema,
        "edition",
        &schema,
        &annotations_schema,
        &schema
    )
    .is_err());
    let mut extra_property = encoded.clone();
    extra_property["requirements"][0]["unexpected"] = serde_json::json!(true);
    assert!(validate_schema(
        &extra_property,
        &schema,
        "edition",
        &schema,
        &annotations_schema,
        &schema
    )
    .is_err());
    let claim_count = encoded
        .get("requirements")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    assert_eq!(claim_count, 30);
    for (pointer, invalid) in [
        (
            "/manifest/fixture_hashes/Q-001",
            serde_json::json!("invalid-digest"),
        ),
        ("/manifest/seeds/Q-013", serde_json::json!("invalid-seed")),
        (
            "/requirements/0/quantifier/domain/lo",
            serde_json::json!(9223372036854775808u64),
        ),
    ] {
        let mut malformed = encoded.clone();
        *malformed.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            validate_schema(
                &malformed,
                &schema,
                "edition",
                &schema,
                &annotations_schema,
                &schema
            )
            .is_err(),
            "{pointer}"
        );
    }
    let dir = tempdir();
    quantifier::export::edition::write(&edition, dir.path()).unwrap();
    let text = fs::read_to_string(dir.path().join("edition.md")).unwrap();
    quantifier::view::assert_proof_free(&text).unwrap();
    let first_domain = serde_json::to_string(&edition.requirements[0].quantifier.domain).unwrap();
    assert!(text.contains(&format!("Declared domain: `{first_domain}`")));
}

#[test]
fn counterexample_export_preserves_failing_regions_and_labels_existential_nonwitnesses() {
    let loaded = reference();
    let reports = loaded
        .claims
        .iter()
        .map(quantifier::check_claim)
        .collect::<quantifier::Result<Vec<_>>>()
        .unwrap();
    let dir = tempdir();
    quantifier::export::counterexample::write(&reports, dir.path()).unwrap();
    let json = fs::read_to_string(dir.path().join("counterexamples.json")).unwrap();
    assert!(json.contains("Q-011"));
    assert!(json.contains("128"));
    assert!(json.contains("Q-029"));
    assert!(json.contains("non-witnesses"));
    let markdown = fs::read_to_string(dir.path().join("counterexamples.md")).unwrap();
    assert!(markdown.contains("Q\\-011"));
    assert!(markdown.contains("Q\\-029"));
    assert!(markdown.contains("non-witnesses among tested cases"));
}

#[test]
fn core_annotation_load_keeps_unknown_sections_and_rejects_orphans() {
    use living_spec::core::annotations::Annotations;

    let dir = tempdir();
    fs::write(
        dir.path().join("spec.md"),
        "- [R-123] Synthetic annotated requirement.\n",
    )
    .unwrap();
    let q = serde_json::to_value(&reference().claims[0].quantifier).unwrap();
    let source = serde_json::json!({
        "schema": "living-spec.annotations/v1",
        "requirements": { "R-123": { "quantifier": q, "future_section": { "kept": true } } },
        "future_top_level": { "kept": true }
    });
    fs::write(
        dir.path().join("annotations.json"),
        serde_json::to_vec_pretty(&source).unwrap(),
    )
    .unwrap();
    let loaded = quantifier::load(dir.path()).unwrap();
    assert_eq!(loaded.claims.len(), 1);
    let annotations = Annotations::load(dir.path().join("annotations.json")).unwrap();
    let saved = dir.path().join("roundtrip.json");
    annotations.save(&saved).unwrap();
    let roundtrip: serde_json::Value = serde_json::from_slice(&fs::read(saved).unwrap()).unwrap();
    assert_eq!(
        roundtrip["requirements"]["R-123"]["future_section"],
        serde_json::json!({"kept":true})
    );
    assert_eq!(
        roundtrip["future_top_level"],
        serde_json::json!({"kept":true})
    );

    let orphaned = serde_json::json!({
        "schema": "living-spec.annotations/v1",
        "requirements": { "R-123": { "quantifier": source["requirements"]["R-123"]["quantifier"].clone() }, "R-999": { "future": true } }
    });
    fs::write(
        dir.path().join("annotations.json"),
        serde_json::to_vec(&orphaned).unwrap(),
    )
    .unwrap();
    assert!(quantifier::load(dir.path()).is_err());
}

#[test]
fn duplicate_annotation_keys_silently_select_a_passing_subject() {
    let dir = tempdir();
    fs::write(
        dir.path().join("spec.md"),
        "- [R-001] Synthetic absolute-value claim.\n",
    )
    .unwrap();
    let section = r#"{
        "kind": "finite-universal",
        "domain": { "type": "int-range", "lo": -128, "hi": 127 },
        "subject": "abs_wrapping_i8",
        "subject": "abs_i8",
        "predicate": {
            "version": 1,
            "expression": {
                "op": "ge",
                "left": { "kind": "field", "source": "output" },
                "right": { "kind": "literal", "value": { "type": "int", "value": 0 } }
            }
        }
    }"#;
    let passing = section.replace("\"subject\": \"abs_wrapping_i8\",", "");
    let failing = section.replace("\"subject\": \"abs_i8\",", "");
    let wrap = |section: &str| {
        format!(
            r#"{{"schema":"living-spec.annotations/v1","requirements":{{"R-001":{{"quantifier":{section}}}}}}}"#
        )
    };
    let inputs = [
        wrap(section),
        // Escaped spellings are the same decoded key, too.
        wrap(&section.replace("\"subject\": \"abs_i8\"", r#""sub\u006aect": "abs_i8""#)),
        format!(
            r#"{{"schema":"living-spec.annotations/v1","requirements":{{"R-001":{{"quantifier":{failing}}},"R-001":{{"quantifier":{passing}}}}}}}"#
        ),
        format!(
            r#"{{"schema":"living-spec.annotations/v1","requirements":{{"R-001":{{"quantifier":{failing},"quantifier":{passing}}}}}}}"#
        ),
        wrap(&passing.replace("\"lo\": -128", "\"lo\": 0, \"lo\": -128")),
        // Validate every object, including unknown sections nested in arrays.
        wrap(&passing).replace(
            "\"requirements\":",
            "\"future\":[{\"kept\":true,\"kept\":false}],\"requirements\":",
        ),
    ];
    for input in inputs {
        fs::write(dir.path().join("annotations.json"), input).unwrap();
        assert!(quantifier::load(dir.path()).is_err());
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
            .args(["quantifier", "check"])
            .arg(dir.path())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
    }
    // The fixture entry point applies the same policy before typed decoding.
    let fixture = dir.path().join("claim.json");
    fs::write(
        &fixture,
        format!(
            r#"{{"id":"R-001","text":"Synthetic absolute-value claim.","quantifier":{section}}}"#
        ),
    )
    .unwrap();
    assert!(quantifier::load(&fixture).is_err());
    // Unambiguous inputs retain both the passing and failing subject behavior.
    for (section, result) in [
        (passing, Outcome::NoCounterexampleInTestedCases),
        (failing, Outcome::CounterexampleFound),
    ] {
        fs::write(dir.path().join("annotations.json"), wrap(&section)).unwrap();
        let loaded = quantifier::load(dir.path()).unwrap();
        assert_eq!(
            quantifier::check_claim(&loaded.claims[0]).unwrap().result,
            result
        );
    }
}

fn pipe_observation() -> quantifier::Loaded {
    quantifier::load(Path::new(
        "tests/fixtures/quantifier/regressions/pipe-observation.json",
    ))
    .unwrap()
}

// Read the GFM row's cells: escaped pipes are content, including in code spans.
// Keep all other escapes literal so code-span JSON can be compared to JSON export.
fn gfm_cells(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = row.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('|') => cell.push('|'),
                Some(next) => {
                    cell.push('\\');
                    cell.push(next);
                }
                None => cell.push('\\'),
            },
            '|' => {
                cells.push(cell.trim().to_owned());
                cell.clear();
            }
            _ => cell.push(c),
        }
    }
    assert!(cell.trim().is_empty());
    assert!(cells.remove(0).is_empty());
    cells
}

fn code_json(span: &str) -> serde_json::Value {
    let delimiter = span.chars().take_while(|c| *c == '`').count();
    assert!(delimiter > 0);
    assert!(span.ends_with(&"`".repeat(delimiter)));
    let content = &span[delimiter..span.len() - delimiter];
    assert!(!content.contains(&"`".repeat(delimiter)));
    serde_json::from_str(content).expect("displayed evidence is literal JSON")
}

fn assert_failed_table(markdown: &str, observation: &quantifier::Observation) {
    let row = markdown
        .lines()
        .find(|line| line.starts_with("| `"))
        .unwrap();
    let cells = gfm_cells(row);
    assert_eq!(cells.len(), 3, "{row}");
    assert_eq!(
        code_json(&cells[0]),
        serde_json::to_value(&observation.input).unwrap()
    );
    assert_eq!(
        code_json(&cells[1]),
        serde_json::to_value(&observation.output).unwrap()
    );
    assert_eq!(cells[2], "counterexample");
}

#[test]
fn pipes_in_observed_strings_can_turn_a_failed_table_row_into_satisfied() {
    let loaded = pipe_observation();
    let report = quantifier::check_claim(&loaded.claims[0]).unwrap();
    assert_eq!(report.result, Outcome::CounterexampleFound);
    assert_eq!(
        report.observations[0].input,
        Value::String("x|y|satisfied|z".into())
    );
    assert_eq!(report.observations[0].output, Value::String(String::new()));
    let rendered = quantifier::view::markdown(std::slice::from_ref(&report)).unwrap();
    assert_failed_table(&rendered, &report.observations[0]);
    let edition = quantifier::export::edition::build(&loaded, vec![report.clone()]);
    let dir = tempdir();
    quantifier::export::edition::write(&edition, dir.path()).unwrap();
    let rendered = fs::read_to_string(dir.path().join("edition.md")).unwrap();
    // Skip the edition's manifest table.
    assert_failed_table(
        rendered.split("## Requirements").nth(1).unwrap(),
        &report.observations[0],
    );
}

#[test]
fn counterexample_markdown_changes_the_concrete_evidence() {
    let loaded = quantifier::load(&quantifier::reference_dir().join("Q-011.json")).unwrap();
    let integer = quantifier::check_claim(&loaded.claims[0]).unwrap();
    let mut string_claim = pipe_observation().claims.remove(0);
    // Exercise backtick delimiters, entities and JSON backslashes, including
    // backslashes immediately before a pipe, in both input and output.
    string_claim.quantifier.domain = Domain::Enum {
        values: vec![Value::String(
            "&amp; & <tag> \\ \\| ` `` x|y|satisfied|z".into(),
        )],
    };
    string_claim.quantifier.subject = "utf8_roundtrip".into();
    string_claim.quantifier.predicate.expression = Expr::Eq {
        left: Operand::Field {
            source: Source::Output,
            path: vec![],
        },
        right: Operand::Literal {
            value: Value::String(String::new()),
        },
    };
    let string = quantifier::check_claim(&string_claim).unwrap();
    for report in [integer, string] {
        assert_eq!(report.result, Outcome::CounterexampleFound);
        let dir = tempdir();
        quantifier::export::counterexample::write(&[report], dir.path()).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.path().join("counterexamples.json")).unwrap())
                .unwrap();
        let evidence = &json["requirements"][0];
        let observation: quantifier::Observation =
            serde_json::from_value(evidence["observations"][0].clone()).unwrap();
        let markdown = fs::read_to_string(dir.path().join("counterexamples.md")).unwrap();
        assert_failed_table(&markdown, &observation);
        let domains: Vec<_> = markdown
            .lines()
            .filter_map(|line| line.strip_prefix("- "))
            .map(code_json)
            .collect();
        assert_eq!(domains, *evidence["failing_subdomains"].as_array().unwrap());
    }
}

#[test]
fn drill2_explicit_sampling_null_is_rejected_in_annotations_and_fixtures() {
    use living_spec::core::annotations::Annotations;
    let dir = tempdir();
    let mut claim = serde_json::to_value(&reference().claims[0]).unwrap();
    claim["id"] = "R-001".into();
    claim["quantifier"]["sampling"] = serde_json::Value::Null;
    assert!(quantifier::annotations::Quantifier::from_section(&claim["quantifier"]).is_err());
    fs::write(dir.path().join("spec.md"), "- [R-001] Synthetic claim.\n").unwrap();
    let annotation_path = dir.path().join("annotations.json");
    fs::write(
        &annotation_path,
        serde_json::json!({"schema":"living-spec.annotations/v1",
        "requirements":{"R-001":{"quantifier":claim["quantifier"]}}})
        .to_string(),
    )
    .unwrap();
    assert!(Annotations::load(&annotation_path).is_err());
    assert!(quantifier::load(dir.path()).is_err());
    let fixture = dir.path().join("fixture.json");
    fs::write(&fixture, claim.to_string()).unwrap();
    assert!(quantifier::load(&fixture).is_err());
    // Removing the field preserves a valid exhaustive finite claim.
    claim["quantifier"]
        .as_object_mut()
        .unwrap()
        .remove("sampling");
    fs::write(&fixture, claim.to_string()).unwrap();
    assert!(quantifier::load(&fixture).is_ok());
}
