//! Regressions for the layer-8 pipeline review findings.
use living_spec::{
    agents::test_index::TestIndex,
    core::{
        annotations::{Annotations, AuditVerdict},
        badge::{Badge, Reason},
        spec::Spec,
    },
    runner::rescore::StatusDocument,
};
use serde_json::json;
use std::{fs, path::Path, process::Command};

fn temp() -> tempfile::TempDir {
    tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap()
}

fn score(dir: &Path, command: &str, source: bool) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_living-spec"));
    cmd.arg(command)
        .arg(dir)
        .arg("--results")
        .arg(dir.join("results.xml"))
        .args(["--format", "junit", "--commit", "review-fixture"]);
    if source {
        cmd.arg("--tests").arg(dir);
    } else {
        cmd.arg("--fingerprints").arg(dir.join("fingerprints.json"));
    }
    cmd.output().unwrap()
}

fn status(dir: &Path) -> StatusDocument {
    serde_json::from_slice(&fs::read(dir.join("status.json")).unwrap()).unwrap()
}

fn results(dir: &Path, case: &str) {
    fs::write(
        dir.join("results.xml"),
        format!("<testsuite tests=\"1\">{case}</testsuite>"),
    )
    .unwrap();
}

#[test]
fn reauditing_does_not_transfer_a_human_override_to_changed_evidence() {
    for command in ["score", "rescore"] {
        for change in [
            "source",
            "requirement",
            "audit_revision",
            "audit_revision_after_unresolved",
            "unchanged",
        ] {
            let dir = temp();
            let root = dir.path();
            fs::create_dir(root.join("tests")).unwrap();
            let old_text = "- [R-001] Export timestamps.\n";
            let text = if change == "requirement" {
                "- [R-001] Export timestamp precision.\n"
            } else {
                old_text
            };
            fs::write(root.join("spec.md"), text).unwrap();
            let line = Spec::parse(text).unwrap().lines.remove(0);
            let old_line = Spec::parse(old_text).unwrap().lines.remove(0);
            let test_path = root.join("tests/test_exports.py");
            // Deliberately unrelated: accepting this binding requires a human decision.
            fs::write(
                &test_path,
                "def test_exports():\n    assert unrelated() == 7\n",
            )
            .unwrap();
            let old = TestIndex::build(root).unwrap().entries.remove(0);
            let revision = if change == "requirement" {
                &old_line.revision
            } else {
                &line.revision
            };
            let audit_revision = if change.starts_with("audit_revision") {
                "sha256:previous-audit"
            } else {
                revision
            };
            let decision = json!({"by":"human", "reason":"Reviewed the prior assertion.",
                "test_id":old.id, "fingerprint":old.fingerprint, "revision":revision});
            fs::write(
                root.join("annotations.json"),
                serde_json::to_vec(&json!({
                    "schema":"living-spec.annotations/v1", "requirements":{"R-001":{"links":[{
                        "test_id":old.id, "fingerprint":old.fingerprint, "revision":revision,
                        "explanation":"Synthetic review fixture", "proposed_by":"human",
                        "audit":{"verdict":"challenged", "revision":audit_revision, "notes":[],
                            "override":decision, "foreign":{"keep":true}}
                    }]}}
                }))
                .unwrap(),
            )
            .unwrap();
            if change == "source" {
                fs::write(&test_path, "def test_exports():\n    assert True\n").unwrap();
            }
            results(
                root,
                "<testcase classname=\"tests.test_exports\" name=\"test_exports\"/>",
            );
            if change == "audit_revision_after_unresolved" {
                fs::remove_file(&test_path).unwrap();
                let unresolved = score(root, command, true);
                assert_eq!(unresolved.status.code(), Some(2));
                fs::write(
                    &test_path,
                    "def test_exports():\n    assert unrelated() == 7\n",
                )
                .unwrap();
            }
            let output = score(root, command, true);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let doc = status(root);
            let annotations = Annotations::load(root.join("annotations.json")).unwrap();
            let audit = &annotations.requirements["R-001"].links[0].audit;
            assert_eq!(audit.verdict, AuditVerdict::Challenged);
            assert_eq!(audit.extra["foreign"], json!({"keep":true}));
            if change == "unchanged" {
                assert_eq!(doc.statuses[0].badge, Badge::Proven);
                assert_eq!(audit.extra["override"], decision);
                assert_eq!(doc.human_overrides, 1);
            } else {
                assert_eq!(
                    doc.statuses[0].badge,
                    Badge::Unproven,
                    "{command}: {change}"
                );
                assert!(!doc.statuses[0].tests[0].admissible);
                assert_eq!(doc.human_overrides, 0);
                assert!(!audit.extra.contains_key("override"));
                let previous = &audit.extra["invalidated_overrides"][0];
                assert_eq!(previous["override"], decision);
                assert_eq!(previous["fingerprint"], old.fingerprint);
                assert_eq!(previous["revision"], *revision);
                assert_eq!(previous["audit_revision"], audit_revision);
            }
        }
    }
}

#[test]
fn pipeline_quantifier_loading_rejects_duplicate_keys_before_writes() {
    let claim = r#"{"kind":"finite-universal","domain":{"type":"int-range","lo":-128,"hi":127},"subject":"abs_i8","predicate":{"version":1,"expression":{"op":"ge","left":{"kind":"field","source":"output"},"right":{"kind":"literal","value":{"type":"int","value":0}}}}}"#;
    for ambiguous in [
        claim.replace(
            "\"subject\":\"abs_i8\"",
            "\"subject\":\"abs_wrapping_i8\",\"subject\":\"abs_i8\"",
        ),
        claim.replace(
            "\"subject\":\"abs_i8\"",
            "\"subject\":\"abs_wrapping_i8\",\"subj\\u0065ct\":\"abs_i8\"",
        ),
        claim.replace("\"value\":0", "\"value\":-1,\"val\\u0075e\":0"),
    ] {
        let dir = temp();
        let root = dir.path();
        fs::write(
            root.join("spec.md"),
            "- [R-001] Synthetic absolute-value claim.\n",
        )
        .unwrap();
        let original = format!(
            r#"{{"schema":"living-spec.annotations/v1","requirements":{{"R-001":{{"quantifier":{ambiguous}}}}}}}"#
        );
        fs::write(root.join("annotations.json"), &original).unwrap();
        results(root, "<testcase name=\"fixture\"/>");
        fs::write(root.join("fingerprints.json"), "{}").unwrap();
        fs::write(root.join("status.json"), "prior status must survive").unwrap();
        fs::write(root.join("history.jsonl"), "prior history must survive\n").unwrap();
        for command in ["score", "rescore", "quantifier"] {
            let output = if command == "quantifier" {
                Command::new(env!("CARGO_BIN_EXE_living-spec"))
                    .args(["quantifier", "check"])
                    .arg(root)
                    .output()
                    .unwrap()
            } else {
                score(root, command, false)
            };
            assert_eq!(output.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&output.stderr).contains("object keys must be unique"));
            assert_eq!(
                fs::read_to_string(root.join("annotations.json")).unwrap(),
                original
            );
            assert_eq!(
                fs::read_to_string(root.join("status.json")).unwrap(),
                "prior status must survive"
            );
            assert_eq!(
                fs::read_to_string(root.join("history.jsonl")).unwrap(),
                "prior history must survive\n"
            );
            assert!(!root.join("spec.html").exists());
        }
        // Valid claims still score; pipeline orphan reporting is preserved.
        fs::remove_file(root.join("status.json")).unwrap();
        fs::remove_file(root.join("history.jsonl")).unwrap();
        fs::write(root.join("annotations.json"), format!(r#"{{"schema":"living-spec.annotations/v1","requirements":{{"R-001":{{"quantifier":{claim}}},"R-999":{{"foreign":true}}}}}}"#)).unwrap();
        let output = score(root, "score", false);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("orphan annotation: R-999"));
        assert_eq!(status(root).quantifier_reports.len(), 1);
    }
}

#[test]
fn generated_html_includes_inferred_boundary_cases() {
    for command in ["score", "rescore"] {
        let dir = temp();
        let root = dir.path();
        let text = "- [R-001] Delay is at most 30 s.\n";
        fs::write(root.join("spec.md"), text).unwrap();
        let revision = Spec::parse(text).unwrap().lines.remove(0).revision;
        let original = serde_json::to_vec(&json!({"schema":"living-spec.annotations/v1", "requirements":{
            "R-001":{"links":[{"test_id":"fixture", "fingerprint":"sha256:synthetic",
                "revision":revision, "explanation":"Synthetic review fixture", "proposed_by":"human",
                "audit":{"revision":revision,"verdict":"accepted","notes":[]}}]}
        }})).unwrap();
        fs::write(root.join("annotations.json"), &original).unwrap();
        fs::write(
            root.join("fingerprints.json"),
            r#"{"fixture":"sha256:synthetic"}"#,
        )
        .unwrap();
        results(root, "<testcase name=\"fixture\"/>");
        let output = score(root, command, false);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let doc = status(root);
        assert_eq!(doc.statuses[0].badge, Badge::Unproven);
        assert!(doc.statuses[0].reasons.contains(&Reason::BoundaryGap));
        assert_eq!(doc.boundary_reports[0].bounds[0].cases.len(), 3);
        let html = fs::read_to_string(root.join("spec.html")).unwrap();
        assert!(html.contains("Boundary coverage"));
        for value in ["29999 ms", "30000 ms", "30001 ms"] {
            assert!(html.contains(value), "missing {value}");
        }
        assert!(html.matches("untested").count() >= 3);
        assert_eq!(
            fs::read(root.join("annotations.json")).unwrap(),
            original,
            "rendering must not persist inferred bounds as authored overrides"
        );
    }
}
