use living_spec::{
    adapters::{parse_junit, parse_libtest},
    core::{
        annotations::Annotations,
        badge::Badge,
        freshness::{Freshness, ResultsSnapshot},
        providers::NoBoundaryEvidence,
        spec::Spec,
        test_identity::{load_fingerprints, Fingerprints, TestOutcome},
    },
    runner::rescore::{
        rescore_with_context, rescore_with_providers, NoQualification, RescoreContext, Result,
        StatusDocument,
    },
};
use std::{collections::BTreeMap, fs, path::Path};

const A: &str = "rust:fixture:tests/export.rs:exports::timestamps";
const B: &str = "rust:fixture:tests/export.rs:exports::order";
const RESULTS: &str = include_str!("fixtures/runner/before.txt");

fn fixture() -> (tempfile::TempDir, Fingerprints) {
    let dir = tempfile::tempdir_in("target").unwrap();
    fs::write(
        dir.path().join("spec.md"),
        include_str!("fixtures/runner/spec/spec.md"),
    )
    .unwrap();
    fs::write(
        dir.path().join("annotations.json"),
        include_str!("fixtures/runner/spec/annotations.json"),
    )
    .unwrap();
    fs::write(dir.path().join("results.txt"), RESULTS).unwrap();
    let identities = load_fingerprints(include_str!("fixtures/runner/fingerprints.json")).unwrap();
    (dir, identities)
}

fn freshness(dir: &Path) -> Freshness {
    let document: StatusDocument =
        serde_json::from_slice(&fs::read(dir.join("status.json")).unwrap()).unwrap();
    let spec = Spec::parse(&fs::read_to_string(dir.join("spec.md")).unwrap()).unwrap();
    document
        .inputs
        .unwrap()
        .check(
            dir,
            &spec,
            &document.statuses,
            &document.boundary_reports,
            &document.quantifier_reports,
        )
        .unwrap()
}

fn score_file(dir: &Path, identities: &Fingerprints, file: &Path) {
    rescore_with_providers(
        dir,
        &parse_libtest(RESULTS).unwrap(),
        identities,
        "fixture",
        file.to_str().unwrap(),
        &NoBoundaryEvidence,
        &NoQualification,
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn retargeted_results_symlink_is_stale() {
    use std::os::unix::fs::symlink;

    let (dir, identities) = fixture();
    let link = dir.path().join("latest.txt");
    symlink("results.txt", &link).unwrap();
    score_file(dir.path(), &identities, &link);
    let document: StatusDocument =
        serde_json::from_slice(&fs::read(dir.path().join("status.json")).unwrap()).unwrap();
    assert_eq!(
        document.inputs.unwrap().results_file.as_deref(),
        Some("latest.txt")
    );
    assert!(!freshness(dir.path()).stale);

    let failed = RESULTS
        .replace(&format!("{B} ... ok"), &format!("{B} ... FAILED"))
        .replace(
            "test result: ok. 2 passed; 0 failed",
            "test result: FAILED. 1 passed; 1 failed",
        );
    fs::write(dir.path().join("failed.txt"), failed).unwrap();
    fs::remove_file(&link).unwrap();
    symlink("failed.txt", &link).unwrap();
    let current = freshness(dir.path());
    assert!(current.stale);
    assert!(current.lines.contains_key("R-002"));
}

#[test]
fn missing_results_file_is_stale() {
    let (dir, identities) = fixture();
    let file = dir.path().join("results.txt");
    score_file(dir.path(), &identities, &file);
    assert!(!freshness(dir.path()).stale);
    fs::remove_file(file).unwrap();
    let current = freshness(dir.path());
    assert!(current.stale);
    assert_eq!(current.lines.len(), 2);
}

#[test]
fn spec_change_during_scoring_is_not_fresh() {
    // The provider runs after the input reads and badge computation, immediately
    // before capture. Each replacement must be compared to the scored snapshot.
    for input in ["spec.md", "annotations.json", "results.txt"] {
        let (dir, identities) = fixture();
        let outcomes = parse_libtest(RESULTS).unwrap();
        let changed_dir = dir.path().to_path_buf();
        let mutate = move |_: &Spec, _: &Annotations| -> Result<Vec<_>> {
            let path = changed_dir.join(input);
            let replacement = match input {
                "spec.md" => fs::read_to_string(&path)?.replace("original", "updated"),
                "annotations.json" => fs::read_to_string(&path)?.replace("accepted", "challenged"),
                _ => RESULTS
                    .replace(&format!("{B} ... ok"), &format!("{B} ... FAILED"))
                    .replace(
                        "test result: ok. 2 passed; 0 failed",
                        "test result: FAILED. 1 passed; 1 failed",
                    ),
            };
            fs::write(path, replacement)?;
            Ok(Vec::new())
        };
        let report = rescore_with_context(
            dir.path(),
            &outcomes,
            &identities,
            "fixture",
            dir.path().join("results.txt").to_str().unwrap(),
            RescoreContext {
                results: None,
                boundary: &NoBoundaryEvidence,
                qualified: &NoQualification,
                reauditor: None,
                quantifier_reports: &mutate,
            },
        )
        .unwrap();
        assert!(report
            .statuses
            .iter()
            .all(|line| line.badge == Badge::Proven));
        let current = freshness(dir.path());
        assert!(current.stale, "replacement of {input} must be stale");
        assert!(!current.lines.is_empty(), "{input}");
        if input == "results.txt" {
            assert!(!current.lines.contains_key("R-001"));
            assert!(current.lines.contains_key("R-002"));
        }
    }
}

#[test]
fn results_file_mismatch_is_rejected() {
    let outcomes = parse_libtest(RESULTS).unwrap();
    let replacements = [
        "unparseable results".to_owned(),
        RESULTS.replace(A, "rust:fixture:tests/export.rs:exports::unrelated"),
        RESULTS.replace(&format!("{B} ... ok"), &format!("{A} ... ok")),
        format!("<testsuite><testcase name=\"{A}\"/></testsuite>"),
        format!("<testsuite><testcase name=\"{A}\"/><testcase name=\"{B}\"><failure/></testcase></testsuite>"),
        format!("<testsuite><testcase name=\"{A}\" time=\"0.1\"/><testcase name=\"{B}\"/></testsuite>"),
    ];
    for replacement in replacements {
        let (dir, identities) = fixture();
        let original_annotations = fs::read(dir.path().join("annotations.json")).unwrap();
        fs::write(dir.path().join("results.txt"), replacement).unwrap();
        let error = rescore_with_providers(
            dir.path(),
            &outcomes,
            &identities,
            "fixture",
            dir.path().join("results.txt").to_str().unwrap(),
            &NoBoundaryEvidence,
            &NoQualification,
        )
        .unwrap_err();
        assert!(error.to_string().contains("results input"), "{error}");
        assert!(!dir.path().join("status.json").exists());
        assert!(!dir.path().join("history.jsonl").exists());
        assert_eq!(
            fs::read(dir.path().join("annotations.json")).unwrap(),
            original_annotations
        );
    }
}

#[test]
fn reordered_outcomes_map_by_id() {
    for scoped_mapping in [false, true] {
        let (dir, identities) = fixture();
        let a = format!("{A}#one");
        let b = format!("{B}#two");
        let (raw_a, raw_b) = if scoped_mapping {
            ("raw-a#one", "raw-b#two")
        } else {
            (a.as_str(), b.as_str())
        };
        let xml = format!(
            "<testsuite><testcase name=\"{raw_b}\"/><testcase name=\"{raw_a}\"/></testsuite>"
        );
        let mapping = if scoped_mapping {
            BTreeMap::from([(raw_a.to_owned(), a.clone()), (raw_b.to_owned(), b.clone())])
        } else {
            BTreeMap::new()
        };
        let mut outcomes = parse_junit(&xml).unwrap();
        for outcome in &mut outcomes {
            if let Some(id) = mapping.get(&outcome.test_id) {
                outcome.test_id.clone_from(id);
            }
        }
        // Ensure the scored vector really is the reverse of the raw file.
        outcomes.sort_by_key(|outcome| outcome.test_id != a);
        let file = dir.path().join("results.txt");
        fs::write(&file, &xml).unwrap();
        let results = ResultsSnapshot::from_bytes(
            dir.path(),
            &file,
            xml.as_bytes().to_vec(),
            "junit",
            mapping,
        )
        .unwrap();
        let report = rescore_with_context(
            dir.path(),
            &outcomes,
            &identities,
            "fixture",
            file.to_str().unwrap(),
            RescoreContext {
                results: Some(&results),
                boundary: &NoBoundaryEvidence,
                qualified: &NoQualification,
                reauditor: None,
                quantifier_reports: &|_, _| Ok(Vec::new()),
            },
        )
        .unwrap();
        assert!(report
            .statuses
            .iter()
            .all(|line| line.badge == Badge::Proven));
        assert!(!freshness(dir.path()).stale);
        fs::write(
            &file,
            xml.replace(
                &format!("name=\"{raw_b}\"/>"),
                &format!("name=\"{raw_b}\"><failure/></testcase>"),
            ),
        )
        .unwrap();
        let current = freshness(dir.path());
        assert!(current.stale);
        assert!(!current.lines.contains_key("R-001"));
        assert!(current.lines.contains_key("R-002"));
    }
}

#[test]
fn duplicate_or_missing_scoped_ids_are_input_errors() {
    let (dir, _) = fixture();
    let xml = "<testsuite><testcase name=\"a#one\"/><testcase name=\"a#two\"/></testsuite>";
    let file = dir.path().join("results.txt");
    for mapping in [
        BTreeMap::from([("a#one".into(), A.into())]),
        BTreeMap::from([("a#one".into(), A.into()), ("a#two".into(), A.into())]),
    ] {
        assert!(ResultsSnapshot::from_bytes(
            dir.path(),
            &file,
            xml.as_bytes().to_vec(),
            "junit",
            mapping,
        )
        .is_err());
    }
    let outcomes = parse_libtest(RESULTS).unwrap();
    let results = ResultsSnapshot::read(dir.path(), file.to_str().unwrap(), &outcomes).unwrap();
    let duplicate: Vec<TestOutcome> = vec![outcomes[0].clone(), outcomes[0].clone()];
    assert!(results.validate(&duplicate).is_err());
    assert!(results.validate(&outcomes[..1]).is_err());
}
