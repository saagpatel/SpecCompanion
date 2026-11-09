use living_spec::adapters::parse_libtest;
use std::{fs, path::Path};

#[test]
fn workspace_crates_with_same_test_name_get_distinct_ids() {
    let capture = "Running unittests src/lib.rs (target/debug/deps/alpha-123abc)\nrunning 1 test\ntest tests::same ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\nRunning unittests src/lib.rs (target/debug/deps/beta-456def)\nrunning 1 test\ntest tests::same ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
    let outcomes = parse_libtest(capture).unwrap();
    let dir = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
    for name in ["alpha", "beta"] {
        let root = dir.path().join(name);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\n"),
        )
        .unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "mod tests { #[test] fn same() { assert_eq!(1, 1); } }",
        )
        .unwrap();
    }
    let index = living_spec::agents::test_index::TestIndex::build(dir.path()).unwrap();
    for (outcome, name) in outcomes.iter().zip(["alpha", "beta"]) {
        assert_eq!(
            outcome.test_id,
            format!("libtest:{name}:src/lib.rs:tests::same")
        );
        assert!(index.get(&outcome.test_id).is_some());
    }
}
