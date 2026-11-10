use living_spec::core::mutation::*;
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::LazyLock};
static CONTRACTS: LazyLock<BTreeMap<&str, Value>> = LazyLock::new(BTreeMap::new);
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

fn sample() -> MutationFile {
    MutationFile::from_bytes(include_bytes!("fixtures/mutation/sample.json")).unwrap()
}
fn bindings<'a>() -> Bindings<'a> {
    Bindings {
        test_fingerprint: "sha256:test",
        requirement_revision: "sha256:revision",
        subject_digest: Some("sha256:subject"),
    }
}
#[test]
fn tier_requires_every_admissible_link_checked() {
    assert_eq!(
        line_tier(&[LinkState::Checked, LinkState::NotRun], true)
            .unwrap()
            .state,
        TierState::NotRun
    );
    assert_eq!(
        line_tier(&[LinkState::Checked], true).unwrap().state,
        TierState::MutationChecked
    );
}
#[test]
fn waiver_makes_signed_off_not_checked() {
    let f = sample();
    let state = evaluate_link(Some(&f.links[3]), &bindings(), &f.signoffs);
    assert_eq!(state, LinkState::Waived { waived: 1 });
    assert_eq!(
        line_tier(&[state], true).unwrap().state,
        TierState::SignedOff
    );
}
#[test]
fn timeout_is_error_not_kill() {
    let mut f = sample();
    f.links[0].mutants[0].outcome = MutantOutcome::Timeout;
    f.links[0].status = LinkStatus::Error;
    f.links[0].reason = Some("timeout".into());
    assert!(f.validate().is_ok());
    f.links[0].status = LinkStatus::Checked;
    assert!(f.validate().is_err());
}
#[test]
fn killed_mutant_then_infra_failure_is_error_and_keeps_evidence() {
    let f = sample();
    let link = &f.links[4];
    assert_eq!(link.status, LinkStatus::Error);
    assert_eq!(link.mutants[0].outcome, MutantOutcome::Killed);
    assert_eq!(
        evaluate_link(Some(link), &bindings(), &[]),
        LinkState::Error
    );
    let round_trip = MutationFile::from_bytes(&f.to_pretty_bytes().unwrap()).unwrap();
    assert_eq!(round_trip.links[4].mutants, link.mutants);
}
#[test]
fn infra_failure_blocks_checked_even_if_all_killed() {
    let mut f = sample();
    f.links[4].status = LinkStatus::Checked;
    assert!(f.validate().is_err());
    assert_eq!(
        evaluate_link(Some(&f.links[4]), &bindings(), &[]),
        LinkState::Error
    );
}
#[test]
fn empty_infra_failure_rejected() {
    let mut f = sample();
    for value in ["", " \t"] {
        f.links[4].infra_failure = Some(value.into());
        assert!(f.validate().is_err());
    }
}
#[test]
fn stored_status_must_match_evidence() {
    let mut f = sample();
    f.links[0].status = LinkStatus::Survived;
    assert!(f.validate().is_err());
}
#[test]
fn stale_binding_is_never_checked() {
    let f = sample();
    let mut b = bindings();
    b.subject_digest = Some("other");
    assert_eq!(evaluate_link(Some(&f.links[0]), &b, &[]), LinkState::Stale);
}
#[test]
fn waiver_bound_to_mutant_and_subject() {
    let mut f = sample();
    if let Signoff::Mutant { mutant_id, .. } = &mut f.signoffs[0] {
        *mutant_id = "other".into();
    }
    assert!(matches!(
        evaluate_link(Some(&f.links[3]), &bindings(), &f.signoffs),
        LinkState::Survived { .. }
    ));
    let mut b = bindings();
    b.subject_digest = Some("other");
    assert_eq!(
        evaluate_link(Some(&f.links[3]), &b, &f.signoffs),
        LinkState::Stale
    );
}
#[test]
fn link_signoff_never_applies_to_survived() {
    let mut f = sample();
    if let Signoff::Link {
        requirement,
        test_id,
        ..
    } = &mut f.signoffs[1]
    {
        *requirement = f.links[1].requirement.clone();
        *test_id = f.links[1].test_id.clone();
    }
    assert!(matches!(
        evaluate_link(Some(&f.links[1]), &bindings(), &f.signoffs),
        LinkState::Survived { .. }
    ));
}
#[test]
fn signoff_bound_to_fingerprint_and_revision() {
    let f = sample();
    assert_eq!(
        evaluate_link(Some(&f.links[2]), &bindings(), &f.signoffs),
        LinkState::SignedOff
    );
    for b in [
        Bindings {
            test_fingerprint: "other",
            ..bindings()
        },
        Bindings {
            requirement_revision: "other",
            ..bindings()
        },
    ] {
        assert_eq!(
            evaluate_link(Some(&f.links[2]), &b, &f.signoffs),
            LinkState::Stale
        );
    }
}
#[test]
fn unknown_field_rejected() {
    let mut v: Value =
        serde_json::from_slice(include_bytes!("fixtures/mutation/sample.json")).unwrap();
    v["links"][0]["foreign"] = json!(true);
    assert!(MutationFile::from_bytes(&serde_json::to_vec(&v).unwrap()).is_err());
}
#[test]
fn mutant_id_is_stable() {
    assert_eq!(
        mutant_id("src/lib.rs", "clamp", Operator::CmpFlip, "<=", ">", 0),
        sample().links[0].mutants[0].id
    );
    assert_ne!(
        mutant_id("src/lib.rs", "clamp", Operator::CmpFlip, "<=", ">", 1),
        sample().links[0].mutants[0].id
    );
}
#[test]
fn validate_rejects_checked_with_survivor() {
    let mut f = sample();
    f.links[1].status = LinkStatus::Checked;
    assert!(f.validate().is_err());
}
#[test]
fn validate_rejects_absolute_path() {
    let mut f = sample();
    let m = &mut f.links[0].mutants[0];
    m.file = "/src/lib.rs".into();
    m.id = mutant_id(
        &m.file,
        &m.function,
        m.operator,
        &m.original,
        &m.replacement,
        m.ordinal,
    );
    assert!(f.validate().is_err());
}
#[test]
fn non_proven_line_has_no_tier() {
    assert!(line_tier(&[LinkState::Checked], false).is_none());
}
#[test]
fn sample_fixture_round_trips_byte_stable() {
    assert_eq!(
        sample().to_pretty_bytes().unwrap(),
        include_bytes!("fixtures/mutation/sample.json")
    );
}
#[test]
fn schema_code_parity() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../contracts/living-spec-mutation-v1.schema.json"
    ))
    .unwrap();
    let mut fixture: Value =
        serde_json::from_slice(include_bytes!("fixtures/mutation/sample.json")).unwrap();
    assert!(validate(&schema, &fixture, &schema));
    assert!(MutationFile::from_bytes(&serde_json::to_vec(&fixture).unwrap()).is_ok());
    fixture["links"][0]["foreign"] = json!(true);
    assert!(!validate(&schema, &fixture, &schema));
    assert!(MutationFile::from_bytes(&serde_json::to_vec(&fixture).unwrap()).is_err());
}

#[test]
fn schema_and_rust_validation_reject_invalid_evidence() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../contracts/living-spec-mutation-v1.schema.json"
    ))
    .unwrap();
    let fixture: Value =
        serde_json::from_slice(include_bytes!("fixtures/mutation/sample.json")).unwrap();
    let results = |value: &Value| {
        (
            validate(&schema, value, &schema),
            MutationFile::from_bytes(&serde_json::to_vec(value).unwrap()).is_ok(),
        )
    };
    assert_eq!(results(&fixture), (true, true));
    let mut bad = fixture.clone();
    bad["links"][0]["status"] = json!("survived");
    assert_eq!(results(&bad), (false, false));
    let mut bad = fixture.clone();
    bad["links"][0]["mutants"][0]["file"] = json!("../src/lib.rs");
    assert_eq!(results(&bad), (false, false));
    for value in ["", " \t"] {
        let mut bad = fixture.clone();
        bad["links"][4]["infra_failure"] = json!(value);
        assert_eq!(results(&bad), (false, false));
    }
    let mut bad = fixture.clone();
    bad["links"][4]["infra_failure"] = Value::Null;
    assert_eq!(results(&bad), (false, false));
    let mut bad = fixture.clone();
    bad["links"][4]["status"] = json!("checked");
    assert_eq!(results(&bad), (false, false));
    let mut bad = fixture.clone();
    bad["links"][1]["status"] = json!("error");
    bad["links"][1]["reason"] = json!("spawn failed");
    bad["links"][1]["infra_failure"] = json!("spawn failed");
    assert_eq!(results(&bad), (true, true));

    // Hash recomputation and link-key uniqueness are intentionally Rust-only;
    // status-from-evidence is also expressible and enforced by this schema.
    let mut bad_hash = fixture.clone();
    bad_hash["links"][0]["mutants"][0]["id"] = json!("m-000000000000");
    let mut duplicate_link = fixture.clone();
    duplicate_link["links"]
        .as_array_mut()
        .unwrap()
        .push(fixture["links"][0].clone());
    for value in [bad_hash, duplicate_link] {
        assert_eq!(results(&value), (true, false));
    }
}
