use living_spec::{
    agents::test_index::TestIndex,
    core::mutation::{LinkStatus, MutantOutcome, MutationFile},
    mutation::{run_link, Options},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

fn sample() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mutation/sample-crate")
}
fn entry(name: &str) -> living_spec::agents::test_index::TestEntry {
    TestIndex::build(&sample())
        .unwrap()
        .entries
        .into_iter()
        .find(|e| e.name == name)
        .unwrap()
}
fn run(options: &Options, name: &str) -> living_spec::core::mutation::LinkRecord {
    let record = run_link(options, &entry(name), "R-001", "sha256:revision", &sample());
    MutationFile {
        schema: "living-spec.mutation/v1".into(),
        engine: "rust-cargo-v1".into(),
        links: vec![record.clone()],
        signoffs: vec![],
    }
    .validate()
    .unwrap();
    record
}
const STRONG: &str = "strong_test_checks_boundaries_and_operators";
const WEAK: &str = "weak_test_only_checks_output_is_nonnegative";
#[test]
fn strong_test_kills_every_mutant() {
    let record = run(&Options::default(), STRONG);
    assert_eq!(record.status, LinkStatus::Checked, "{record:?}");
    assert!(
        record
            .mutants
            .iter()
            .all(|m| m.outcome == MutantOutcome::Killed),
        "{record:?}"
    );
}
#[test]
fn weak_test_leaves_a_survivor() {
    assert_eq!(run(&Options::default(), WEAK).status, LinkStatus::Survived);
}
fn contents(root: &Path, result: &mut Vec<(PathBuf, Vec<u8>)>) {
    for e in fs::read_dir(root).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() {
            contents(&e.path(), result);
        } else {
            result.push((e.path(), fs::read(e.path()).unwrap()));
        }
    }
    result.sort();
}
fn tree_hash(root: &Path) -> Vec<u8> {
    let mut files = vec![];
    contents(root, &mut files);
    let mut hash = Sha256::new();
    for (path, bytes) in files {
        let path = path.strip_prefix(root).unwrap().to_string_lossy();
        hash.update((path.len() as u64).to_be_bytes());
        hash.update(path.as_bytes());
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    hash.finalize().to_vec()
}
#[test]
fn original_tree_unchanged() {
    let before = tree_hash(&sample());
    run(&Options::default(), WEAK);
    assert_eq!(before, tree_hash(&sample()));
}

#[cfg(unix)]
mod stub {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn make(script: &str) -> (tempfile::TempDir, Options) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cargo");
        fs::write(
            &path,
            format!("#!/bin/sh\ncd '{}'\n{}\n", dir.path().display(), script),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        let options = Options {
            cargo: path,
            max_mutants: 1,
            timeout: Duration::from_secs(2),
        };
        (dir, options)
    }
    const PASS: &str = "printf 'running 1 test\\ntest weak_test_only_checks_output_is_nonnegative ... ok\\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\\n'";
    fn after_baseline(action: &str) -> String {
        format!("n=0; [ ! -f count ] || n=$(cat count); n=$((n+1)); printf %s $n > count; if [ $n -le 2 ]; then {PASS}; else {action}; fi")
    }
    #[test]
    fn cargo_env_forced_no_color() {
        let (dir, options) = make(&format!(
            "printf '%s %s %s\\n' \"$CARGO_TERM_COLOR\" \"$NO_COLOR\" \"$*\" >> calls; {PASS}"
        ));
        run(&options, WEAK);
        let calls = fs::read_to_string(dir.path().join("calls")).unwrap();
        assert_eq!(calls.lines().count(), 3);
        assert!(calls.lines().all(|line| line
            .starts_with("never 1 test --offline --locked --test weak ")
            && line.ends_with(" -- --exact")));
    }
    #[test]
    fn zero_matching_tests_is_error_not_kill() {
        let (_dir, options) = make(&after_baseline("printf 'running 0 tests\\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out\\n'"));
        assert_eq!(run(&options, WEAK).status, LinkStatus::Error);
    }
    #[test]
    fn build_failure_is_unviable_not_kill() {
        let (_dir, options) = make(&after_baseline("echo 'error[E1234]: broken' >&2; exit 1"));
        let record = run(&options, WEAK);
        assert_eq!(record.status, LinkStatus::Unsupported);
        assert_eq!(record.mutants[0].outcome, MutantOutcome::Unviable);
    }
    #[test]
    fn baseline_failure_is_error() {
        let (_dir, options) = make("exit 1");
        assert_eq!(run(&options, WEAK).status, LinkStatus::Error);
    }
    #[test]
    fn timeout_is_inconclusive_not_a_kill() {
        let (_dir, options) = make(&after_baseline("sleep 5"));
        let record = run(&options, WEAK);
        assert_eq!(record.status, LinkStatus::Error);
        assert_eq!(record.mutants[0].outcome, MutantOutcome::Timeout);
    }
    #[test]
    fn timeout_covers_inherited_output_pipes() {
        let (_dir, mut options) = make(&after_baseline("sleep 5 & exit 0"));
        options.timeout = Duration::from_millis(300);
        let record = run(&options, WEAK);
        assert_eq!(record.mutants[0].outcome, MutantOutcome::Timeout);
    }
    #[test]
    fn killed_then_spawn_failure_keeps_evidence_as_error() {
        let fail = "printf 'running 1 test\\ntest weak_test_only_checks_output_is_nonnegative ... FAILED\\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\\n'; rm cargo; exit 1";
        let (_dir, mut options) = make(&after_baseline(fail));
        options.max_mutants = 2;
        let record = run(&options, WEAK);
        assert_eq!(record.status, LinkStatus::Error);
        assert!(record.infra_failure.is_some());
        assert_eq!(record.mutants[0].outcome, MutantOutcome::Killed);
    }
    #[test]
    fn non_rust_test_unsupported_without_spawn() {
        let (dir, options) = make("touch called");
        let mut test = entry(WEAK);
        test.language = living_spec::agents::test_index::Language::Python;
        let record = run_link(&options, &test, "R-001", "sha256:revision", &sample());
        assert_eq!(
            record.reason.as_deref(),
            Some("only Rust tests are supported in v1")
        );
        assert!(!dir.path().join("called").exists());
    }
}
