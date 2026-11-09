use living_spec::{
    agents::{linker::propose, test_index::TestIndex},
    core::{
        spec::revision,
        spec::Spec,
        test_identity::{fingerprint, rename_hint},
    },
};
use std::fs;

#[test]
fn revised_digests_preserve_literals_and_trailing_spaces() {
    assert_ne!(fingerprint("x = 'a  b'"), fingerprint("x = 'a b'"));
    assert_ne!(fingerprint("a  \nb"), fingerprint("a\nb"));
    assert_eq!(
        rename_hint("def test_one():\n    assert value"),
        rename_hint("def test_two():\n    assert value")
    );
    assert_ne!(revision("Count >= 3"), revision("Count > 3"));
}

#[test]
fn python_candidates_resolve_and_explain() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("test_export.py"), "class TestExports:\n    def test_export_timestamp(self):\n        assert export_timestamp() == 30\n").unwrap();
    let index = TestIndex::build(dir.path()).unwrap();
    assert_eq!(index.entries.len(), 1);
    let spec = Spec::parse("- [R-001] Export timestamp is 30.\n").unwrap();
    let proposals = propose(&spec, &index);
    assert_eq!(proposals.len(), 1);
    assert!(index.get(&proposals[0].test_id).is_some());
    assert!(!proposals[0].explanation.is_empty());
}
