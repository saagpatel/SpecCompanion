use living_spec::{
    agents::test_index::TestIndex,
    cli,
    core::{
        mutation::{LinkRecord, LinkStatus, MutationFile, Signoff, Target, TargetKind},
        spec::Spec,
    },
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn sample() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mutation/sample-crate")
}
fn temp() -> tempfile::TempDir {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    fs::create_dir_all(&target).unwrap();
    tempfile::Builder::new()
        .prefix("mutation-cli-")
        .tempdir_in(target)
        .unwrap()
}
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == "target" || entry.file_name() == ".git" {
            continue;
        }
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}
fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, String) {
    let dir = temp();
    let root = dir.path().join("project");
    copy_tree(&sample(), &root);
    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
    fs::write(
        spec_dir.join("spec.md"),
        "- [LS-001] The clamp preserves the input bounds.\n",
    )
    .unwrap();
    let spec = Spec::parse(&fs::read_to_string(spec_dir.join("spec.md")).unwrap()).unwrap();
    let index = TestIndex::build(&root).unwrap();
    let entry = index
        .entries
        .iter()
        .find(|entry| entry.name == "weak_test_only_checks_output_is_nonnegative")
        .unwrap();
    let revision = spec.lines[0].revision.clone();
    let annotations = json!({"schema":"living-spec.annotations/v1","requirements":{"LS-001":{"links":[{
        "test_id":entry.id,"fingerprint":entry.fingerprint,"revision":revision,
        "rename_hint":entry.rename_hint,"explanation":"reviewed test","proposed_by":"human",
        "audit":{"verdict":"accepted","revision":revision,"notes":[]}
    }]}}});
    fs::write(
        spec_dir.join("annotations.json"),
        serde_json::to_vec_pretty(&annotations).unwrap(),
    )
    .unwrap();
    (dir, root, spec_dir, entry.id.clone())
}
fn cargo_stub(root: &Path, fail_mutants: bool) -> PathBuf {
    let path = root.join("cargo-stub");
    let mutant = if fail_mutants {
        "if [ \"$n\" -gt 2 ]; then printf 'running 1 test\\ntest %s ... FAILED\\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\\n' \"$6\"; exit 101; fi"
    } else {
        ":"
    };
    fs::write(&path, format!("#!/bin/sh\nn=0; [ ! -f .cli-count ] || n=$(cat .cli-count); n=$((n+1)); echo $n > .cli-count; {mutant}; printf 'running 1 test\\ntest %s ... ok\\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\\n' \"$6\"\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}
fn mutate(root: &Path, spec: &Path, cargo: &Path) -> Result<(), living_spec::cli::Exit> {
    cli::run(&[
        "mutate".into(),
        spec.to_string_lossy().into_owned(),
        "--tests".into(),
        root.to_string_lossy().into_owned(),
        "--crate".into(),
        root.to_string_lossy().into_owned(),
        "--jobs".into(),
        "2".into(),
        "--max-mutants".into(),
        "1".into(),
        "--only".into(),
        "LS-001".into(),
        "--cargo".into(),
        cargo.to_string_lossy().into_owned(),
    ])
}

fn mutate_scoped(
    root: &Path,
    spec: &Path,
    requested_crate: &Path,
    cargo: &Path,
) -> Result<(), living_spec::cli::Exit> {
    cli::run(&[
        "mutate".into(),
        spec.to_string_lossy().into_owned(),
        "--tests".into(),
        root.to_string_lossy().into_owned(),
        "--crate".into(),
        requested_crate.to_string_lossy().into_owned(),
        "--cargo".into(),
        cargo.to_string_lossy().into_owned(),
    ])
}

#[test]
fn mutate_writes_deterministic_file_and_preserves_signoffs_without_absolute_paths() {
    let (_temp, root, spec, test_id) = setup();
    let cargo = cargo_stub(&root, true);
    let file_path = spec.join("mutation.json");
    let seeded = MutationFile {
        schema: "living-spec.mutation/v1".into(),
        engine: "rust-cargo-v1".into(),
        links: vec![],
        signoffs: vec![Signoff::Link {
            requirement: "LS-999".into(),
            test_id: "libtest:other:tests/other.rs:test".into(),
            test_fingerprint: "sha256:fingerprint".into(),
            requirement_revision: "sha256:revision".into(),
            reason: "historic signoff".into(),
            signed_by: "reviewer".into(),
        }],
    };
    fs::write(&file_path, seeded.to_pretty_bytes().unwrap()).unwrap();
    mutate(&root, &spec, &cargo).unwrap();
    let first = fs::read(&file_path).unwrap();
    mutate(&root, &spec, &cargo).unwrap();
    let second = fs::read(&file_path).unwrap();
    assert_eq!(first, second);
    let file = MutationFile::from_bytes(&first).unwrap();
    assert_eq!(file.links.len(), 1);
    assert_eq!(file.links[0].test_id, test_id);
    assert_eq!(file.links[0].status, LinkStatus::Checked);
    assert_eq!(file.signoffs.len(), 1);
    assert!(!String::from_utf8(first)
        .unwrap()
        .contains(root.to_string_lossy().as_ref()));
    let link_signoff = [
        "signoff",
        spec.to_str().unwrap(),
        "link",
        "LS-001",
        &test_id,
        "--reason",
        "reviewed checked evidence",
        "--signed-by",
        "reviewer",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    cli::run(&link_signoff).unwrap();
    let mut signed_file = MutationFile::from_bytes(&fs::read(&file_path).unwrap()).unwrap();
    signed_file.signoffs.reverse();
    fs::write(&file_path, signed_file.to_pretty_bytes().unwrap()).unwrap();
    let signed = fs::read(&file_path).unwrap();
    cli::run(&link_signoff).unwrap();
    assert_eq!(signed, fs::read(&file_path).unwrap());
}

#[test]
fn survivor_does_not_fail_exit_code_and_link_signoff_is_rejected() {
    let (_temp, root, spec, _test_id) = setup();
    let cargo = cargo_stub(&root, false);
    mutate(&root, &spec, &cargo).unwrap();
    let file = MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
    assert_eq!(file.links[0].status, LinkStatus::Survived);
    let test_id = file.links[0].test_id.clone();
    let result = cli::run(&[
        "signoff".into(),
        spec.to_string_lossy().into_owned(),
        "link".into(),
        "LS-001".into(),
        test_id.clone(),
        "--reason".into(),
        "not equivalent".into(),
        "--signed-by".into(),
        "reviewer".into(),
    ]);
    assert!(result.is_err());
    let mutant_id = file.links[0]
        .mutants
        .iter()
        .find(|m| m.outcome == living_spec::core::mutation::MutantOutcome::Survived)
        .unwrap()
        .id
        .clone();
    let mutant_signoff = [
        "signoff",
        spec.to_str().unwrap(),
        "mutant",
        "LS-001",
        &test_id,
        &mutant_id,
        "--reason",
        "equivalent mutant",
        "--signed-by",
        "reviewer",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    cli::run(&mutant_signoff).unwrap();
    let signed = fs::read(spec.join("mutation.json")).unwrap();
    cli::run(&mutant_signoff).unwrap();
    assert_eq!(signed, fs::read(spec.join("mutation.json")).unwrap());
    let signoff_file = MutationFile::from_bytes(&signed).unwrap();
    assert!(
        matches!(&signoff_file.signoffs[0], Signoff::Mutant { subject_digest, .. } if subject_digest == &signoff_file.links[0].subject_digest)
    );
}

#[test]
fn crate_scope_preserves_existing_record_for_unsupported_test_in_other_crate() {
    let dir = temp();
    let root = dir.path().join("project");
    let selected = root.join("selected");
    let other = root.join("other");
    fs::create_dir_all(selected.join("src")).unwrap();
    fs::create_dir_all(selected.join("tests")).unwrap();
    fs::create_dir_all(other.join("tests")).unwrap();
    fs::write(
        selected.join("Cargo.toml"),
        "[package]\nname = \"selected\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(selected.join("src/lib.rs"), "pub fn unused() {}\n").unwrap();
    fs::write(
        selected.join("tests/no_subject.rs"),
        "#[test]\nfn unsupported_selected_test() {}\n",
    )
    .unwrap();
    fs::write(
        other.join("Cargo.toml"),
        "[package]\nname = \"other\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        other.join("tests/unsupported.py"),
        "def test_outside(): pass\n",
    )
    .unwrap();

    let spec_dir = root.join("spec");
    fs::create_dir_all(&spec_dir).unwrap();
    fs::write(
        spec_dir.join("spec.md"),
        "- [LS-001] Scoped mutation does not replace another crate's record.\n",
    )
    .unwrap();
    let spec = Spec::parse(&fs::read_to_string(spec_dir.join("spec.md")).unwrap()).unwrap();
    let index = TestIndex::build(&root).unwrap();
    let selected_entry = index
        .entries
        .iter()
        .find(|entry| entry.name == "unsupported_selected_test")
        .unwrap();
    let other_entry = index
        .entries
        .iter()
        .find(|entry| entry.name == "test_outside")
        .unwrap();
    let revision = &spec.lines[0].revision;
    let annotations = json!({
        "schema":"living-spec.annotations/v1",
        "requirements":{"LS-001":{"links":[
            {"test_id":selected_entry.id,"fingerprint":selected_entry.fingerprint,"revision":revision,
             "rename_hint":selected_entry.rename_hint,"explanation":"selected crate test","proposed_by":"human",
             "audit":{"verdict":"accepted","revision":revision,"notes":[]}},
            {"test_id":other_entry.id,"fingerprint":other_entry.fingerprint,"revision":revision,
             "rename_hint":other_entry.rename_hint,"explanation":"outside unsupported test","proposed_by":"human",
             "audit":{"verdict":"accepted","revision":revision,"notes":[]}}
        ]}}
    });
    fs::write(
        spec_dir.join("annotations.json"),
        serde_json::to_vec_pretty(&annotations).unwrap(),
    )
    .unwrap();
    let outside_record = LinkRecord {
        requirement: "LS-001".into(),
        test_id: other_entry.id.clone(),
        test_fingerprint: other_entry.fingerprint.clone(),
        requirement_revision: revision.clone(),
        subject_digest: "sha256:existing-subject".into(),
        target: Target {
            kind: TargetKind::Test,
            name: "unsupported".into(),
        },
        baseline_passes: 2,
        status: LinkStatus::Error,
        reason: Some("preserve this existing error".into()),
        infra_failure: Some("prior runner failure".into()),
        mutants: vec![],
    };
    let mutation_path = spec_dir.join("mutation.json");
    fs::write(
        &mutation_path,
        MutationFile {
            schema: "living-spec.mutation/v1".into(),
            engine: "rust-cargo-v1".into(),
            links: vec![outside_record.clone()],
            signoffs: vec![],
        }
        .to_pretty_bytes()
        .unwrap(),
    )
    .unwrap();
    let cargo = root.join("unused-cargo-stub");
    fs::write(&cargo, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    }

    mutate_scoped(&root, &spec_dir, &selected, &cargo).unwrap();

    let saved = MutationFile::from_bytes(&fs::read(mutation_path).unwrap()).unwrap();
    assert!(saved.links.contains(&outside_record));
    assert_eq!(saved.links.len(), 2);
    assert!(saved
        .links
        .iter()
        .any(|record| record.test_id == selected_entry.id
            && record.status == LinkStatus::Unsupported));
}

#[test]
fn completed_mutation_run_writes_error_links_and_returns_success() {
    let (_temp, root, spec, test_id) = setup();
    let cargo = cargo_stub(&root, false);
    fs::write(&cargo, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .args([
            "mutate",
            spec.to_str().unwrap(),
            "--tests",
            root.to_str().unwrap(),
            "--crate",
            root.to_str().unwrap(),
            "--jobs",
            "2",
            "--max-mutants",
            "1",
            "--only",
            "LS-001",
            "--cargo",
            cargo.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "completed error links must not fail the CLI: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(
        "mutation run complete: 1 links run, 0 checked, 0 survived, 0 unsupported, 1 error"
    ));

    let file = MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
    let record = file
        .links
        .iter()
        .find(|record| record.test_id == test_id)
        .unwrap();
    assert_eq!(record.status, LinkStatus::Error);
}

#[test]
fn score_flag_missing_file_is_an_input_error() {
    let (_temp, root, spec, test_id) = setup();
    let results = root.join("results.txt");
    fs::write(
        &results,
        format!(
            "running 1 test\ntest {test_id} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
        ),
    )
    .unwrap();
    let result = cli::run(&[
        "score".into(),
        spec.to_string_lossy().into_owned(),
        "--results".into(),
        results.to_string_lossy().into_owned(),
        "--format".into(),
        "libtest".into(),
        "--tests".into(),
        root.to_string_lossy().into_owned(),
        "--mutation".into(),
    ]);
    assert!(result.is_err());
    assert!(!spec.join("status.json").exists());
}

#[test]
fn overridden_link_can_be_mutated_and_signed_off() {
    let (_temp, root, spec, test_id) = setup();
    let path = spec.join("annotations.json");
    let mut annotations: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let link = &mut annotations["requirements"]["LS-001"]["links"][0];
    link["audit"]["verdict"] = json!("challenged");
    link["audit"]["override"] = json!({"by":"human", "reason":"reviewed limitation", "test_id":link["test_id"], "fingerprint":link["fingerprint"], "revision":link["revision"]});
    fs::write(path, serde_json::to_vec_pretty(&annotations).unwrap()).unwrap();
    mutate(&root, &spec, &cargo_stub(&root, true)).unwrap();
    let file = MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
    assert_eq!(file.links[0].status, LinkStatus::Checked);
    cli::run(&[
        "signoff".into(),
        spec.to_string_lossy().into_owned(),
        "link".into(),
        "LS-001".into(),
        test_id,
        "--reason".into(),
        "reviewed".into(),
        "--signed-by".into(),
        "reviewer".into(),
    ])
    .unwrap();
    let file = MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
    assert_eq!(file.signoffs.len(), 1);
}

#[test]
fn relative_single_component_spec_supports_both_signoffs() {
    for (fail_mutants, kind) in [(true, "link"), (false, "mutant")] {
        let (_temp, root, spec, test_id) = setup();
        mutate(&root, &spec, &cargo_stub(&root, fail_mutants)).unwrap();
        let file =
            MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
        let mut args = vec!["signoff", "spec", kind, "LS-001", test_id.as_str()];
        if kind == "mutant" {
            args.push(&file.links[0].mutants[0].id);
        }
        args.extend(["--reason", "reviewed", "--signed-by", "reviewer"]);
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
            .current_dir(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let signed =
            MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
        assert_eq!(signed.signoffs.len(), 1);
    }
}

#[test]
fn mutate_then_score_with_crate_tests_dir_as_tests_root() {
    let (_temp, root, spec, test_id) = setup();
    let tests = root.join("tests");
    mutate_scoped(&tests, &spec, &root, &cargo_stub(&root, true)).unwrap();
    let file = MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
    assert_eq!(file.links[0].status, LinkStatus::Checked);
    let (class, name) = test_id.rsplit_once(':').unwrap();
    let results = spec.join("results.xml");
    fs::write(&results, format!("<testsuites tests=\"1\"><testsuite tests=\"1\"><testcase classname=\"{class}\" name=\"{name}\"/></testsuite></testsuites>")).unwrap();
    cli::run(&[
        "score".into(),
        spec.to_string_lossy().into_owned(),
        "--results".into(),
        results.to_string_lossy().into_owned(),
        "--format".into(),
        "junit".into(),
        "--tests".into(),
        tests.to_string_lossy().into_owned(),
        "--mutation".into(),
    ])
    .unwrap();
    let status: serde_json::Value =
        serde_json::from_slice(&fs::read(spec.join("status.json")).unwrap()).unwrap();
    assert_eq!(
        status["statuses"][0]["mutation"]["state"],
        "mutation_checked"
    );
}

fn relative_mutate(root: &Path, cargo: &Path, extra: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .current_dir(root)
        .args([
            "mutate",
            "spec",
            "--tests",
            "tests",
            "--max-mutants",
            "1",
            "--cargo",
        ])
        .arg(cargo)
        .args(extra)
        .output()
        .unwrap()
}

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn relative_tests_dir_from_crate_root_mutate_with_crate_dot() {
    let (_temp, root, spec, test_id) = setup();
    let output = relative_mutate(&root, &cargo_stub(&root, true), &["--crate", "."]);
    assert_success(&output);
    let file = MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
    assert_eq!(file.links.len(), 1);
    assert_eq!(file.links[0].test_id, test_id);
    assert_eq!(file.links[0].status, LinkStatus::Checked);
}

#[test]
fn empty_tests_path_resolves_to_current_directory() {
    let (_temp, root, spec, _) = setup();
    let cargo = cargo_stub(&root, true);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .current_dir(&root)
        .args([
            "mutate",
            "spec",
            "--tests",
            "",
            "--crate",
            ".",
            "--max-mutants",
            "1",
            "--cargo",
        ])
        .arg(cargo)
        .output()
        .unwrap();
    assert_success(&output);
    let file = MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
    assert_eq!(file.links[0].status, LinkStatus::Checked);
}

#[test]
fn relative_tests_dir_without_crate_flag_keeps_existing_evidence() {
    let (_temp, root, spec, _) = setup();
    let cargo = cargo_stub(&root, true);
    mutate(&root, &spec, &cargo).unwrap();
    let before = fs::read(spec.join("mutation.json")).unwrap();
    assert_success(&relative_mutate(&root, &cargo, &[]));
    assert_eq!(fs::read(spec.join("mutation.json")).unwrap(), before);
}

#[test]
fn relative_tests_dir_then_score_keeps_checked() {
    let (_temp, root, spec, test_id) = setup();
    assert_success(&relative_mutate(&root, &cargo_stub(&root, true), &[]));
    let (class, name) = test_id.rsplit_once(':').unwrap();
    fs::write(spec.join("results.xml"), format!("<testsuites tests=\"1\"><testsuite tests=\"1\"><testcase classname=\"{class}\" name=\"{name}\"/></testsuite></testsuites>")).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .current_dir(&root)
        .args([
            "score",
            "spec",
            "--results",
            "spec/results.xml",
            "--format",
            "junit",
            "--tests",
            "tests",
            "--mutation",
        ])
        .output()
        .unwrap();
    assert_success(&output);
    let status: serde_json::Value =
        serde_json::from_slice(&fs::read(spec.join("status.json")).unwrap()).unwrap();
    assert_eq!(
        status["statuses"][0]["mutation"]["state"],
        "mutation_checked"
    );
}

#[test]
fn uncanonicalizable_root_is_error_not_unsupported_record() {
    let (_temp, root, spec, _) = setup();
    let cargo = cargo_stub(&root, true);
    mutate(&root, &spec, &cargo).unwrap();
    let before = fs::read(spec.join("mutation.json")).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .current_dir(&root)
        .args(["mutate", "spec", "--tests", "missing", "--crate", "."])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid input"));
    assert_eq!(fs::read(spec.join("mutation.json")).unwrap(), before);
}

fn python_signoff_case(parameterized: bool) {
    let dir = temp();
    let spec_dir = dir.path().join("spec");
    let tests = dir.path().join("tests");
    fs::create_dir(&spec_dir).unwrap();
    fs::create_dir(&tests).unwrap();
    let text = "- [LS-001] Export preserves the value.\n";
    fs::write(spec_dir.join("spec.md"), text).unwrap();
    let source = if parameterized {
        "@pytest.mark.parametrize('value', [30])\ndef test_export(value):\n    assert export_value(value) == value\n"
    } else {
        "def test_export():\n    assert export_value(30) == 30\n"
    };
    fs::write(tests.join("test_export.py"), source).unwrap();
    let entry = TestIndex::build(&tests).unwrap().entries.remove(0);
    assert!(entry.id.starts_with("pytest:"));
    let id = if parameterized {
        format!("{}#30", entry.id)
    } else {
        entry.id.clone()
    };
    let revision = &Spec::parse(text).unwrap().lines[0].revision;
    let annotations = json!({"schema":"living-spec.annotations/v1","requirements":{"LS-001":{"links":[{
        "test_id":id,"fingerprint":entry.fingerprint,"revision":revision,
        "rename_hint":entry.rename_hint,"explanation":"reviewed","proposed_by":"human",
        "audit":{"verdict":"accepted","revision":revision,"notes":[],"override":{
            "by":"human","reason":"reviewed assertion","test_id":id,"fingerprint":entry.fingerprint,"revision":revision
        }}
    }]}}});
    fs::write(
        spec_dir.join("annotations.json"),
        serde_json::to_vec_pretty(&annotations).unwrap(),
    )
    .unwrap();
    let spec = spec_dir.to_str().unwrap();
    let tests_root = tests.to_str().unwrap();
    cli::run(&["mutate", spec, "--tests", tests_root].map(str::to_owned)).unwrap();
    let file =
        MutationFile::from_bytes(&fs::read(spec_dir.join("mutation.json")).unwrap()).unwrap();
    assert_eq!(file.links.len(), 1);
    assert_eq!(file.links[0].test_id, id);
    assert_eq!(file.links[0].status, LinkStatus::Unsupported);
    cli::run(
        &[
            "signoff",
            spec,
            "link",
            "LS-001",
            &id,
            "--tests",
            tests_root,
            "--reason",
            "reviewed",
            "--signed-by",
            "reviewer",
        ]
        .map(str::to_owned),
    )
    .unwrap();
    let file =
        MutationFile::from_bytes(&fs::read(spec_dir.join("mutation.json")).unwrap()).unwrap();
    assert!(matches!(&file.signoffs[0], Signoff::Link { test_id, .. } if test_id == &id));
    let results = spec_dir.join("results.xml");
    let name = if parameterized {
        "test_export[30]"
    } else {
        "test_export"
    };
    fs::write(&results, format!("<testsuite tests=\"1\"><testcase classname=\"test_export\" name=\"{name}\"/></testsuite>")).unwrap();
    cli::run(
        &[
            "score",
            spec,
            "--results",
            results.to_str().unwrap(),
            "--format",
            "junit",
            "--tests",
            tests_root,
            "--mutation",
        ]
        .map(str::to_owned),
    )
    .unwrap();
    let status: serde_json::Value =
        serde_json::from_slice(&fs::read(spec_dir.join("status.json")).unwrap()).unwrap();
    assert_eq!(status["statuses"][0]["mutation"]["state"], "signed_off");
}

#[test]
fn mutate_then_signoff_non_rust_link_with_sibling_tests_dir() {
    python_signoff_case(false);
}

#[test]
fn parameterized_case_link_mutate_signoff_score() {
    python_signoff_case(true);
}

fn javascript_hash_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, String) {
    let dir = temp();
    let spec = dir.path().join("spec");
    let tests = dir.path().join("tests");
    fs::create_dir(&spec).unwrap();
    fs::create_dir(&tests).unwrap();
    let text = "- [LS-001] Export preserves the value.\n";
    fs::write(spec.join("spec.md"), text).unwrap();
    fs::write(
        tests.join("export.test.js"),
        "test('exports #tag', () => { expect(exportValue(30)).toBe(30); });\n",
    )
    .unwrap();
    let entry = TestIndex::build(&tests).unwrap().entries.remove(0);
    let revision = &Spec::parse(text).unwrap().lines[0].revision;
    let annotations = json!({"schema":"living-spec.annotations/v1","requirements":{"LS-001":{"links":[{
        "test_id":entry.id,"fingerprint":entry.fingerprint,"revision":revision,
        "rename_hint":entry.rename_hint,"explanation":"reviewed","proposed_by":"human",
        "audit":{"verdict":"accepted","revision":revision,"notes":[]}
    }]}}});
    fs::write(
        spec.join("annotations.json"),
        serde_json::to_vec_pretty(&annotations).unwrap(),
    )
    .unwrap();
    (dir, spec, tests, entry.id)
}

#[test]
fn javascript_title_with_hash_stays_proven_without_mutation_flag() {
    let (_dir, spec, tests, id) = javascript_hash_fixture();
    let results = spec.join("results.xml");
    let (class, name) = id.rsplit_once(':').unwrap();
    fs::write(
        &results,
        format!(
            "<testsuite tests=\"1\"><testcase classname=\"{class}\" name=\"{name}\"/></testsuite>"
        ),
    )
    .unwrap();
    cli::run(
        &[
            "score",
            spec.to_str().unwrap(),
            "--results",
            results.to_str().unwrap(),
            "--format",
            "junit",
            "--tests",
            tests.to_str().unwrap(),
        ]
        .map(str::to_owned),
    )
    .unwrap();
    let status: serde_json::Value =
        serde_json::from_slice(&fs::read(spec.join("status.json")).unwrap()).unwrap();
    assert_eq!(status["statuses"][0]["badge"], "proven");
    let annotations: serde_json::Value =
        serde_json::from_slice(&fs::read(spec.join("annotations.json")).unwrap()).unwrap();
    assert_eq!(
        annotations["requirements"]["LS-001"]["links"][0]["test_id"],
        id
    );
    assert_eq!(
        annotations["requirements"]["LS-001"]["links"][0]["audit"]["verdict"],
        "accepted"
    );
}

#[test]
fn javascript_title_with_hash_can_be_mutated_and_signed_off() {
    let (_dir, spec, tests, id) = javascript_hash_fixture();
    cli::run(
        &[
            "mutate",
            spec.to_str().unwrap(),
            "--tests",
            tests.to_str().unwrap(),
        ]
        .map(str::to_owned),
    )
    .unwrap();
    let file = MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
    assert_eq!(file.links[0].test_id, id);
    assert_eq!(file.links[0].status, LinkStatus::Unsupported);
    cli::run(
        &[
            "signoff",
            spec.to_str().unwrap(),
            "link",
            "LS-001",
            &id,
            "--tests",
            tests.to_str().unwrap(),
            "--reason",
            "reviewed",
            "--signed-by",
            "reviewer",
        ]
        .map(str::to_owned),
    )
    .unwrap();
    let file = MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
    assert!(matches!(&file.signoffs[0], Signoff::Link { test_id, .. } if test_id == &id));
}

#[test]
fn unknown_hash_suffix_does_not_match_base_prefix() {
    let (_dir, _spec, tests, id) = javascript_hash_fixture();
    let index = TestIndex::build(&tests).unwrap();
    assert!(index.resolve_test_id(&format!("{id}#unknown")).is_none());
    assert_eq!(index.resolve_test_id(&id).unwrap().id, id);
}

#[test]
fn unknown_rust_case_is_unresolved_without_spawning_cargo_or_signoff() {
    let (_temp, root, spec, base) = setup();
    let id = format!("{base}#case");
    let path = spec.join("annotations.json");
    let mut annotations: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    annotations["requirements"]["LS-001"]["links"][0]["test_id"] = json!(id);
    fs::write(path, serde_json::to_vec_pretty(&annotations).unwrap()).unwrap();
    let marker = root.join("spawned");
    let cargo = root.join("cargo-stub");
    fs::write(
        &cargo,
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    }
    mutate(&root, &spec, &cargo).unwrap();
    let file = MutationFile::from_bytes(&fs::read(spec.join("mutation.json")).unwrap()).unwrap();
    assert!(file.links.is_empty());
    assert!(!marker.exists());
    cli::run(
        &[
            "signoff",
            spec.to_str().unwrap(),
            "link",
            "LS-001",
            &id,
            "--tests",
            root.to_str().unwrap(),
            "--reason",
            "reviewed",
            "--signed-by",
            "reviewer",
        ]
        .map(str::to_owned),
    )
    .unwrap_err();
}
