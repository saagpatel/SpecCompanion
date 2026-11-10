use living_spec::{
    agents::test_index::TestIndex,
    cli,
    core::{
        mutation::{LinkStatus, MutationFile, Signoff},
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
