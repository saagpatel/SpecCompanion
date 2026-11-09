use living_spec::{
    agents::{
        auditor::audit,
        linker::{propose, propose_with_options, Options},
        test_index::{TestEntry, TestIndex},
    },
    core::{
        annotations::{Audit, AuditVerdict, Link},
        spec::{revision, Spec},
    },
};
use std::{collections::BTreeMap, fs, path::Path};

fn entry(file: &str, source: &str) -> TestEntry {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
    let index = TestIndex::build(dir.path()).unwrap();
    assert_eq!(index.entries.len(), 1, "{source}");
    index.entries.into_iter().next().unwrap()
}
fn link(entry: &TestEntry) -> Link {
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
fn verdict(requirement: &str, entry: &TestEntry) -> Audit {
    let spec = Spec::parse(&format!("- [R-001] {requirement}")).unwrap();
    audit(&spec.lines[0], &link(entry), entry)
}
#[test]
fn indexes_all_languages_with_scoped_names_and_source_spans() {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("src/storage/mod.rs", r#"
// #[test] fn fake() {}
#[cfg(test)]
mod tests {
    #[cfg(test)] fn helper() {}
    #[test]
    fn export_timestamp() { assert_eq!(export_timestamp(), 30); }
    #[test] #[should_panic]
    fn rejects_export() { reject_export(); }
}
"#),
        ("tests/export.rs", "#[test]\nfn timestamp() { assert!(export_timestamp()); }"),
        ("tests/test_export.py", "class TestExport:\n    def test_timestamp(self):\n        assert export_timestamp() == 30\n"),
        ("tests/export.test.ts", r#"import { it, describe } from 'vitest';
// it("fake", () => { expect(true).toBe(true); });
describe("exports", () => {
  describe("timestamps", () => {
    it("preserves original", () => { expect(export_timestamp()).toBe(30); });
  });
});
"#),
        ("export.test.js", r#"const text = "test('fake')"; test("original timestamp", function() { assert.strictEqual(export_timestamp(), 30); });"#),
    ];
    for (file, source) in files {
        let path = dir.path().join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    let index = TestIndex::build(dir.path()).unwrap();
    let ids: Vec<_> = index.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "jest:root:export.test.js:original timestamp",
            "libtest:export:tests/export.rs:timestamp",
            "libtest:lib:src/lib.rs:storage::tests::export_timestamp",
            "libtest:lib:src/lib.rs:storage::tests::rejects_export",
            "pytest:tests:tests/test_export.py:TestExport::test_timestamp",
            "vitest:tests:tests/export.test.ts:exports > timestamps > preserves original",
        ]
    );
    for e in &index.entries {
        assert!(e.line > 0 && !e.source.is_empty() && !e.body.is_empty());
        assert!(e.fingerprint.starts_with("sha256:") && e.rename_hint.starts_with("sha256:"));
    }
    assert_eq!(index.entries[2].line, 6);
    assert!(index.entries[3].should_panic);
}

#[test]
fn raw_revisions_and_rename_identity_preserve_authored_bytes() {
    let spec = Spec::parse("- [R-001]   Count >= 3.  \n```\n- [R-002] ignored\n```\n").unwrap();
    assert_eq!(spec.lines.len(), 1);
    assert_eq!(spec.lines[0].text, "  Count >= 3.");
    assert_eq!(spec.lines[0].text.trim_start(), "Count >= 3.");
    assert_ne!(
        revision(&spec.lines[0].text),
        revision(spec.lines[0].text.trim_start())
    );
    assert_eq!(revision("a  "), revision("a"));
    let one = entry(
        "test.py",
        "def test_one():\n    assert export_timestamp('a  b')\n",
    );
    let renamed = entry(
        "test.py",
        "def test_two():\n    assert export_timestamp('a  b')\n",
    );
    let changed = entry(
        "test.py",
        "def test_one():\n    assert export_timestamp('a b')\n",
    );
    assert_eq!(one.rename_hint, renamed.rename_hint);
    assert_ne!(one.fingerprint, renamed.fingerprint);
    assert_ne!(one.rename_hint, changed.rename_hint);
    let result = audit(
        &Spec::parse("- [R-001] Export timestamps.").unwrap().lines[0],
        &link(&one),
        &changed,
    );
    assert_eq!(result.verdict, AuditVerdict::Challenged);
    assert!(result.notes.iter().any(|n| n.contains("identity changed")));
}

#[test]
fn rename_with_should_panic_does_not_match_index() {
    let old = entry("test.rs", "#[test]\nfn old() { assert!(value); }");
    let changed = entry(
        "test.rs",
        "#[test]\n#[should_panic]\nfn new() { assert!(value); }",
    );
    assert_ne!(old.rename_hint, changed.rename_hint);
}

#[test]
fn scoring_excludes_paths_comments_and_uncalled_identifiers() {
    let e = entry("export_timestamp/test.py", "def test_unrelated():\n    # export timestamp\n    export_timestamp = 1\n    assert unrelated() == 1\n");
    let spec = Spec::parse("- [R-001] Export timestamp.").unwrap();
    assert!(propose(&spec, &TestIndex { entries: vec![e] }).is_empty());
    let e = entry(
        "test.py",
        "def test_unrelated():\n    assert exportTimestamp() == 'original timestamp'\n",
    );
    let index = TestIndex {
        entries: vec![e.clone(), {
            let mut second = e.clone();
            second.id.push_str("_second");
            second
        }],
    };
    let p = propose_with_options(
        &spec,
        &index,
        Options {
            threshold: 2,
            top_k: 1,
        },
    );
    assert_eq!(p.len(), 1);
    assert!(
        p[0].explanation.contains("called identifiers")
            && p[0].explanation.contains("body literals")
    );
    assert_eq!(p[0].revision, revision(&spec.lines[0].text));
    assert!(propose_with_options(
        &spec,
        &index,
        Options {
            threshold: 99,
            top_k: 2
        }
    )
    .is_empty());
}

#[test]
fn seeded_proposals_resolve_explain_and_link_six_genuine_lines() {
    let root = Path::new("tests/fixtures/agents/seeded");
    let spec = Spec::parse(&fs::read_to_string(root.join("living-spec/spec.md")).unwrap()).unwrap();
    let index = TestIndex::build(root).unwrap();
    let proposals = propose(&spec, &index);
    assert_eq!(spec.lines.len(), 8);
    for p in &proposals {
        assert!(!p.explanation.is_empty());
        let e = index.get(&p.test_id).unwrap();
        assert_eq!(e.fingerprint, p.fingerprint);
        assert_eq!(e.rename_hint, p.rename_hint);
    }
    for (id, name) in [
        ("R-002", "test_sessions_expire_after_30_seconds"),
        ("R-003", "test_passwords_shorter_than_8_are_rejected"),
        ("R-004", "test_clamp_maximum_100"),
        ("R-005", "test_names_sort_alphabetically"),
        ("R-007", "test_csv_escapes_quotes"),
        ("R-008", "test_pagination_size_20"),
    ] {
        let p = proposals
            .iter()
            .find(|p| p.requirement == id && p.test_id.ends_with(name))
            .unwrap();
        let e = index.get(&p.test_id).unwrap();
        let line = spec.lines.iter().find(|r| r.id == id).unwrap();
        let result = audit(line, &link(e), e);
        assert_eq!(
            result.verdict,
            AuditVerdict::Accepted,
            "{id}: {:?}",
            result.notes
        );
        assert_eq!(result.revision, p.revision);
    }
}

#[test]
fn cheating_case_is_challenged() {
    let root = Path::new("tests/fixtures/agents/seeded");
    let index = TestIndex::build(root).unwrap();
    for (name, requirement, why) in [
        (
            "test_exports_keep_original_timestamps",
            "Exports keep original timestamps.",
            "placeholder",
        ),
        (
            "test_archives_retain_7_entries",
            "Archives retain 7 entries.",
            "unrelated",
        ),
    ] {
        let e = index.entries.iter().find(|e| e.name == name).unwrap();
        let result = verdict(requirement, e);
        assert_eq!(result.verdict, AuditVerdict::Challenged);
        assert!(
            result.notes.iter().any(|n| n.contains(why)),
            "{:?}",
            result.notes
        );
    }
}

#[test]
fn placeholder_regressions_from_desktop_and_rust_macros() {
    let cases = [
        ("py", "assert True"),
        ("py", "assert False"),
        ("py", "assert not False"),
        ("py", "assert not True"),
        ("py", "assert 1 == 1"),
        ("py", "assert 0 == 0"),
        ("py", "assert 2 + 2 == 4"),
        ("py", "assert export_timestamp == export_timestamp"),
        ("py", "assertTrue(True)"),
        ("py", "self.assertTrue(True)"),
        ("py", "assertEqual(True, True)"),
        ("py", "assertEqual(1, 1)"),
        ("js", "expect(true).toBe(true);"),
        ("js", "expect(false).toBe(false);"),
        ("js", "expect(1).toBe(1);"),
        ("js", "expect(2).toBe(3);"),
        (
            "js",
            "const actual = 1; expect(actual).toBe(export_timestamp());",
        ),
        ("js", "expect('export timestamp').toBe('export timestamp');"),
        ("js", "expect(null).toBe(null);"),
        ("js", "expect(undefined).toBe(undefined);"),
        ("js", "expect(export_timestamp()).toBe(export_timestamp());"),
        ("rs", "assert!(true);"),
        ("rs", "assert_eq!(1, 1);"),
        ("rs", "assert_ne!(1, 2);"),
        ("rs", "assert_eq!(export_timestamp(), export_timestamp());"),
        (
            "rs",
            "let export_timestamp = true; assert!(export_timestamp);",
        ),
    ];
    for (lang, body) in cases {
        let (file, source) = match lang {
            "py" => (
                "test.py",
                format!("def test_export_timestamp():\n    {body}\n"),
            ),
            "js" => (
                "test.js",
                format!("test('export timestamp', () => {{ {body} }});"),
            ),
            _ => (
                "tests/export.rs",
                format!("#[test]\nfn export_timestamp() {{ {body} }}"),
            ),
        };
        let result = verdict("Export timestamps.", &entry(file, &source));
        assert_eq!(
            result.verdict,
            AuditVerdict::Challenged,
            "{body}: {:?}",
            result.notes
        );
        assert!(
            result.notes.iter().any(|n| n.contains("placeholder")),
            "{body}: {:?}",
            result.notes
        );
    }
}

#[test]
fn supported_assertions_include_aliases_panics_raises_and_multiline() {
    let cases = [
        ("tests/export.rs", "#[test]\nfn export_timestamp() { let result = export_timestamp(30); assert_eq!(result, 30); }"),
        ("tests/export.rs", "#[test]\nfn export_timestamp() { assert_eq!(30, export_timestamp(30)); }"),
        ("tests/export.rs", "#[test]\nfn export_timestamp() { assert!(export_timestamp(30) > 0); }"),
        ("tests/export.rs", "#[test]\nfn export_timestamp() { assert!(export_timestamp(30) >= 0); }"),
        ("tests/export.rs", "#[test]\n#[should_panic]\nfn export_timestamp() { export_timestamp(30); }"),
        ("test.py", "def test_timestamp():\n    result = export_timestamp(30)\n    assert result == 30\n"),
        ("test.py", "def test_timestamp():\n    assert (\n        export_timestamp(30) == 30\n    )\n"),
        ("test.py", "def test_timestamp():\n    with pytest.raises(ValueError):\n        export_timestamp(30)\n"),
        ("test.py", "def test_timestamp():\n    with pytest.raises(ValueError):\n        export_timestamp(30)\n"),
        ("test.js", "test('timestamp', () => { const result = exportTimestamp(30); expect(result).toBe(30); });"),
        ("test.js", "it('timestamp', () => { expect(export_timestamp(30)).toBe(30); });"),
    ];
    for (file, source) in cases {
        let result = verdict("Export timestamps at 30.", &entry(file, source));
        assert_eq!(
            result.verdict,
            AuditVerdict::Accepted,
            "{source}: {:?}",
            result.notes
        );
    }
}

#[test]
fn allowlisted_shapes_keep_computed_values_and_independent_expectations() {
    let cases = [
        ("tests/export.rs", "#[test]\n#[should_panic]\nfn timestamp() { let value = 30; export_timestamp(value); }"),
        ("tests/export.rs", "#[test]\nfn timestamp() { assert!(matches!(export_timestamp(30), Result::Success(_))); }"),
        ("tests/export.rs", "#[test]\nfn timestamp() { assert!(matches!(export_timestamp(30), 20..=30)); }"),
        ("test.py", "@pytest.mark.parametrize('value', [30])\ndef test_timestamp(value):\n    assert export_timestamp(value) == 30\n"),
        ("test.js", "test('timestamp', () => { const value = export_timestamp(30); expect(value).toBeTruthy(); });"),
        ("test.js", "test('timestamp', () => { expect(export_timestamp(30)).toBeFalsy(); });"),
        ("test.js", "test('timestamp', () => { expect(export_timestamp(30)).toThrow(Error); });"),
    ];
    for (file, source) in cases {
        let result = verdict("Export timestamp at 30.", &entry(file, source));
        assert_eq!(
            result.verdict,
            AuditVerdict::Accepted,
            "{source}: {:?}",
            result.notes
        );
    }
    for matcher in [
        "toBe",
        "toEqual",
        "toStrictEqual",
        "toBeGreaterThan",
        "toBeGreaterThanOrEqual",
        "toBeLessThan",
        "toBeLessThanOrEqual",
    ] {
        let source = format!("test('timestamp', () => {{ const value = export_timestamp(30); expect(value).{matcher}(30); }});");
        assert_eq!(
            verdict("Export timestamp at 30.", &entry("test.js", &source)).verdict,
            AuditVerdict::Accepted,
            "{matcher}"
        );
    }
}

#[test]
fn ordinary_rust_assertions_follow_values_without_accepting_decoys() {
    // Every supported shape gets a genuine assertion and the same shape with an
    // unrelated subject. Test names, messages and unused locals are decoys.
    let forms = [
        "assert!(\n SUBJECT(30) > 0,\n \"export timestamp\"\n);",
        "assert_eq!(\n SUBJECT(30),\n 30,\n);",
        "assert_ne!(\n SUBJECT(30),\n 0,\n);",
        "assert!(matches!(\n SUBJECT(30),\n 30\n));",
        "assert!(SUBJECT(30).is_err());",
        "assert!(SUBJECT(30).is_ok());",
        "assert!(SUBJECT(30).is_some());",
        "assert!(SUBJECT(30).is_none());",
        "assert_eq!(SUBJECT(30).contains(&30), true);",
        "let error = SUBJECT(30).unwrap_err(); assert_eq!(error.code(), 30);",
        "let mut result = SUBJECT(30); let alias = result; let compared = alias + 1; assert_eq!(compared, 31);",
    ];
    for form in forms {
        for (subject, expected) in [
            ("export_timestamp", AuditVerdict::Accepted),
            ("unrelated", AuditVerdict::Challenged),
        ] {
            let source = format!(
                "#[test]\nfn export_timestamp() {{ let unused_export_timestamp = 30; {} }}",
                form.replace("SUBJECT", subject)
            );
            let result = verdict(
                "Export timestamps at 30.",
                &entry("tests/export.rs", &source),
            );
            assert_eq!(result.verdict, expected, "{source}: {:?}", result.notes);
        }
    }
    for (subject, expected) in [
        ("export_timestamp", AuditVerdict::Accepted),
        ("unrelated", AuditVerdict::Challenged),
    ] {
        let source =
            format!("#[test]\n#[should_panic]\nfn export_timestamp() {{ {subject}(30); }}");
        assert_eq!(
            verdict(
                "Export timestamps at 30.",
                &entry("tests/export.rs", &source)
            )
            .verdict,
            expected
        );
    }
    // String literals in asserted values count; messages, pattern names and
    // constant aliases do not turn placeholders into evidence.
    for (body, expected) in [
        (
            "assert_eq!(output().contains(\"export timestamp\"), true);",
            AuditVerdict::Accepted,
        ),
        (
            "let name = \"export timestamp\"; assert_eq!(output().contains(name), true);",
            AuditVerdict::Accepted,
        ),
        (
            "assert!(matches!(unrelated(), ExportTimestamp));",
            AuditVerdict::Challenged,
        ),
        (
            "assert!(matches!(export_timestamp(), _));",
            AuditVerdict::Challenged,
        ),
        (
            "let export_timestamp: bool = true; assert!(export_timestamp);",
            AuditVerdict::Challenged,
        ),
        (
            "let x = export_timestamp(); let x = true; assert!(x);",
            AuditVerdict::Challenged,
        ),
        (
            "let x = export_timestamp(); assert_eq!(x, x);",
            AuditVerdict::Challenged,
        ),
    ] {
        let source = format!("#[test]\nfn export_timestamp() {{ {body} }}");
        assert_eq!(
            verdict("Export timestamps.", &entry("tests/export.rs", &source)).verdict,
            expected,
            "{body}"
        );
    }
}

#[test]
fn rust_python_and_javascript_index_entries_have_scoped_spans() {
    let expected_source_contains_assertion = true;
    let rust = entry(
        "tests/export.rs",
        "#[test]\nfn timestamp() { assert!(export_timestamp()); }",
    );
    let python = entry(
        "tests/test_export.py",
        "def test_timestamp():\n    assert export_timestamp()\n",
    );
    let javascript = entry(
        "tests/export.test.js",
        "test('timestamp', () => { expect(exportTimestamp()).toBe(30); });",
    );
    assert_eq!(rust.id, "libtest:export:tests/export.rs:timestamp");
    assert_eq!(rust.line, 1);
    assert_eq!(
        rust.source.contains("assert!(export_timestamp())"),
        expected_source_contains_assertion
    );
    assert_eq!(
        python.id,
        "pytest:tests:tests/test_export.py:test_timestamp"
    );
    assert_eq!(python.line, 1);
    assert_eq!(
        python.source.contains("assert export_timestamp()"),
        expected_source_contains_assertion
    );
    assert_eq!(javascript.id, "jest:tests:tests/export.test.js:timestamp");
    assert_eq!(javascript.line, 1);
    assert_eq!(
        javascript.source.contains("expect(exportTimestamp())"),
        expected_source_contains_assertion
    );
}

#[test]
fn rust_irrelevant_and_unsupported_assertions_are_challenged() {
    let irrelevant = entry(
        "tests/export.rs",
        "#[test]\nfn timestamp() { export_timestamp(30); assert_eq!(unrelated(), 30); }",
    );
    let unsupported = entry(
        "tests/export.rs",
        "#[test]\nfn timestamp() { assert!(true || export_timestamp(30)); }",
    );
    let placeholder = entry(
        "tests/export.rs",
        "#[test]\nfn timestamp() { assert!(true); }",
    );
    assert_eq!(
        verdict("Export timestamp is 30.", &irrelevant).verdict,
        AuditVerdict::Challenged
    );
    assert_eq!(
        verdict("Export timestamp is 30.", &unsupported).verdict,
        AuditVerdict::Challenged
    );
    assert_eq!(
        verdict("Export timestamp is 30.", &placeholder).verdict,
        AuditVerdict::Challenged
    );
}

#[test]
fn linker_proposal_resolves_and_explains_a_genuine_timestamp_test() {
    let expected_empty_explanation = false;
    let root = Path::new("tests/fixtures/agents/seeded");
    let index = TestIndex::build(root).unwrap();
    let spec = Spec::parse("- [R-001] Sessions expire after 30 seconds.").unwrap();
    let proposals = propose(&spec, &index);
    let proposal = &proposals[0];
    let entry = index.get(&proposal.test_id).unwrap();
    assert_eq!(entry.name, "test_sessions_expire_after_30_seconds");
    assert_eq!(entry.fingerprint, proposal.fingerprint);
    assert_eq!(proposal.explanation.is_empty(), expected_empty_explanation);
    assert_eq!(
        audit(&spec.lines[0], &link(entry), entry).verdict,
        AuditVerdict::Accepted
    );
}

#[test]
fn missing_irrelevant_numeric_and_unsupported_assertions_are_challenged() {
    let cases = [
        ("test.py", "def test_timestamp():\n    export_timestamp(30)\n", "no supported assertion"),
        ("test.py", "def test_timestamp():\n    assert unrelated() == 30\n", "unrelated"),
        ("test.py", "def test_timestamp():\n    assert export_timestamp(10) > 0\n", "required number"),
        ("test.py", "def test_timestamp():\n    assert export_timestamp(30.5) > 0\n", "required number"),
        ("test.py", "def test_timestamp():\n    assert export_timestamp() == True, '30'\n", "required number"),
        ("test.py", "def test_timestamp():\n    # assert export_timestamp(30)\n    print('assert export_timestamp(30)')\n", "no supported assertion"),
        ("test.js", "test('timestamp', () => { expect(export_timestamp(30)).customMatcher(); });", "unsupported test shape"),
        ("test.py", "def test_timestamp():\n    assert (export_timestamp(30) if flag else True)\n", "unsupported test shape"),
        ("tests/export.rs", "#[test]\nfn timestamp() { custom_assert!(export_timestamp(30)); }", "no supported assertion"),
    ];
    for (file, source, why) in cases {
        let result = verdict("Export timestamps at 30.", &entry(file, source));
        assert_eq!(result.verdict, AuditVerdict::Challenged);
        assert!(
            result.notes.iter().any(|n| n.contains(why)),
            "{source}: {:?}",
            result.notes
        );
    }
}

#[test]
fn source_resolution_distinguishes_changed_renamed_and_ambiguous() {
    use living_spec::agents::test_index::SourceResolution;
    let one = entry(
        "test.py",
        "def test_one():\n    assert export_timestamp()\n",
    );
    let renamed = entry(
        "test.py",
        "def test_two():\n    assert export_timestamp()\n",
    );
    let changed = entry(
        "test.py",
        "def test_one():\n    assert export_timestamp() == 30\n",
    );
    let old = link(&one);
    assert!(matches!(
        TestIndex { entries: vec![one] }.resolve(&old),
        SourceResolution::Exact(_)
    ));
    assert!(matches!(
        TestIndex {
            entries: vec![changed]
        }
        .resolve(&old),
        SourceResolution::Changed(_)
    ));
    assert!(matches!(
        TestIndex {
            entries: vec![renamed.clone()]
        }
        .resolve(&old),
        SourceResolution::Renamed(_)
    ));
    let mut duplicate = renamed.clone();
    duplicate.id.push_str("_other");
    assert!(matches!(
        TestIndex {
            entries: vec![renamed, duplicate]
        }
        .resolve(&old),
        SourceResolution::Unresolved
    ));
    assert!(matches!(
        TestIndex::default().resolve(&old),
        SourceResolution::Unresolved
    ));
}

#[test]
fn multiline_python_signatures_and_literal_only_discovery() {
    let e = entry("tests/test_export.py", "class TestExport:\n    def test_timestamp(\n        self,\n    ):\n        assert export_timestamp() == 30\n");
    assert_eq!(e.name, "TestExport::test_timestamp");
    assert_eq!(
        verdict("Export timestamp is 30.", &e).verdict,
        AuditVerdict::Accepted
    );
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("test.py"),
        "text = \"\"\"\ndef test_fake():\n    assert export()\n\"\"\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("test.js"),
        "test('object', { fake: expect(export()).toBe(30) });",
    )
    .unwrap();
    fs::write(
        dir.path().join("test.rs"),
        "const S: &str = r###\"some \"quote\" #[test] fn fake() { assert!(export()); }\"###;",
    )
    .unwrap();
    assert!(TestIndex::build(dir.path()).unwrap().entries.is_empty());
}

#[test]
fn unsupported_control_flow_and_manual_panics_never_accept() {
    for (file, source) in [
        ("test.py", "def test_timestamp():\n    if False:\n        assert export_timestamp(30)\n"),
        ("test.py", "def test_timestamp():\n    assert True or export_timestamp(30)\n"),
        ("test.js", "test('timestamp', () => { return; expect(export_timestamp(30)).toBe(30); });"),
        ("test.js", "test('timestamp', () => { expect(flag ? true : export_timestamp(30)).toBe(true); });"),
        ("tests/export.rs", "#[test]\n#[should_panic]\nfn test_timestamp() { export_timestamp(30); panic!(\"fake\"); }"),
        ("tests/export.rs", "#[test]\nfn test_timestamp() { assert!(true || export_timestamp(30)); }"),
        ("test.py", "@pytest.mark.parametrize('value', [30])\ndef test_timestamp(value):\n    assert export_timestamp(value)\n"),
    ] {
        let result = verdict("Export timestamp is 30.", &entry(file, source));
        assert_eq!(result.verdict, AuditVerdict::Challenged, "{source}: {:?}", result.notes);
        assert!(result.notes.iter().any(|n| n.contains("unsupported")), "{source}: {:?}", result.notes);
    }
}

#[test]
fn index_and_adapter_ids_agree() {
    use living_spec::adapters::{parse_junit_with, parse_libtest};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let index = TestIndex::build(&root.join("tests")).unwrap();
    let entry = index
        .entries
        .iter()
        .find(|entry| entry.name == "index_and_adapter_ids_agree")
        .unwrap();
    let cargo_output = "     Running tests/agents.rs (target/debug/deps/agents-012abc)\nrunning 1 test\ntest index_and_adapter_ids_agree ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";
    let outcomes = parse_libtest(cargo_output).unwrap();
    assert_eq!(
        entry.id,
        "libtest:agents:tests/agents.rs:index_and_adapter_ids_agree"
    );
    assert_eq!(outcomes[0].test_id, entry.id);
    let full_index = TestIndex::build(root).unwrap();
    assert_eq!(full_index.get(&entry.id), Some(entry));
    let unit_output = "Running unittests src/lib.rs (target/debug/deps/living_spec-012abc)\nrunning 1 test\ntest adapters::tests::libtest_parses_pass_failure_skip_and_failure_detail ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";
    let unit = parse_libtest(unit_output).unwrap();
    let indexed_unit = full_index.get(&unit[0].test_id).unwrap();
    assert_eq!(indexed_unit.file, "src/adapters/mod.rs");
    let binary = parse_libtest("Running unittests src/main.rs (target/debug/deps/living_spec-012abc)\nrunning 1 test\ntest fixture ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n").unwrap();
    assert_eq!(binary[0].test_id, "libtest:living_spec:src/main.rs:fixture");
    // JUnit needs explicit framework/target/file metadata or a supported file/class mapping.
    let junit =
        "<testsuite tests=\"1\"><testcase classname=\"libtest:agents:tests/agents.rs\" name=\"index_and_adapter_ids_agree\"/></testsuite>";
    let outcomes = parse_junit_with(junit, |class, name| index.junit_id(class, name)).unwrap();
    assert_eq!(outcomes[0].test_id, entry.id);
}

#[test]
fn junit_source_mapping_requires_scoped_or_unique_file_class_metadata() {
    let e = entry("tests/test_export.py", "class TestExport:\n    def test_timestamp(self):\n        assert export_timestamp() == 30\n");
    let index = TestIndex {
        entries: vec![e.clone()],
    };
    assert_eq!(
        index.junit_id("tests.test_export.TestExport", "test_timestamp"),
        e.id
    );
    assert_eq!(
        index.junit_id("unrelated.TestExport", "test_timestamp"),
        "unrelated.TestExport::test_timestamp"
    );
    assert_eq!(index.junit_id("", "test_timestamp"), "test_timestamp");
    let mut duplicate = e.clone();
    duplicate.id.push_str("#other");
    let ambiguous = TestIndex {
        entries: vec![e, duplicate],
    };
    assert_eq!(
        ambiguous.junit_id("tests.test_export.TestExport", "test_timestamp"),
        "tests.test_export.TestExport::test_timestamp"
    );
}

#[test]
fn junit_each_titles_map_only_unambiguous_scoped_cases() {
    for (file, source, scope) in [
        ("tests/example.js", "test.each([[30], [31]])('timestamp %i', (value) => { expect(timestamp(value)).toBe(value); });", "jest:tests:tests/example.js"),
        ("tests/example.ts", "import { test } from 'vitest';\ntest.each([[30]])('timestamp %i', (value) => { expect(timestamp(value)).toBe(value); });", "vitest:tests:tests/example.ts"),
    ] {
        let e = entry(file, source);
        let index = TestIndex { entries: vec![e.clone()] };
        assert_eq!(index.junit_id(scope, "timestamp 30"), format!("{}#30", e.id));
        assert_eq!(index.junit_id(scope, "timestamp unknown"), format!("{scope}::timestamp unknown"));
        assert_eq!(index.junit_id("", "timestamp 30"), "timestamp 30");
        let ambiguous = TestIndex { entries: vec![e.clone(), e] };
        assert_eq!(ambiguous.junit_id(scope, "timestamp 30"), format!("{scope}::timestamp 30"));
    }
}
