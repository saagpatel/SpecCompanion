use living_spec::{
    adapters::parse_libtest,
    agents::test_index::{SourceResolution, TestIndex},
    core::{annotations::Annotations, badge::Badge, freshness::ResultsSnapshot},
    runner::rescore::StatusDocument,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const CAPTURE: &str = include_str!("fixtures/pipeline/workspace-same-target/capture.txt");
const IDS: [&str; 3] = [
    "libtest:alpha/integration:tests/integration.rs:smoke",
    "libtest:beta/integration:tests/integration.rs:smoke",
    "libtest:integration:tests/integration.rs:unique",
];
const WORKSPACE_FILES: [&str; 6] = [
    "Cargo.toml",
    "Cargo.lock",
    "alpha/Cargo.toml",
    "alpha/tests/integration.rs",
    "beta/Cargo.toml",
    "beta/tests/integration.rs",
];

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pipeline/workspace-same-target")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap()
}

fn copy_workspace(root: &Path) {
    for file in WORKSPACE_FILES {
        let dest = root.join(file);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(fixture().join(file), dest).unwrap();
    }
}

fn ids(capture: &str) -> Vec<String> {
    let mut ids: Vec<_> = parse_libtest(capture)
        .unwrap()
        .into_iter()
        .map(|outcome| outcome.test_id)
        .collect();
    ids.sort();
    ids
}

#[test]
fn recorded_capture_and_workspace_index_agree() {
    assert_eq!(ids(CAPTURE), IDS);
    let index = TestIndex::build(&fixture()).unwrap();
    assert_eq!(
        index
            .entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        IDS
    );
    assert_eq!(
        index.get(IDS[0]).unwrap().file,
        "alpha/tests/integration.rs"
    );
    assert_eq!(index.get(IDS[1]).unwrap().file, "beta/tests/integration.rs");
}

#[test]
fn capture_without_package_artifacts_still_rejects_duplicate_ids() {
    let plain = CAPTURE
        .lines()
        .filter(|line| !line.starts_with('{'))
        .collect::<Vec<_>>()
        .join("\n");
    let error = parse_libtest(&plain).unwrap_err();
    assert!(error.0.contains("duplicate libtest id"), "{error}");
}

#[test]
fn plain_ambiguous_link_does_not_resolve_to_either_package() {
    let index = TestIndex::build(&fixture()).unwrap();
    let annotations = Annotations::load(fixture().join("spec/annotations.json")).unwrap();
    let mut link = annotations.requirements["WS-001"].links[0].clone();
    link.test_id = "libtest:integration:tests/integration.rs:smoke".into();
    assert!(index.get(&link.test_id).is_none());
    assert_eq!(index.resolve(&link), SourceResolution::Unresolved);
    // Qualification is conditional: the unique test has no qualified alias.
    assert!(index
        .resolve_test_id("libtest:alpha/integration:tests/integration.rs:unique")
        .is_none());
}

#[test]
fn indistinguishable_source_packages_still_fail_closed() {
    for replacement in ["name = \"alpha\"", "# no package name"] {
        let temp = scratch();
        copy_workspace(temp.path());
        let manifest = temp.path().join("beta/Cargo.toml");
        let text = fs::read_to_string(&manifest)
            .unwrap()
            .replace("name = \"beta\"", replacement);
        fs::write(manifest, text).unwrap();
        assert!(TestIndex::build(temp.path())
            .unwrap_err()
            .to_string()
            .contains("duplicate scoped test id"));
    }
}

#[test]
fn score_rescore_freshness_and_html_preserve_package_identity() {
    let temp = scratch();
    for file in ["spec.md", "annotations.json"] {
        fs::copy(fixture().join("spec").join(file), temp.path().join(file)).unwrap();
    }
    let results = temp.path().join("capture.txt");
    fs::write(&results, CAPTURE).unwrap();
    let html = temp.path().join("spec.html");
    for verb in ["score", "rescore"] {
        let output = Command::new(env!("CARGO_BIN_EXE_living-spec"))
            .arg(verb)
            .arg(temp.path())
            .arg("--results")
            .arg(&results)
            .args(["--format", "libtest", "--tests"])
            .arg(fixture())
            .arg("--html")
            .arg(&html)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let status: StatusDocument =
            serde_json::from_slice(&fs::read(temp.path().join("status.json")).unwrap()).unwrap();
        assert_eq!(status.statuses.len(), 3);
        assert!(status
            .statuses
            .iter()
            .all(|line| line.badge == Badge::Proven));
        let outcomes = parse_libtest(CAPTURE).unwrap();
        ResultsSnapshot::read(temp.path(), results.to_str().unwrap(), &outcomes).unwrap();
        let page = fs::read_to_string(&html).unwrap();
        for id in IDS {
            assert!(page.contains(id), "missing {id}");
        }
    }
    // Render loads the stored input bindings independently of the score pipeline.
    let output = Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .arg("render")
        .arg(temp.path())
        .arg("--out")
        .arg(&html)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let page = fs::read_to_string(html).unwrap();
    assert!(!page.contains("rescore required"));
    assert!(!page.contains("data-badge=\"stale\""));
    for id in IDS {
        assert!(page.contains(id));
    }
}

#[test]
fn ignored_cargo_json_kinds_do_not_affect_capture_identity() {
    // The committed capture omits these noisy records from the actual run.
    let noise = "{\"reason\":\"compiler-message\",\"message\":{\"rendered\":\"warning\"}}\n\
                 {\"reason\":\"build-finished\",\"success\":true}\n";
    assert_eq!(
        parse_libtest(&format!("{noise}{CAPTURE}")).unwrap(),
        parse_libtest(CAPTURE).unwrap()
    );
}

#[test]
fn fresh_offline_cargo_capture_matches_recorded_ids() {
    match Command::new("cargo")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping live capture: cargo is not on PATH");
            return;
        }
        result => assert!(result.unwrap().success()),
    }
    let temp = scratch();
    copy_workspace(temp.path());
    let capture_path = temp.path().join("capture.txt");
    let capture = fs::File::create(&capture_path).unwrap();
    // Shared file handles preserve stdout/stderr order, like shell `2>&1`.
    let result = Command::new("cargo")
        .args(["test", "--workspace", "--offline", "--message-format=json"])
        .current_dir(temp.path())
        .env("CARGO_TARGET_DIR", temp.path().join("target"))
        .env("CARGO_TERM_COLOR", "never")
        .env("NO_COLOR", "1")
        .stdout(capture.try_clone().unwrap())
        .stderr(capture)
        .status()
        .unwrap();
    let fresh = fs::read_to_string(capture_path).unwrap();
    assert!(result.success(), "{fresh}");
    assert_eq!(ids(&fresh), ids(CAPTURE));
}
