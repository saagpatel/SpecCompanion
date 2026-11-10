use living_spec::{
    agents::test_index::TestIndex,
    cli,
    core::{
        annotations::{Annotations, Audit, AuditVerdict, Link, RequirementAnnotations},
        mutation::MutationFile,
        spec::Spec,
    },
};
use std::{collections::BTreeMap, fs, path::Path};

fn put(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn dogfood_subset_mutation_round_trip() {
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    let temp = tempfile::tempdir_in(target).unwrap();
    let spec_dir = temp.path().join("spec");
    let crate_dir = temp.path().join("sample");
    fs::create_dir_all(&spec_dir).unwrap();
    put(
        &crate_dir,
        "Cargo.toml",
        "[package]\nname = \"mutation-round-trip\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    put(
        &crate_dir,
        "Cargo.lock",
        "version = 3\n\n[[package]]\nname = \"mutation-round-trip\"\nversion = \"0.1.0\"\n",
    );
    put(
        &crate_dir,
        "src/lib.rs",
        "pub fn select(value: i32) -> i32 { if value == 42 { 7 } else { 0 } }\n",
    );
    put(
        &crate_dir,
        "tests/contract.rs",
        "use mutation_round_trip::select;\n#[test]\nfn strong_test() { assert_eq!(select(42), 7); assert_eq!(select(0), 0); }\n#[test]\nfn weak_test() { assert!(select(42) >= 0); }\n",
    );
    let spec_text = "- [LS-901] select(42) returns 7.\n- [LS-902] For input 42, select returns an output >= 0.\n";
    put(&spec_dir, "spec.md", spec_text);
    let spec = Spec::parse(spec_text).unwrap();
    let index = TestIndex::build(&crate_dir).unwrap();
    let mut requirements = BTreeMap::new();
    for (line, name) in spec.lines.iter().zip(["strong_test", "weak_test"]) {
        let entry = index
            .entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap();
        let mut link = Link {
            test_id: entry.id.clone(),
            fingerprint: entry.fingerprint.clone(),
            rename_hint: Some(entry.rename_hint.clone()),
            revision: line.revision.clone(),
            explanation: "synthetic mutation round trip".into(),
            proposed_by: "human".into(),
            audit: Audit {
                verdict: AuditVerdict::Accepted,
                revision: line.revision.clone(),
                notes: vec![],
                extra: BTreeMap::new(),
            },
            extra: BTreeMap::new(),
        };
        link.audit = living_spec::agents::auditor::audit(line, &link, entry);
        assert_eq!(link.audit.verdict, AuditVerdict::Accepted);
        requirements.insert(
            line.id.clone(),
            RequirementAnnotations {
                links: vec![link],
                ..Default::default()
            },
        );
    }
    Annotations {
        schema: "living-spec.annotations/v1".into(),
        requirements,
        extra: BTreeMap::new(),
    }
    .save(spec_dir.join("annotations.json"))
    .unwrap();

    cli::run(&[
        "mutate".into(),
        spec_dir.to_string_lossy().into_owned(),
        "--tests".into(),
        crate_dir.to_string_lossy().into_owned(),
        "--crate".into(),
        crate_dir.to_string_lossy().into_owned(),
        "--max-mutants".into(),
        "8".into(),
        "--jobs".into(),
        "1".into(),
    ])
    .unwrap();

    let cases: Vec<String> = index
        .entries
        .iter()
        .filter(|entry| matches!(entry.name.as_str(), "strong_test" | "weak_test"))
        .map(|entry| {
            let (class, _) = entry.id.rsplit_once(':').unwrap();
            format!("<testcase classname=\"{class}\" name=\"{}\"/>", entry.name)
        })
        .collect();
    put(
        &spec_dir,
        "results.xml",
        &format!(
            "<testsuites tests=\"2\"><testsuite tests=\"2\">{}{}</testsuite></testsuites>",
            cases[0], cases[1]
        ),
    );
    cli::run(&[
        "score".into(),
        spec_dir.to_string_lossy().into_owned(),
        "--results".into(),
        spec_dir.join("results.xml").to_string_lossy().into_owned(),
        "--format".into(),
        "junit".into(),
        "--tests".into(),
        crate_dir.to_string_lossy().into_owned(),
        "--mutation".into(),
    ])
    .unwrap();
    let status: serde_json::Value =
        serde_json::from_slice(&fs::read(spec_dir.join("status.json")).unwrap()).unwrap();
    let states: BTreeMap<_, _> = status["statuses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| {
            (
                line["id"].as_str().unwrap().to_owned(),
                line["mutation"]["state"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(states["LS-901"], "mutation_checked");
    assert_eq!(states["LS-902"], "survived");
}

#[test]
fn committed_dogfood_mutation_has_no_absolute_paths_if_present() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("living-spec/mutation.json");
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed reading committed mutation file: {error}"),
    };
    let text = std::str::from_utf8(&bytes).unwrap();
    let file = MutationFile::from_bytes(&bytes).unwrap();
    assert!(!text.contains("/Users/") && !text.contains("/tmp/"));
    for record in file.links {
        for mutant in record.mutants {
            assert!(!Path::new(&mutant.file).is_absolute());
        }
    }
}
