//! Layer 3 review inputs must stay challenged even with passing outcomes.
use living_spec::{
    agents::{
        auditor::audit,
        test_index::{TestEntry, TestIndex},
    },
    core::{
        annotations::{Audit, AuditVerdict, Link, RequirementAnnotations},
        badge::{score_line_at_revision, Badge, Reason},
        spec::Spec,
        test_identity::{TestOutcome, TestStatus},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

fn indexed(file: &str, source: &str) -> TestEntry {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
    let index = TestIndex::build(dir.path()).unwrap();
    assert_eq!(index.entries.len(), 1, "{source}");
    index.entries.into_iter().next().unwrap()
}

fn challenged_even_when_passed(entry: &TestEntry) {
    let spec = Spec::parse("- [R-001] Export timestamp is 30.").unwrap();
    let requirement = &spec.lines[0];
    let mut link = Link {
        test_id: entry.id.clone(),
        fingerprint: entry.fingerprint.clone(),
        rename_hint: Some(entry.rename_hint.clone()),
        revision: requirement.revision.clone(),
        explanation: "Review regression".into(),
        proposed_by: "human".into(),
        audit: Audit {
            revision: requirement.revision.clone(),
            verdict: AuditVerdict::Accepted,
            notes: vec![],
            extra: BTreeMap::new(),
        },
        extra: BTreeMap::new(),
    };
    link.audit = audit(requirement, &link, entry);
    assert_eq!(
        link.audit.verdict,
        AuditVerdict::Challenged,
        "{}: {:?}",
        entry.source,
        link.audit.notes
    );
    assert!(link
        .audit
        .notes
        .iter()
        .any(|note| note.starts_with("unsupported test shape: ")));
    let outcome = TestOutcome {
        test_id: entry.id.clone(),
        status: TestStatus::Passed,
        duration_ms: None,
        failure_message: None,
    };
    let identities = TestIndex {
        entries: vec![entry.clone()],
    }
    .identities();
    let status = score_line_at_revision(
        "R-001",
        &requirement.revision,
        &RequirementAnnotations {
            links: vec![link],
            ..Default::default()
        },
        &[outcome],
        &identities,
        &BTreeSet::new(),
    );
    assert_eq!(status.badge, Badge::Unproven);
    assert!(status.reasons.contains(&Reason::LinkChallenged));
}

#[test]
fn javascript_regex_literals_become_fabricated_assertion_evidence() {
    for body in [
        "/expect(export_timestamp(30)).toBe(30)/.test('x');",
        r"/[/]expect(export_timestamp(30)).toBe(30)/gi.test('x');",
        r"/\/expect(export_timestamp(30)).toBe(30)/.test('x');",
        "if (true) /expect(export_timestamp(30)).toBe(30)/.test('x');",
        "const ratio = export_timestamp(30) / 30; expect(ratio).toBe(1);",
        "const ratio = 30 / 2 / 3; expect(export_timestamp(ratio)).toBe(30);",
    ] {
        challenged_even_when_passed(&indexed(
            "test.js",
            &format!("test('export timestamp', () => {{\n    {body}\n}});"),
        ));
    }
    // A regex containing a syntactically complete fake test must stay opaque
    // during discovery too, including braces in character classes.
    let e = indexed(
        "test.js",
        r#"
const text = /test('fake', () => { expect(export_timestamp(30)).toBe(30); })[}]/;
test('real', () => { expect(export_timestamp(30)).toBe(30); });
"#,
    );
    assert_eq!(e.name, "real");
}

#[test]
fn irrefutable_rust_binding_patterns_pass_the_placeholder_gate() {
    for pattern in [
        "any_value",
        "_",
        "mut any_value",
        "ref any_value",
        "value @ _",
        "(left, right)",
        "[a, b]",
        "Wrapper(value)",
        "ExportTimestamp",
        "Ok(_) | Err(_)",
        "Some(_) | None",
        "0 | any_value",
    ] {
        challenged_even_when_passed(&indexed(
            "tests/export.rs",
            &format!(
                r#"
fn export_timestamp(_: u32) -> u32 {{ 0 }}
#[test]
fn test_export_timestamp() {{
    assert!(matches!(export_timestamp(30), {pattern}));
}}
"#
            ),
        ));
    }
}

#[test]
fn a_placeholder_assertion_can_supply_the_expected_panic() {
    for body in [
        "export_timestamp(30); assert!(false);",
        "export_timestamp(30); assert_eq!(0, 1);",
        "export_timestamp(30); assert_ne!(1, 1);",
        "export_timestamp(30); assert!(unrelated());",
        "unrelated(); export_timestamp(30);",
        "export_timestamp(panic!(\"unconditional\"), 30);",
        "export_timestamp(unrelated(30));",
        "export_timestamp(values[30]);",
    ] {
        challenged_even_when_passed(&indexed(
            "tests/export.rs",
            &format!(
                r#"
fn export_timestamp(_: u32) -> u32 {{ 0 }}
#[test]
#[should_panic]
fn test_export_timestamp() {{ {body} }}
"#
            ),
        ));
    }
}

#[test]
fn multiline_decorators_bypass_the_decorated_test_safeguard() {
    let source = r#"def skip_assertion():
    return lambda fn: lambda: None

def export_timestamp(value):
    raise RuntimeError('the decorated test never calls this')

@skip_assertion(
)
def test_export_timestamp():
    assert export_timestamp(30) == 30
"#;
    let original = indexed("test_export.py", source);
    assert!(original.decorated);
    assert!(original.source.starts_with("@skip_assertion(\n)\n"));
    assert_eq!(original.line, 7);
    challenged_even_when_passed(&original);
    let changed = indexed(
        "test_export.py",
        &source.replace("@skip_assertion(\n)", "@skip_assertion(\n    30,\n)"),
    );
    assert_ne!(original.fingerprint, changed.fingerprint);

    for decorators in [
        "@outer\n@skip_assertion(\n    (30, {'nested': [1, 2]}),\n)\n# comment\n",
        "@skip_assertion(\n    foo\n@ bar\n)\n",
        "@skip_assertion \\\n    (\n    )\n",
    ] {
        let e = indexed(
            "test_export.py",
            &format!(
                "{decorators}def test_export_timestamp():\n    assert export_timestamp(30) == 30\n"
            ),
        );
        assert!(e.decorated);
        assert!(e.source.starts_with(decorators));
        challenged_even_when_passed(&e);
    }
    let method = indexed("test_export.py", "class TestExport:\n    @skip_assertion(\n    )\n    def test_export_timestamp(self):\n        assert export_timestamp(30) == 30\n");
    assert!(method.decorated);
    challenged_even_when_passed(&method);
}

#[test]
fn unsupported_python_context_managers_can_swallow_failed_assertions() {
    let source = r#"import contextlib

def export_timestamp(value):
    return 0

def test_export_timestamp():
    with contextlib.suppress(AssertionError):
        assert export_timestamp(30) == 30
"#;
    challenged_even_when_passed(&indexed("test_export.py", source));
    challenged_even_when_passed(&indexed("test_export.py", &source.replace(
        "import contextlib", "class Swallow:\n    def __enter__(self): pass\n    def __exit__(self, *args): return True"
    ).replace("contextlib.suppress(AssertionError)", "Swallow()")));
    for body in [
        "with pytest.raises(AssertionError):\n        export_timestamp(30)\n        assert False",
        "with pytest.raises(AssertionError):\n        assert export_timestamp(30) == 30",
        "with pytest.raises(AssertionError):\n        export_timestamp(30); assert False",
        "with pytest.raises(AssertionError):\n        export_timestamp(unrelated(30))",
        "with pytest.raises(ValueError), contextlib.suppress(AssertionError):\n        assert export_timestamp(30) == 30",
        "with pytest.raises(ValueError) as captured:\n        export_timestamp(30)",
        "with pytest.raises(make_exception()):\n        export_timestamp(30)",
        "with pytest.raises(ValueError):\n        export_timestamp(30)\n        unrelated()",
    ] {
        challenged_even_when_passed(&indexed("test_export.py", &format!(
            "def test_export_timestamp():\n    {body}\n"
        )));
    }
}

#[test]
fn stringify_macro_token_trees_do_not_supply_assertions() {
    for body in [
        "stringify!(assert_eq!(export_timestamp(30), 30));",
        "format!(\"{}\", export_timestamp(30)); assert_eq!(export_timestamp(30), 30);",
        "dbg!(export_timestamp(30)); assert_eq!(export_timestamp(30), 30);",
        "assert_eq!(export_timestamp(30), 30); stringify!(ignored);",
    ] {
        challenged_even_when_passed(&indexed(
            "tests/export.rs",
            &format!("#[test]\nfn timestamp() {{ {body} }}"),
        ));
    }
}

#[test]
fn unsupported_binding_forms_cannot_create_computed_evidence() {
    for (file, source) in [
        ("tests/export.rs", "#[test]\nfn timestamp() { let (export_timestamp, _) = (true, ()); assert!(export_timestamp); }"),
        ("tests/export.rs", "#[test]\nfn timestamp() { let (unused, _) = (true, ()); assert_eq!(export_timestamp(30), 30); }"),
        ("test_export.py", "def test_timestamp():\n    export_timestamp: bool = True\n    assert export_timestamp\n"),
        ("test_export.py", "def test_timestamp():\n    unused: bool = True\n    assert export_timestamp(30) == 30\n"),
        ("test_export.py", "def test_timestamp():\n    export_timestamp = other = True\n    assert export_timestamp == True\n"),
        ("test.js", "test('timestamp', () => { const [export_timestamp] = [true]; expect(export_timestamp).toBeTruthy(); });"),
    ] {
        challenged_even_when_passed(&indexed(file, source));
    }
}

#[test]
fn always_truthy_containers_do_not_establish_behavior() {
    for body in [
        "assert (export_timestamp(30), False)",
        "assert [export_timestamp(30)]",
        "assert export_timestamp(30) == [30]",
        "value = (export_timestamp(30), False)\n    assert value",
    ] {
        challenged_even_when_passed(&indexed(
            "test_export.py",
            &format!("def test_timestamp():\n    {body}\n"),
        ));
    }
    for body in [
        "expect([export_timestamp(30)]).toBeTruthy();",
        "expect({value: export_timestamp(30)}).toBeTruthy();",
        "const value = [export_timestamp(30)]; expect(value).toBeTruthy();",
        "expect([export_timestamp(30)]).toEqual([30]);",
    ] {
        challenged_even_when_passed(&indexed(
            "test.js",
            &format!("test('timestamp', () => {{ {body} }});"),
        ));
    }
}

#[test]
fn same_value_derivations_and_literal_constructors_are_not_evidence() {
    for body in [
        "let value = export_timestamp(30); assert_eq!(value, value + 0);",
        "let value = export_timestamp(30); let alias = value; assert_eq!(value, alias);",
        "let value = export_timestamp(30); assert_eq!(value.left(), value.right());",
        "let value = export_timestamp(30); assert_eq!(value.left(), wrap(value.right()));",
        "let export_timestamp = String::from(\"thirty\"); assert_eq!(export_timestamp.len(), 30);",
        "assert!(Some(export_timestamp(30)).is_some());",
        "let value = format!(\"{}\", export_timestamp(30)); assert_eq!(value.len(), 30);",
    ] {
        challenged_even_when_passed(&indexed(
            "tests/export.rs",
            &format!("#[test]\nfn timestamp() {{ {body} }}"),
        ));
    }
    for body in [
        "value = export_timestamp(30)\n    assert value == value + 0",
        "export_timestamp = bool(True)\n    assert export_timestamp == True",
        "assert len([export_timestamp(30)]) == 1",
    ] {
        challenged_even_when_passed(&indexed(
            "test_export.py",
            &format!("def test_timestamp():\n    {body}\n"),
        ));
    }
    for body in [
        "const value = export_timestamp(30); expect(value).toBe(value + 0);",
        "const export_timestamp = Boolean(true); expect(export_timestamp).toBeTruthy();",
    ] {
        challenged_even_when_passed(&indexed(
            "test.js",
            &format!("test('timestamp', () => {{ {body} }});"),
        ));
    }
}

#[test]
fn unsupported_assertion_messages_are_not_ignored() {
    challenged_even_when_passed(&indexed(
        "tests/export.rs",
        "#[test]\nfn timestamp() { assert_eq!(export_timestamp(30), 30, stringify!(ignored)); }",
    ));
    challenged_even_when_passed(&indexed(
        "test_export.py",
        "def test_timestamp():\n    assert export_timestamp(30) == 30, (lambda: True)\n",
    ));
    challenged_even_when_passed(&indexed(
        "test.js",
        "test('timestamp', () => { expect(30, export_timestamp(30)).toBe(30); });",
    ));
}
