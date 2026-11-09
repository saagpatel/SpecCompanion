use living_spec::{
    cli::boundary::parse_results,
    core::{annotations::Annotations, spec::Spec, test_identity::fingerprint},
    quantifier,
};
use serde_json::json;
use std::{fs, path::Path, process::Command};

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap()
}

fn boundary(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .args(["boundary", args[0]])
        .arg(dir)
        .args(&args[1..])
        .output()
        .unwrap()
}

#[test]
fn boundary_cli_skips_unrelated_prose_without_losing_boundary_errors() {
    let dir = tempdir();
    let annotations = dir.path().join("annotations.json");
    fs::write(
        dir.path().join("spec.md"),
        "- [R-001] Delay is at most 30 s.\n- [R-002] The service records audit events.\n- [R-003] The service records metrics.\n",
    )
    .unwrap();
    fs::write(&annotations, r#"{"schema":"living-spec.annotations/v1","requirements":{"R-002":{"foreign":{"keep":true}}}}"#).unwrap();
    let result = boundary(dir.path(), &["gen"]);
    assert!(result.status.success(), "{:?}", result);
    let generated: serde_json::Value =
        serde_json::from_slice(&fs::read(&annotations).unwrap()).unwrap();
    assert_eq!(
        generated["requirements"]["R-002"],
        json!({"foreign":{"keep":true}})
    );
    assert!(generated["requirements"].get("R-003").is_none());
    assert_eq!(
        generated["requirements"]["R-001"]["bounds"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let results = dir.path().join("results.xml");
    fs::write(
        &results,
        "<testsuite><testcase name=\"unrelated\"/></testsuite>",
    )
    .unwrap();
    let export = dir.path().join("coverage.json");
    let args = [
        "coverage",
        "--results",
        results.to_str().unwrap(),
        "--format",
        "junit",
        "--json",
        export.to_str().unwrap(),
    ];
    let result = boundary(dir.path(), &args);
    assert_eq!(result.status.code(), Some(3), "{:?}", result);
    let maps: serde_json::Value = serde_json::from_slice(&fs::read(&export).unwrap()).unwrap();
    assert_eq!(maps.as_array().unwrap().len(), 1);
    assert_eq!(maps[0]["requirement"], "R-001");
    let before = fs::read(&annotations).unwrap();
    for bad_line in ["Response is fast.", "Delay is at most 1 fortnight."] {
        fs::write(
            dir.path().join("spec.md"),
            format!("- [R-001] Delay is at most 30 s.\n- [R-002] {bad_line}\n"),
        )
        .unwrap();
        for args in [&["gen"][..], &args[..]] {
            assert_eq!(boundary(dir.path(), args).status.code(), Some(1));
        }
        assert_eq!(fs::read(&annotations).unwrap(), before);
    }
}

#[test]
fn boundary_junit_matches_literal_names_and_rejects_cross_class_duplicates() {
    let outcomes = parse_results(
        r#"<testsuite><testcase classname="limits" name="below"/><testcase classname="limits" name="at"><skipped/></testcase><testcase classname="limits" name="above"><failure/></testcase></testsuite>"#,
        "junit",
    ).unwrap();
    assert_eq!(
        outcomes
            .iter()
            .map(|o| o.test_id.as_str())
            .collect::<Vec<_>>(),
        ["below", "at", "above"]
    );
    assert!(parse_results(
        r#"<testsuite><testcase classname="first" name="same"/><testcase classname="second" name="same"/></testsuite>"#,
        "junit",
    ).is_err());
    let dir = tempdir();
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/cli/boundary/requirements/18.json")).unwrap();
    fs::write(
        dir.path().join("spec.md"),
        "- [R-018] Caption is under 80 chars.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("annotations.json"),
        serde_json::to_vec(&json!({
            "schema":"living-spec.annotations/v1", "requirements":{"R-018":{
                "bounds":fixture["expected_bounds"], "boundary_links":fixture["links"]
            }}
        }))
        .unwrap(),
    )
    .unwrap();
    let results = dir.path().join("results.xml");
    let cases = fixture["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|outcome| {
            format!(
                "<testcase classname=\"limits\" name=\"{}\"/>",
                outcome["test_id"].as_str().unwrap()
            )
        })
        .collect::<String>();
    fs::write(&results, format!("<testsuite>{cases}</testsuite>")).unwrap();
    let result = boundary(
        dir.path(),
        &[
            "coverage",
            "--results",
            results.to_str().unwrap(),
            "--format",
            "junit",
        ],
    );
    assert!(result.status.success(), "{:?}", result);
}

#[test]
fn explicit_null_quantifier_is_an_input_error_even_beside_a_valid_claim() {
    let dir = tempdir();
    fs::write(
        dir.path().join("spec.md"),
        "- [R-001] Clamp output remains bounded.\n- [R-002] Another requirement.\n",
    )
    .unwrap();
    let claim: serde_json::Value =
        serde_json::from_slice(&fs::read(quantifier::reference_dir().join("Q-001.json")).unwrap())
            .unwrap();
    let mut annotation = json!({"schema":"living-spec.annotations/v1","requirements":{"R-001":{"quantifier":claim["quantifier"]},"R-002":{}}});
    let path = dir.path().join("annotations.json");
    fs::write(&path, serde_json::to_vec(&annotation).unwrap()).unwrap();
    assert_eq!(quantifier::load(dir.path()).unwrap().claims.len(), 1);
    annotation["requirements"]["R-002"]["quantifier"] = serde_json::Value::Null;
    fs::write(&path, serde_json::to_vec(&annotation).unwrap()).unwrap();
    assert!(Annotations::load(&path).is_err());
    assert!(quantifier::load(dir.path()).is_err());
    let results = dir.path().join("results.xml");
    fs::write(
        &results,
        "<testsuite><testcase name=\"receipt\"/></testsuite>",
    )
    .unwrap();
    for name in ["status.json", "history.jsonl", "spec.html"] {
        fs::write(dir.path().join(name), format!("original {name}\n")).unwrap();
    }
    for command in ["score", "rescore", "check"] {
        let mut process = Command::new(env!("CARGO_BIN_EXE_living-spec"));
        process.arg(command).arg(dir.path());
        if command != "check" {
            process
                .arg("--results")
                .arg(&results)
                .args(["--format", "junit"]);
        }
        let result = process.output().unwrap();
        assert_eq!(result.status.code(), Some(1), "{:?}", result);
        for name in ["status.json", "history.jsonl", "spec.html"] {
            assert_eq!(
                fs::read_to_string(dir.path().join(name)).unwrap(),
                format!("original {name}\n")
            );
        }
    }
}

#[test]
fn dogfood_fingerprints_preserve_trailing_whitespace_claim() {
    let spec = Spec::parse(include_str!("../living-spec/spec.md")).unwrap();
    let line = spec.lines.iter().find(|line| line.id == "LS-007").unwrap();
    assert_eq!(
        line.text,
        "Test fingerprints preserve literal contents and trailing whitespace."
    );
    assert!(include_str!("../living-spec/spec.html").contains(&line.text));
    assert_ne!(fingerprint("a  \nb"), fingerprint("a\nb"));
    assert_ne!(fingerprint("x = 'a  b'"), fingerprint("x = 'a b'"));
    let annotations: Annotations =
        serde_json::from_str(include_str!("../living-spec/annotations.json")).unwrap();
    for link in &annotations.requirements["LS-007"].links {
        assert_eq!(link.revision, line.revision);
        assert_eq!(link.audit.revision, line.revision);
    }
}
