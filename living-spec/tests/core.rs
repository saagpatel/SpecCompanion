use living_spec::core::{
    annotations::{Annotations, Audit, AuditVerdict, Link, RequirementAnnotations},
    badge::{score_line_at_revision, Badge, Reason},
    history::{History, Transition},
    spec::{revision, Spec, SpecError},
    test_identity::{
        fingerprint, rename_hint, resolve_link_with_identities, Resolution, TestIdentity,
        TestOutcome, TestStatus,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
fn link() -> Link {
    Link {
        test_id: "test".into(),
        fingerprint: fingerprint("fn test() {\n    assert_eq!(actual(), expected());\n}"),
        rename_hint: rename_hint("fn test() {\n    assert_eq!(actual(), expected());\n}"),
        revision: revision("requirement"),
        explanation: "test".into(),
        proposed_by: "human".into(),
        audit: Audit {
            verdict: AuditVerdict::Accepted,
            revision: revision("requirement"),
            notes: vec![],
            extra: BTreeMap::new(),
        },
        extra: BTreeMap::new(),
    }
}
fn outcome(status: TestStatus) -> TestOutcome {
    TestOutcome {
        test_id: "test".into(),
        status,
        duration_ms: None,
        failure_message: None,
    }
}
fn score(
    a: &RequirementAnnotations,
    o: &[TestOutcome],
    gaps: &[&str],
) -> living_spec::core::badge::LineStatus {
    score_line_at_revision(
        "R-001",
        &revision("requirement"),
        a,
        o,
        &BTreeMap::from([(
            "test".into(),
            TestIdentity {
                fingerprint: link().fingerprint,
                rename_hint: link().rename_hint,
            },
        )]),
        &gaps.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>(),
    )
}
#[test]
fn roundtrip_and_orphans() {
    let root = Path::new("tests/fixtures/core/basic");
    let spec = Spec::parse(&fs::read_to_string(root.join("spec.md")).unwrap()).unwrap();
    assert_eq!(spec.lines.len(), 6);
    assert_eq!(spec.lines[3].heading_path, ["Behavior", "More"]);
    let a = Annotations::load(root.join("annotations.json")).unwrap();
    assert!(a.orphans(&spec).is_empty());
    let dir = tempfile::tempdir_in("target").unwrap();
    let path = dir.path().join("annotations.json");
    a.save(&path).unwrap();
    assert_eq!(
        fs::read(root.join("annotations.json")).unwrap(),
        fs::read(&path).unwrap()
    );
    assert_eq!(a, Annotations::load(path).unwrap());
    assert!(a.requirements["R-001"].extra.contains_key("future"));
    let orphan = Annotations::load("tests/fixtures/core/orphan/annotations.json").unwrap();
    assert_eq!(orphan.orphans(&spec), ["R-999"]);
}
#[test]
fn invalid_ids() {
    assert_eq!(
        Spec::parse("- [R-001] a\n- [R-001] b"),
        Err(SpecError::DuplicateId {
            id: "R-001".into(),
            line_no: 2
        })
    );
    assert_eq!(
        Spec::parse("prose\n- [bad] b"),
        Err(SpecError::MalformedId { line_no: 2 })
    );
}
#[test]
fn rule_one() {
    let mut a = RequirementAnnotations::default();
    assert_eq!(score(&a, &[], &[]).reasons, [Reason::NoLink]);
    let mut l = link();
    l.audit.verdict = AuditVerdict::Challenged;
    a.links.push(l);
    assert_eq!(score(&a, &[], &[]).reasons, [Reason::LinkChallenged]);
}
#[test]
fn rule_two() {
    let a = RequirementAnnotations {
        links: vec![link()],
        ..Default::default()
    };
    let failed = score(&a, &[outcome(TestStatus::Failed)], &[]);
    assert_eq!(
        (failed.badge, failed.reasons),
        (Badge::Broken, vec![Reason::TestFailed])
    );
    let unresolved = score_line_at_revision(
        "R-001",
        &revision("requirement"),
        &a,
        &[],
        &BTreeMap::new(),
        &BTreeSet::new(),
    );
    assert_eq!(
        (unresolved.badge, unresolved.reasons),
        (Badge::Broken, vec![Reason::LinkUnresolved])
    );
}
#[test]
fn rule_three() {
    let a = RequirementAnnotations {
        links: vec![link()],
        bounds: vec![serde_json::json!({})],
        ..Default::default()
    };
    let s = score(&a, &[outcome(TestStatus::Passed)], &["R-001"]);
    assert_eq!(
        (s.badge, s.reasons),
        (Badge::Unproven, vec![Reason::BoundaryGap])
    );
}
#[test]
fn rule_four() {
    let a = RequirementAnnotations {
        links: vec![link()],
        ..Default::default()
    };
    assert_eq!(
        score(&a, &[outcome(TestStatus::Passed)], &[]).badge,
        Badge::Proven
    );
}
#[test]
fn rename() {
    let l = link();
    let mut o = outcome(TestStatus::Passed);
    o.test_id = "renamed".into();
    let map = BTreeMap::from([(
        "renamed".into(),
        TestIdentity {
            fingerprint: fingerprint("fn renamed() {\n    assert_eq!(actual(), expected());\n}"),
            rename_hint: l.rename_hint.clone(),
        },
    )]);
    assert_eq!(
        resolve_link_with_identities(&l, &[o], &map),
        Resolution::Renamed {
            new_id: "renamed".into()
        }
    );
    assert_ne!(fingerprint(" a\n b "), fingerprint("a b"));
    assert_ne!(
        fingerprint("assert_eq!(x, \"a  b\")"),
        fingerprint("assert_eq!(x, \"a b\")")
    );
    assert_ne!(fingerprint("a  \nb\t"), fingerprint("a\nb"));
}

#[test]
fn trailing_space_inside_python_multiline_literal_changes_fingerprint() {
    let source = "def test_message():\n    assert message == '''hello  \nworld'''\n";
    let changed = source.replace("hello  \n", "hello\n");
    assert_ne!(fingerprint(source), fingerprint(&changed));
    assert_ne!(rename_hint(source), rename_hint(&changed));
}

#[test]
fn trailing_space_inside_rust_raw_string_changes_fingerprint() {
    let source = "fn test_message() {\n    assert_eq!(message, r#\"hello  \nworld\"#);\n}";
    let changed = source.replace("hello  \n", "hello\n");
    assert_ne!(fingerprint(source), fingerprint(&changed));
    assert_ne!(rename_hint(source), rename_hint(&changed));
}

#[test]
fn crlf_and_lf_sources_share_a_fingerprint() {
    let source = "fn test_message() {\n    assert!(true);\n}\n";
    for ending in ["\r\n", "\r"] {
        let converted = source.replace('\n', ending);
        assert_eq!(fingerprint(source), fingerprint(&converted));
        assert_eq!(rename_hint(source), rename_hint(&converted));
        let decorated = "@marker\ndef test_message():\n    assert value\n";
        assert!(rename_hint(decorated).is_some());
        assert_eq!(
            rename_hint(decorated),
            rename_hint(&decorated.replace('\n', ending))
        );
    }
}
#[test]
fn renamed_link_requires_reaudit() {
    let l = link();
    let mut passed = outcome(TestStatus::Passed);
    passed.test_id = "renamed".into();
    let identities = BTreeMap::from([(
        "renamed".into(),
        TestIdentity {
            fingerprint: fingerprint("fn renamed() {\n    assert_eq!(actual(), expected());\n}"),
            rename_hint: l.rename_hint.clone(),
        },
    )]);
    let annotations = RequirementAnnotations {
        links: vec![l],
        ..Default::default()
    };
    let result = score_line_at_revision(
        "R-001",
        &revision("requirement"),
        &annotations,
        &[passed],
        &identities,
        &BTreeSet::new(),
    );
    assert_eq!(result.badge, Badge::Unproven);
    assert_eq!(result.reasons, [Reason::TestChanged]);
    assert_eq!(result.tests.len(), 1);
    assert!(!result.tests[0].admissible);
}

#[test]
fn unsupported_annotation_schema_rejected() {
    let dir = tempfile::tempdir_in("target").unwrap();
    let path = dir.path().join("annotations.json");
    fs::write(
        &path,
        r#"{"schema":"living-spec.annotations/v2","requirements":{}}"#,
    )
    .unwrap();
    assert!(Annotations::load(path)
        .unwrap_err()
        .to_string()
        .contains("unsupported annotations schema"));
}

#[test]
fn rename_with_should_panic_does_not_match() {
    for (old, new) in [
        (
            "#[test]\nfn old() { assert!(value); }",
            "#[test]\n#[should_panic]\nfn new() { assert!(value); }",
        ),
        (
            "#[test]\nfn old() { assert!(value); }",
            "#[test]\n#[ignore]\nfn new() { assert!(value); }",
        ),
        ("def old(): assert value", "@mark\ndef new(): assert value"),
    ] {
        let mut l = link();
        l.rename_hint = rename_hint(old);
        let current = TestIdentity {
            fingerprint: fingerprint(new),
            rename_hint: rename_hint(new),
        };
        assert_eq!(
            resolve_link_with_identities(&l, &[], &BTreeMap::from([("new".into(), current)])),
            Resolution::Unresolved
        );
        assert_ne!(rename_hint(old), rename_hint(new));
    }
}
#[test]
fn history() {
    let d = tempfile::tempdir_in("target").unwrap();
    let h = History::new(d.path().join("history.jsonl"));
    let mut t = Transition {
        at: "2026-01-01T00:00:00Z".into(),
        commit: "a".into(),
        requirement: "R-001".into(),
        from: Badge::Unproven,
        to: Badge::Proven,
        test_id: "test".into(),
        note: "".into(),
    };
    h.append(&t).unwrap();
    t.to = Badge::Broken;
    h.append(&t).unwrap();
    assert_eq!(
        h.history_for("R-001")
            .unwrap()
            .iter()
            .map(|t| t.to)
            .collect::<Vec<_>>(),
        [Badge::Proven, Badge::Broken]
    );
    assert_eq!(h.last_transition("R-001").unwrap(), Some(t));
}

#[test]
fn skipped_test_is_not_proven() {
    let a = RequirementAnnotations {
        links: vec![link()],
        ..Default::default()
    };
    let s = score(&a, &[outcome(TestStatus::Skipped)], &[]);
    assert_eq!(s.badge, Badge::Unproven);
    assert!(s.reasons.contains(&Reason::NotExecuted));
}
#[test]
fn edited_line_makes_links_stale() {
    let a = RequirementAnnotations {
        links: vec![link()],
        ..Default::default()
    };
    let old = Spec::parse("- [R-001] amount < 30  ").unwrap();
    let new = Spec::parse("- [R-001] amount > 30").unwrap();
    assert_ne!(old.lines[0].revision, new.lines[0].revision);
    assert_eq!(old.lines[0].revision, revision("amount < 30"));
    let s = score_line_at_revision(
        "R-001",
        &new.lines[0].revision,
        &a,
        &[outcome(TestStatus::Passed)],
        &BTreeMap::from([(
            "test".into(),
            TestIdentity {
                fingerprint: link().fingerprint,
                rename_hint: None,
            },
        )]),
        &BTreeSet::new(),
    );
    assert_eq!(s.badge, Badge::Unproven);
    assert!(s.reasons.contains(&Reason::StaleRevision));
}
#[test]
fn edited_test_needs_reaudit() {
    let a = RequirementAnnotations {
        links: vec![link()],
        ..Default::default()
    };
    let identities = BTreeMap::from([(
        "test".into(),
        TestIdentity {
            fingerprint: fingerprint("fn test() { assert!(false); }"),
            rename_hint: None,
        },
    )]);
    assert_eq!(
        resolve_link_with_identities(&a.links[0], &[outcome(TestStatus::Passed)], &identities),
        Resolution::Changed
    );
    let s = score_line_at_revision(
        "R-001",
        &revision("requirement"),
        &a,
        &[outcome(TestStatus::Passed)],
        &identities,
        &BTreeSet::new(),
    );
    assert_eq!(s.badge, Badge::Unproven);
    assert_eq!(s.reasons, [Reason::TestChanged]);
}
#[test]
fn ambiguous_rename_is_unresolved() {
    let l = link();
    let identity = TestIdentity {
        fingerprint: l.fingerprint.clone(),
        rename_hint: l.rename_hint.clone(),
    };
    let map = BTreeMap::from([("a".into(), identity.clone()), ("b".into(), identity)]);
    assert_eq!(
        resolve_link_with_identities(&l, &[], &map),
        Resolution::Unresolved
    );
}
#[test]
fn parametrized_case_missing_is_not_proven() {
    let mut a = RequirementAnnotations {
        links: vec![link(), link()],
        ..Default::default()
    };
    a.links[0].test_id = "test#one".into();
    a.links[1].test_id = "test#two".into();
    let identity = TestIdentity {
        fingerprint: link().fingerprint,
        rename_hint: link().rename_hint,
    };
    let identities = BTreeMap::from([
        ("test#one".into(), identity.clone()),
        ("test#two".into(), identity),
    ]);
    let mut o = outcome(TestStatus::Passed);
    o.test_id = "test#one".into();
    let s = score_line_at_revision(
        "R-001",
        &revision("requirement"),
        &a,
        &[o],
        &identities,
        &BTreeSet::new(),
    );
    assert_eq!(s.badge, Badge::Unproven);
    assert_eq!(s.reasons, [Reason::NotExecuted]);
    // A matching body in a different case never repairs a missing case.
    assert_eq!(
        resolve_link_with_identities(
            &a.links[1],
            &[],
            &BTreeMap::from([("test#one".into(), identities["test#one"].clone())])
        ),
        Resolution::Unresolved
    );
}
#[test]
fn challenged_failure_is_diagnostic_only() {
    let mut a = RequirementAnnotations {
        links: vec![link()],
        ..Default::default()
    };
    a.links[0].audit.verdict = AuditVerdict::Challenged;
    let s = score(&a, &[outcome(TestStatus::Failed)], &[]);
    assert_eq!(s.badge, Badge::Unproven);
    assert_eq!(s.tests[0].status, TestStatus::Failed);
    assert_eq!(s.reasons, [Reason::LinkChallenged]);
    let mut good = link();
    good.test_id = "good".into();
    a.links.push(good);
    let identities = a
        .links
        .iter()
        .map(|l| {
            (
                l.test_id.clone(),
                TestIdentity {
                    fingerprint: l.fingerprint.clone(),
                    rename_hint: l.rename_hint.clone(),
                },
            )
        })
        .collect();
    let mut good = outcome(TestStatus::Passed);
    good.test_id = "good".into();
    let s = score_line_at_revision(
        "R-001",
        &revision("requirement"),
        &a,
        &[outcome(TestStatus::Failed), good],
        &identities,
        &BTreeSet::new(),
    );
    assert_eq!(s.badge, Badge::Proven);
}
#[test]
fn fenced_examples_are_ignored() {
    let spec = Spec::parse("```markdown\n- [bad] ignored\n``` not a closing fence\n- [R-001] ignored too\n```\n- [R-001] real\n~~~\n- [R-001] ignored\n~~~").unwrap();
    assert_eq!(spec.lines.len(), 1);
    assert_eq!(spec.lines[0].text, "real");
    assert_eq!(
        Spec::parse("```\n- [R-999] example\n```\n")
            .unwrap()
            .lines
            .len(),
        0
    );
}

#[test]
fn human_overrides_require_authorship_and_reason_and_preserve_other_gates() {
    let mut a = RequirementAnnotations {
        links: vec![link()],
        ..Default::default()
    };
    a.links[0].audit.verdict = AuditVerdict::Challenged;
    let dir = tempfile::tempdir_in("target").unwrap();
    let path = dir.path().join("annotations.json");
    for invalid in [
        serde_json::json!({"by":"linker", "reason":"reviewed"}),
        serde_json::json!({"by":"human", "reason":"  \n"}),
        serde_json::json!({"by":"human"}),
        serde_json::json!({"reason":"reviewed"}),
        serde_json::json!(null),
    ] {
        a.links[0].audit.extra.insert("override".into(), invalid);
        assert_eq!(
            score(&a, &[outcome(TestStatus::Passed)], &[]).badge,
            Badge::Unproven
        );
        Annotations {
            schema: "living-spec.annotations/v1".into(),
            requirements: BTreeMap::from([("R-001".into(), a.clone())]),
            extra: BTreeMap::new(),
        }
        .save(&path)
        .unwrap();
        assert!(Annotations::load(&path).is_err());
    }
    a.links[0].audit.extra.insert(
        "override".into(),
        serde_json::json!({"by":"human", "reason":"Reviewed the assertion limitation"}),
    );
    let accepted = score(&a, &[outcome(TestStatus::Passed)], &[]);
    assert_eq!(accepted.badge, Badge::Proven);
    assert!(accepted.reasons.contains(&Reason::HumanOverride));
    assert_eq!(a.links[0].audit.verdict, AuditVerdict::Challenged);
    assert_eq!(
        score(&a, &[outcome(TestStatus::Failed)], &[]).badge,
        Badge::Broken
    );
    assert_eq!(
        score(&a, &[outcome(TestStatus::Skipped)], &[]).badge,
        Badge::Unproven
    );
    a.bounds.push(serde_json::json!({}));
    assert_eq!(
        score(&a, &[outcome(TestStatus::Passed)], &["R-001"]).badge,
        Badge::Unproven
    );
    a.bounds.clear();
    a.links[0].audit.revision = "old".into();
    assert_eq!(
        score(&a, &[outcome(TestStatus::Passed)], &[]).badge,
        Badge::Unproven
    );
    a.links[0].audit.revision = revision("requirement");
    a.links[0].fingerprint = "changed".into();
    assert_eq!(
        score(&a, &[outcome(TestStatus::Passed)], &[]).badge,
        Badge::Unproven
    );
    a.links[0].test_id = "missing".into();
    a.links[0].rename_hint = None;
    assert_eq!(
        score(&a, &[outcome(TestStatus::Passed)], &[]).badge,
        Badge::Broken
    );
}

#[test]
fn challenged_unresolved_link_still_breaks() {
    let mut a = RequirementAnnotations {
        links: vec![link()],
        ..Default::default()
    };
    a.links[0].audit.verdict = AuditVerdict::Challenged;
    let s = score_line_at_revision(
        "R-001",
        &revision("requirement"),
        &a,
        &[],
        &BTreeMap::new(),
        &BTreeSet::new(),
    );
    assert_eq!(s.badge, Badge::Broken);
    assert!(s.reasons.contains(&Reason::LinkUnresolved));
}
#[test]
fn missing_current_digest_cannot_establish_proof() {
    let a = RequirementAnnotations {
        links: vec![link()],
        ..Default::default()
    };
    let s = score_line_at_revision(
        "R-001",
        &revision("requirement"),
        &a,
        &[outcome(TestStatus::Passed)],
        &BTreeMap::new(),
        &BTreeSet::new(),
    );
    assert_eq!(s.badge, Badge::Unproven);
    assert_eq!(s.reasons, [Reason::TestChanged]);
}
#[test]
fn audit_revision_must_also_be_current() {
    let mut a = RequirementAnnotations {
        links: vec![link()],
        ..Default::default()
    };
    a.links[0].audit.revision = revision("old requirement");
    let s = score(&a, &[outcome(TestStatus::Passed)], &[]);
    assert_eq!(s.badge, Badge::Unproven);
    assert!(s.reasons.contains(&Reason::StaleRevision));
}
#[test]
fn declaration_only_hint_cannot_resolve_rename() {
    let mut l = link();
    l.rename_hint = rename_hint("fn old() { assert!(false); }");
    let identity = TestIdentity {
        fingerprint: fingerprint("fn new() { assert!(true); }"),
        rename_hint: rename_hint("fn new() { assert!(true); }"),
    };
    assert_eq!(
        resolve_link_with_identities(&l, &[], &BTreeMap::from([("new".into(), identity)])),
        Resolution::Unresolved
    );
}

#[test]
fn rename_hint_keeps_code_on_declaration_line() {
    for (old, renamed, changed, _body) in [
        (
            "fn old() { assert_eq!(f(), 1);\n}",
            "fn new() { assert_eq!(f(), 1);\n}",
            "fn new() { assert_eq!(f(), 2);\n}",
            " assert_eq!(f(), 1);\n}",
        ),
        (
            "#[test]\nfn old() { assert_eq!(f(), 1);\n}",
            "#[test]\nfn new() { assert_eq!(f(), 1);\n}",
            "#[test]\nfn new() { assert_eq!(f(), 2);\n}",
            " assert_eq!(f(), 1);\n}",
        ),
        (
            "@mark\ndef old(): assert f() == 1\n",
            "@mark\ndef new(): assert f() == 1\n",
            "@mark\ndef new(): assert f() == 2\n",
            " assert f() == 1\n",
        ),
        (
            "it(\"old\", () => { expect(f()).toBe(1);\n});",
            "it(\"new\", () => { expect(f()).toBe(1);\n});",
            "it(\"new\", () => { expect(f()).toBe(2);\n});",
            " expect(f()).toBe(1);\n});",
        ),
        (
            "fn old(\n)\n{ assert_eq!(f(), 1);\n}",
            "fn new(\n)\n{ assert_eq!(f(), 1);\n}",
            "fn new(\n)\n{ assert_eq!(f(), 2);\n}",
            " assert_eq!(f(), 1);\n}",
        ),
    ] {
        let hint = rename_hint(old);
        assert!(hint.is_some());
        assert_eq!(hint, rename_hint(renamed));
        assert_ne!(hint, rename_hint(changed));
        let mut l = link();
        l.rename_hint = hint;
        let identities = BTreeMap::from([(
            "new".into(),
            TestIdentity {
                fingerprint: fingerprint(changed),
                rename_hint: rename_hint(changed),
            },
        )]);
        assert_eq!(
            resolve_link_with_identities(&l, &[], &identities),
            Resolution::Unresolved
        );
    }
}

#[test]
fn uncertain_extraction_refuses_rename() {
    for source in [
        "declaration\nassert!(value);",
        "fn old()",
        "fn old() {",
        "fn old() -> Result<(), Error> { assert!(value); }",
        "def old(value={'key': 1}): assert value",
        "it(title, () => { expect(value).toBe(1); });",
        "it(\"old\", () => expect(value).toBe(1));",
    ] {
        assert_eq!(rename_hint(source), None, "{source}");
        let mut l = link();
        l.rename_hint = rename_hint(source);
        let identities = BTreeMap::from([(
            "new".into(),
            TestIdentity {
                fingerprint: fingerprint(source),
                rename_hint: rename_hint(source),
            },
        )]);
        assert_eq!(
            resolve_link_with_identities(&l, &[], &identities),
            Resolution::Unresolved
        );
    }
}

fn assert_duplicate_outcomes_are_broken(statuses: [TestStatus; 2]) {
    let annotations = RequirementAnnotations {
        links: vec![link()],
        ..Default::default()
    };
    let mut outcomes = statuses.map(outcome);
    outcomes[0].duration_ms = Some(1);
    outcomes[1].duration_ms = Some(2);
    let result = score(&annotations, &outcomes, &[]);
    assert_eq!(result.badge, Badge::Broken);
    assert!(result.reasons.contains(&Reason::TestFailed));
    assert_eq!(result.tests.len(), 2);
    for (evidence, outcome) in result.tests.iter().zip(outcomes) {
        assert_eq!(evidence.test_id, outcome.test_id);
        assert_eq!(evidence.status, outcome.status);
        assert_eq!(evidence.duration_ms, outcome.duration_ms);
    }
}

#[test]
fn duplicate_outcome_pass_then_fail_is_broken() {
    assert_duplicate_outcomes_are_broken([TestStatus::Passed, TestStatus::Failed]);
    assert_duplicate_outcomes_are_broken([TestStatus::Passed, TestStatus::Errored]);
}

#[test]
fn duplicate_outcome_fail_then_pass_is_broken() {
    assert_duplicate_outcomes_are_broken([TestStatus::Failed, TestStatus::Passed]);
    assert_duplicate_outcomes_are_broken([TestStatus::Errored, TestStatus::Passed]);
}
