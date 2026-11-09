use living_spec::{
    adapters::{parse_junit, parse_libtest},
    core::{
        annotations::Annotations,
        badge::{Badge, Reason},
        providers::NoBoundaryEvidence,
        test_identity::{load_fingerprints, Fingerprints, TestStatus},
    },
    runner::rescore::{rescore_with_providers, NoQualification, Result},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/runner")
        .join(name)
}

#[test]
fn adapter_rejects_duplicate_ids() {
    use living_spec::adapters::parse_junit_with;

    for (first, second) in [("ok", "FAILED"), ("FAILED", "ok"), ("ok", "ok")] {
        let input = format!("running 2 tests\ntest same ... {first}\ntest same ... {second}\n");
        assert!(parse_libtest(&input)
            .unwrap_err()
            .to_string()
            .contains("duplicate libtest id"));
    }
    for result in ["", "<failure/>", "<error/>"] {
        let input = format!(
            "<testsuite><testcase name=\"same\"/><testcase name=\"same\">{result}</testcase></testsuite>"
        );
        assert!(parse_junit(&input)
            .unwrap_err()
            .to_string()
            .contains("duplicate JUnit test id"));
    }
    let mapped = "<testsuite><testcase name=\"one\"/><testcase name=\"two\"/></testsuite>";
    assert!(parse_junit_with(mapped, |_, _| "same".into())
        .unwrap_err()
        .to_string()
        .contains("duplicate JUnit test id"));

    let cases = parse_libtest("running 2 tests\ntest same#one ... ok\ntest same#two ... ok\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n").unwrap();
    assert_eq!(cases.len(), 2);
    assert_ne!(cases[0].test_id, cases[1].test_id);
    let cases = parse_junit(
        "<testsuite><testcase name=\"same#one\"/><testcase name=\"same#two\"/></testsuite>",
    )
    .unwrap();
    assert_eq!(cases.len(), 2);
    assert_ne!(cases[0].test_id, cases[1].test_id);
    assert!(parse_libtest("running 2 tests\ntest same#one ... ok\ntest same#one ... FAILED\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n")
        .unwrap_err()
        .to_string()
        .contains("duplicate libtest id"));
    assert!(parse_junit(
        "<testsuite><testcase name=\"same#one\"/><testcase name=\"same#one\"/></testsuite>"
    )
    .is_err());
}
fn copy_spec() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
    for name in ["spec.md", "annotations.json"] {
        fs::copy(fixture("spec").join(name), dir.path().join(name)).unwrap();
    }
    dir
}
fn fingerprints() -> Fingerprints {
    load_fingerprints(&fs::read_to_string(fixture("fingerprints.json")).unwrap()).unwrap()
}
fn rescore(
    dir: &Path,
    outcomes: &[living_spec::core::test_identity::TestOutcome],
    fingerprints: &Fingerprints,
    commit: &str,
    note: &str,
) -> living_spec::runner::rescore::Result<living_spec::runner::rescore::RescoreReport> {
    rescore_with_providers(
        dir,
        outcomes,
        fingerprints,
        commit,
        note,
        &NoBoundaryEvidence,
        &NoQualification,
    )
}

#[test]
fn finding_1_namespaced_xml_attributes_cannot_green_an_unexecuted_test() {
    for attributes in [
        "name=\"other\" q:name=\"timestamps\"",
        "q:name=\"timestamps\" name=\"other\"",
    ] {
        let xml = format!(
            "<testsuite xmlns:q=\"urn:extension\" tests=\"2\">\n\
             <testcase classname=\"rust:fixture:tests/export.rs:exports\" {attributes}/>\n\
             <testcase classname=\"rust:fixture:tests/export.rs:exports\" name=\"order\"/>\n\
             </testsuite>"
        );
        let outcomes = parse_junit(&xml).unwrap();
        assert_eq!(
            outcomes[0].test_id,
            "rust:fixture:tests/export.rs:exports::other"
        );
        assert!(!outcomes.iter().any(|o| o.test_id.ends_with("::timestamps")));
        let dir = copy_spec();
        let report = rescore(
            dir.path(),
            &outcomes,
            &fingerprints(),
            "fixture",
            "finding 1",
        )
        .unwrap();
        assert_eq!(report.statuses[0].badge, Badge::Unproven);
        assert_eq!(report.statuses[0].reasons, [Reason::NotExecuted]);
        let persisted: living_spec::runner::rescore::StatusDocument =
            serde_json::from_slice(&fs::read(dir.path().join("status.json")).unwrap()).unwrap();
        assert_eq!(persisted.statuses[0].badge, Badge::Unproven);
        assert_eq!(persisted.statuses[1].badge, Badge::Proven);
    }
    // Every supported attribute uses its exact name, regardless of order.
    let outcomes = parse_junit(
        "<testsuite xmlns:q=\"urn:extension\" tests=\"1\" q:tests=\"99\" failures=\"0\" q:failures=\"99\" errors=\"0\" q:errors=\"99\" skipped=\"0\" q:skipped=\"99\">\
         <testcase classname=\"actual\" q:classname=\"alias\" name=\"case\" time=\"0.01\" q:time=\"invalid\"/>\
         </testsuite>",
    )
    .unwrap();
    assert_eq!(outcomes[0].test_id, "actual::case");
    assert_eq!(outcomes[0].duration_ms, Some(10));
    assert!(parse_junit(
        "<testsuite xmlns:q=\"urn:extension\"><testcase q:name=\"alias\"/></testsuite>"
    )
    .is_err());
    assert!(parse_junit("<testsuite><testcase name=\"one\" name=\"two\"/></testsuite>").is_err());
}

#[test]
fn finding_4_truncated_next_target_is_not_a_complete_passing_capture() {
    let passing = "Running tests/a.rs (target/debug/deps/a-123abc)\n\
                   running 1 test\n\
                   test pass ... ok\n\
                   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
    let next = "Running tests/b.rs (target/debug/deps/b-456def)\n";
    assert!(parse_libtest(&format!("{passing}{next}")).is_err());
    assert!(parse_libtest(&format!("{passing}{next}{next}")).is_err());
    assert!(parse_libtest(&format!("{next}{passing}")).is_err());
    assert!(parse_libtest(&format!("{passing}Doc-tests fixture\n")).is_err());
    assert!(parse_libtest(&format!("{passing}{next}running 0 tests\n")).is_err());

    let zero_suite = "running 0 tests\n\
                      test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
    let outcomes = parse_libtest(&format!("{passing}{next}{zero_suite}")).unwrap();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].test_id, "libtest:a:tests/a.rs:pass");
    // Standalone harness captures still work without Cargo target headers.
    assert_eq!(
        parse_libtest(&fs::read_to_string(fixture("before.txt")).unwrap())
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn unresolved_link_is_broken_never_dropped() {
    let dir = copy_spec();
    let mut outcomes = parse_libtest(&fs::read_to_string(fixture("before.txt")).unwrap()).unwrap();
    let missing = outcomes.remove(0).test_id;
    let mut fp = fingerprints();
    fp.remove(&missing);
    let report = rescore(dir.path(), &outcomes, &fp, "missing-commit", "fixture").unwrap();
    assert_eq!(report.exit_code(), 2);
    assert_eq!(report.unresolved[0].test_id, missing);
    assert_eq!(report.statuses[0].reasons, [Reason::LinkUnresolved]);
    assert_eq!(
        Annotations::load(dir.path().join("annotations.json"))
            .unwrap()
            .requirements["R-001"]
            .links
            .len(),
        1
    );
}

#[test]
fn adapters_preserve_case_statuses() {
    let result = parse_junit("<testsuite><testcase name=\"fails\"><failure/></testcase><testcase name=\"errors\"><error/></testcase><testcase name=\"skips\"><skipped/></testcase></testsuite>").unwrap();
    assert_eq!(
        result.iter().map(|o| o.status).collect::<Vec<_>>(),
        [TestStatus::Failed, TestStatus::Errored, TestStatus::Skipped]
    );
}
#[cfg(unix)]
#[test]
fn qualification_provider_preserves_test_outcomes() {
    use living_spec::runner::rescore::{NoBoundaryEvidence, QualifiedProvider};
    struct Qualify;
    impl QualifiedProvider for Qualify {
        fn qualify(
            &self,
            _: &living_spec::core::spec::Spec,
            _: &Annotations,
            statuses: &mut [living_spec::core::badge::LineStatus],
        ) -> Result<()> {
            if statuses[0].badge == Badge::Proven {
                statuses[0].badge = Badge::Qualified;
                statuses[0].reasons.push(Reason::PrerequisiteBroken {
                    path: vec!["R-001".into(), "R-002".into()],
                });
            }
            Ok(())
        }
    }
    let dir = copy_spec();
    let outcomes = parse_libtest(&fs::read_to_string(fixture("before.txt")).unwrap()).unwrap();
    let report = rescore_with_providers(
        dir.path(),
        &outcomes,
        &fingerprints(),
        "commit",
        "fixture",
        &NoBoundaryEvidence,
        &Qualify,
    )
    .unwrap();
    assert_eq!(report.statuses[0].badge, Badge::Qualified);
    assert_eq!(report.statuses[0].tests[0].status, TestStatus::Passed);
}

#[test]
fn qualification_cannot_hide_a_broken_line() {
    use living_spec::runner::rescore::{NoBoundaryEvidence, QualifiedProvider};
    struct HideFailure;
    impl QualifiedProvider for HideFailure {
        fn qualify(
            &self,
            _: &living_spec::core::spec::Spec,
            _: &Annotations,
            statuses: &mut [living_spec::core::badge::LineStatus],
        ) -> Result<()> {
            statuses[0].badge = Badge::Proven;
            Ok(())
        }
    }
    let dir = copy_spec();
    let outcomes = parse_libtest(&fs::read_to_string(fixture("after.txt")).unwrap()).unwrap();
    assert!(rescore_with_providers(
        dir.path(),
        &outcomes,
        &fingerprints(),
        "commit",
        "fixture",
        &NoBoundaryEvidence,
        &HideFailure
    )
    .is_err());
    assert!(!dir.path().join("status.json").exists());
}
