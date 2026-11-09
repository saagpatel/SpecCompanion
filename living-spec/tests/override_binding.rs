//! Human decisions must never transfer across a write command's evidence rebind.
use living_spec::{
    agents::test_index::TestIndex,
    core::{annotations::Annotations, badge::Badge, spec::Spec},
    runner::rescore::StatusDocument,
};
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

fn write_command(root: &Path, command: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .arg(command)
        .arg(root)
        .arg("--tests")
        .arg(root)
        .arg("--write")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(if command == "audit" { 1 } else { 0 }),
        "{command}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn binding_change(change: &str) {
    for command in ["audit", "link"] {
        for author in ["human", "linker"] {
            let dir = tempfile::tempdir_in("target").unwrap();
            let root = dir.path();
            let spec = "- [R-001] Export timestamps at 30.\n";
            fs::write(root.join("spec.md"), spec).unwrap();
            let revision = Spec::parse(spec).unwrap().lines.remove(0).revision;
            let source = "def test_export_timestamps():\n    assert unrelated(30) == 30\n";
            fs::write(root.join("test_exports.py"), source).unwrap();
            let entry = TestIndex::build(root).unwrap().entries.remove(0);
            let mut decision = json!({
                "by":"human", "reason":"Reviewed the original limitation.",
                "test_id":entry.id, "fingerprint":entry.fingerprint, "revision":revision,
                "foreign":{"retain":true}
            });
            if change == "unbound" {
                for key in ["test_id", "fingerprint", "revision"] {
                    decision.as_object_mut().unwrap().remove(key);
                }
            }
            let file = root.join("annotations.json");
            fs::write(
                &file,
                serde_json::to_vec(&json!({
                    "schema":"living-spec.annotations/v1", "requirements":{"R-001":{"links":[{
                        "test_id":entry.id, "fingerprint":entry.fingerprint,
                        "rename_hint":entry.rename_hint, "revision":revision,
                        "proposed_by":author, "explanation":"Synthetic reviewed link",
                        "audit":{"verdict":"challenged", "revision":revision, "notes":[],
                            "override":decision, "foreign":{"keep":true}}
                    }]}}
                }))
                .unwrap(),
            )
            .unwrap();
            match change {
                "source" => fs::write(
                    root.join("test_exports.py"),
                    source.replace("== 30", ">= 30"),
                )
                .unwrap(),
                "rename" => fs::write(
                    root.join("test_exports.py"),
                    source.replace("test_export_timestamps", "test_export_timestamps_renamed"),
                )
                .unwrap(),
                "requirement" => fs::write(
                    root.join("spec.md"),
                    "- [R-001] Export timestamps precisely at 30.\n",
                )
                .unwrap(),
                "unchanged" | "unbound" => (),
                _ => unreachable!(),
            }
            write_command(root, command);
            let once = fs::read(&file).unwrap();
            let saved = Annotations::load(&file).unwrap();
            let original = &saved.requirements["R-001"].links[0];
            assert_eq!(original.audit.extra["foreign"], json!({"keep":true}));
            if change == "unchanged" {
                assert_eq!(original.audit.extra["override"], decision);
                assert!(original.human_override_reason().is_some());
            } else {
                for link in &saved.requirements["R-001"].links {
                    assert!(
                        link.human_override_reason().is_none(),
                        "{command}/{author}/{change}"
                    );
                    assert!(!link.audit.extra.contains_key("override"));
                }
                assert_eq!(
                    original.audit.extra["invalidated_overrides"][0]["override"],
                    decision
                );
            }
            write_command(root, command);
            assert_eq!(fs::read(&file).unwrap(), once, "write is idempotent");

            // Read back through source-backed scoring, with passing outcomes.
            let current = TestIndex::build(root).unwrap().entries.remove(0);
            fs::write(
                root.join("results.xml"),
                format!(
                    "<testsuite><testcase classname=\"test_exports\" name=\"{}\"/></testsuite>",
                    current.name
                ),
            )
            .unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_living-spec"))
                .arg("score")
                .arg(root)
                .arg("--tests")
                .arg(root)
                .arg("--results")
                .arg(root.join("results.xml"))
                .args(["--format", "junit"])
                .output()
                .unwrap();
            assert!(
                matches!(output.status.code(), Some(0 | 2)),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let status: StatusDocument =
                serde_json::from_slice(&fs::read(root.join("status.json")).unwrap()).unwrap();
            assert_eq!(status.human_overrides, usize::from(change == "unchanged"));
            assert_eq!(
                status.statuses[0].badge == Badge::Proven,
                change == "unchanged",
                "{command}/{author}/{change}"
            );
        }
    }
}

#[test]
fn override_invalidated_when_test_changes() {
    binding_change("source");
}

#[test]
fn override_invalidated_on_rename() {
    binding_change("rename");
}

#[test]
fn override_invalidated_when_requirement_changes() {
    binding_change("requirement");
}

#[test]
fn override_survives_unchanged_binding_through_both_write_commands() {
    binding_change("unchanged");
}

#[test]
fn unbound_legacy_override_is_inactive_history() {
    binding_change("unbound");
}

#[test]
fn stored_override_requires_all_three_binding_fields_to_match() {
    let fixture: Value = json!({
        "test_id":"test", "fingerprint":"source", "revision":"line",
        "explanation":"Synthetic", "proposed_by":"human",
        "audit":{"verdict":"challenged", "revision":"line", "notes":[],
            "override":{"by":"human", "reason":"Reviewed", "test_id":"test",
                "fingerprint":"source", "revision":"line"}}
    });
    for field in ["test_id", "fingerprint", "revision"] {
        let mut mismatched = fixture.clone();
        mismatched["audit"]["override"][field] = json!("other");
        let link: living_spec::core::annotations::Link =
            serde_json::from_value(mismatched).unwrap();
        assert!(!link.is_accepted(), "{field}");
    }
}

#[test]
fn stale_override_is_not_rendered_as_live_acceptance() {
    let spec = Spec::parse("- [R-001] Export timestamps.").unwrap();
    let revision = &spec.lines[0].revision;
    let annotations: Annotations = serde_json::from_value(json!({
        "schema":"living-spec.annotations/v1", "requirements":{"R-001":{"links":[{
            "test_id":"test", "fingerprint":"source", "revision":revision,
            "explanation":"Synthetic", "proposed_by":"human",
            "audit":{"verdict":"challenged", "revision":revision, "notes":[],
                "override":{"by":"human", "reason":"Reviewed", "test_id":"test",
                    "fingerprint":"source", "revision":revision}}
        }]}}
    }))
    .unwrap();
    let label = "accepted by human override: Reviewed";
    assert!(living_spec::web::render_spec(&spec, &annotations, None, &[]).contains(label));
    for text in [
        "- [R-001] Export timestamps precisely.",
        "- [R-002] Other requirement.",
    ] {
        let changed = Spec::parse(text).unwrap();
        assert!(!living_spec::web::render_spec(&changed, &annotations, None, &[]).contains(label));
    }
}
