use living_spec::{adapters::parse_libtest, agents::test_index::TestIndex};
use std::{fs, path::Path};

#[test]
fn explicit_library_path_is_independent_of_manifest_section_order() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Cargo.toml"), "[lib]\npath = \"code/library.rs\"\n\n[package]\nname = \"late-package\"\nversion = \"0.1.0\"\n").unwrap();
    fs::create_dir_all(root.path().join("code")).unwrap();
    fs::write(
        root.path().join("code/library.rs"),
        "#[test]\nfn smoke() {}\n",
    )
    .unwrap();
    let index = TestIndex::build(root.path()).unwrap();
    assert_eq!(index.entries.len(), 1);
    assert_eq!(
        index.entries[0].id,
        "libtest:late_package:code/library.rs:smoke"
    );
}

#[test]
fn index_and_adapter_ids_agree_for_bin_targets() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"sample-app\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"package_lib\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("src/bin/tool")).unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        "#[test]\nfn lib_name_only() { assert!(works()); }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src/bin/tool.rs"),
        "#[test]\nfn smoke() { assert_eq!(run(), 1); }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src/bin/hyphen-auto.rs"),
        "#[test]\nfn hyphen_auto_test() {}\n",
    )
    .unwrap();
    let index = TestIndex::build(root.path()).unwrap();
    let entry = index
        .entries
        .iter()
        .find(|entry| entry.name == "smoke")
        .unwrap();
    assert_eq!(entry.id, "libtest:tool:src/bin/tool.rs:smoke");
    let output = "Running unittests src/bin/tool.rs (target/debug/deps/tool-012abc)\nrunning 1 test\ntest smoke ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
    let outcomes = parse_libtest(output).unwrap();
    assert_eq!(outcomes[0].test_id, entry.id);
    assert!(index
        .get("libtest:package_lib:src/lib.rs:lib_name_only")
        .is_some());
    assert!(index
        .get("libtest:hyphen_auto:src/bin/hyphen-auto.rs:hyphen_auto_test")
        .is_some());
    fs::write(
        root.path().join("src/main.rs"),
        "mod root_support;\nmod outer { mod inner; }\n#[test]\nfn package_main_smoke() { assert!(works()); }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src/root_support.rs"),
        "#[test]\nfn package_main_module() { assert!(works()); }\n",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("src/outer")).unwrap();
    fs::write(
        root.path().join("src/outer/inner.rs"),
        "#[test]\nfn inline_module_child() { assert!(works()); }\n",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("src/bin/directory_tool")).unwrap();
    fs::write(
        root.path().join("src/bin/directory_tool/main.rs"),
        "mod sibling;\n#[test]\nfn directory_smoke() { assert!(works()); }\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src/bin/directory_tool/sibling.rs"),
        "#[test]\nfn sibling_smoke() { assert!(works()); }\n",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("src/bin/directory-default")).unwrap();
    fs::write(
        root.path().join("src/bin/directory-default/main.rs"),
        "#[test]\nfn directory_default_test() { assert!(works()); }\n",
    )
    .unwrap();
    let index = TestIndex::build(root.path()).unwrap();
    assert!(index
        .get("libtest:sample_app:src/main.rs:root_support::package_main_module")
        .is_some());
    assert!(index
        .get("libtest:sample_app:src/main.rs:outer::inner::inline_module_child")
        .is_some());
    assert!(index
        .get("libtest:directory_tool:src/bin/directory_tool/main.rs:sibling::sibling_smoke")
        .is_some());

    let second = tempfile::tempdir().unwrap();
    fs::write(
        second.path().join("Cargo.toml"),
        "[package]\nname = \"custom\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"chosen_lib\"\npath = \"code/library.rs\"\n\n[[bin]]\nname = \"chosen_tool\"\npath = \"commands/cli.rs\"\n\n[[bin]]\nname = \"root_path\"\npath = \"root_entry.rs\"\n\n[[bin]]\nname = \"path_default\"\n\n[[bin]]\nname = \"hyphen-tool\"\n\n[[bin]]\nname = \"directory-default\"\n\n[[test]]\nname = \"chosen_checks\"\npath = \"checks/smoke.rs\"\n\n[[test]]\nname = \"auto_checks\"\n",
    )
    .unwrap();
    for (path, expected) in [
        (
            "code/library.rs",
            "libtest:chosen_lib:code/library.rs:library_test",
        ),
        (
            "commands/cli.rs",
            "libtest:chosen_tool:commands/cli.rs:binary_test",
        ),
        (
            "commands/helper.rs",
            "libtest:chosen_tool:commands/cli.rs:helper::nested_binary_test",
        ),
        (
            "root_entry.rs",
            "libtest:root_path:root_entry.rs:root_path_test",
        ),
        (
            "root_helper.rs",
            "libtest:root_path:root_entry.rs:root_helper::root_module_test",
        ),
        (
            "checks/smoke.rs",
            "libtest:chosen_checks:checks/smoke.rs:integration_test",
        ),
        (
            "src/bin/path_default.rs",
            "libtest:path_default:src/bin/path_default.rs:default_bin",
        ),
        (
            "src/bin/hyphen-tool.rs",
            "libtest:hyphen_tool:src/bin/hyphen-tool.rs:hyphen_bin",
        ),
        (
            "src/bin/directory-default/main.rs",
            "libtest:directory_default:src/bin/directory-default/main.rs:directory_default_test",
        ),
        (
            "tests/auto_checks.rs",
            "libtest:auto_checks:tests/auto_checks.rs:default_test",
        ),
    ] {
        let source = format!(
            "{}#[test]\nfn {}() {{ assert!(works()); }}\n",
            match path {
                "commands/cli.rs" => "mod helper;\n",
                "root_entry.rs" => "mod root_helper;\n",
                _ => "",
            },
            match path {
                "code/library.rs" => "library_test",
                "commands/cli.rs" => "binary_test",
                "commands/helper.rs" => "nested_binary_test",
                "root_entry.rs" => "root_path_test",
                "root_helper.rs" => "root_module_test",
                "checks/smoke.rs" => "integration_test",
                "src/bin/path_default.rs" => "default_bin",
                "src/bin/hyphen-tool.rs" => "hyphen_bin",
                "src/bin/directory-default/main.rs" => "directory_default_test",
                _ => "default_test",
            },
        );
        let file = second.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        let index = TestIndex::build(Path::new(second.path())).unwrap();
        assert!(
            index.get(expected).is_some(),
            "{path}: {:?}",
            index.entries.iter().map(|e| &e.id).collect::<Vec<_>>()
        );
    }
}

#[test]
fn unknown_cargo_rust_layout_has_no_guessed_identity() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"sample\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("custom")).unwrap();
    fs::write(
        root.path().join("custom/orphan.rs"),
        "#[test]\nfn smoke() {}\n",
    )
    .unwrap();
    assert!(TestIndex::build(root.path()).unwrap().entries.is_empty());

    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/main.rs"),
        "stringify!(mod hidden;);\n#[path = \"custom.rs\"] mod custom;\n",
    )
    .unwrap();
    fs::write(
        root.path().join("custom.rs"),
        "#[test]\nfn custom_path_test() {}\n",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/hidden.rs"),
        "#[test]\nfn macro_hidden_test() {}\n",
    )
    .unwrap();
    assert!(TestIndex::build(root.path()).unwrap().entries.is_empty());
}

#[test]
fn module_resolution_ignores_macro_tokens_and_custom_paths() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"sample\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/main.rs"),
        "stringify!(mod hidden;);\n#[test]\nfn root_test() {}\n",
    )
    .unwrap();
    fs::write(
        root.path().join("src/hidden.rs"),
        "#[test]\nfn macro_hidden_test() {}\n",
    )
    .unwrap();
    let index = TestIndex::build(root.path()).unwrap();
    assert_eq!(index.entries.len(), 1);
    assert_eq!(index.entries[0].id, "libtest:sample:src/main.rs:root_test");

    fs::write(
        root.path().join("src/main.rs"),
        "#[path = \"../custom.rs\"] mod custom;\n#[test]\nfn root_test() {}\n",
    )
    .unwrap();
    fs::write(
        root.path().join("custom.rs"),
        "#[test]\nfn custom_path_test() {}\n",
    )
    .unwrap();
    let index = TestIndex::build(root.path()).unwrap();
    assert_eq!(index.entries.len(), 1);
    assert_eq!(index.entries[0].id, "libtest:sample:src/main.rs:root_test");
}
