use living_spec::{
    adapters::parse_libtest,
    agents::{
        auditor::{self, audit},
        test_index::{TestEntry, TestIndex},
    },
    boundary::{coverage::CaseLink, gen::generate, parse_bounds},
    core::{
        annotations::{Annotations, Audit, AuditVerdict, Dependency, Link, RequirementAnnotations},
        badge::{Badge, Reason},
        history::History,
        providers::BoundaryGapProvider,
        spec::Spec,
        test_identity::{load_fingerprints, Fingerprints, TestOutcome, TestStatus},
    },
    pipeline::rescore::{rescore, rescore_with_sources},
    runner::rescore::StatusDocument,
    runner::rescore::{rescore_with_providers, NoQualification, Result},
    web::{collect_rollup, ProjectState},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn link(entry: &TestEntry) -> Link {
    Link {
        extra: BTreeMap::new(),
        revision: String::new(),
        rename_hint: Some(entry.rename_hint.clone()),
        test_id: entry.id.clone(),
        fingerprint: entry.fingerprint.clone(),
        explanation: "fixture".into(),
        proposed_by: "linker".into(),
        audit: Audit {
            extra: BTreeMap::new(),
            revision: String::new(),
            verdict: AuditVerdict::Challenged,
            notes: vec![],
        },
    }
}

fn agent_cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .args(args)
        .output()
        .unwrap()
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pipeline/runner")
        .join(name)
}
fn copy_spec() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
    for name in ["spec.md", "annotations.json"] {
        fs::copy(fixture("spec").join(name), dir.path().join(name)).unwrap();
    }
    dir
}
fn fingerprints() -> Fingerprints {
    load_fingerprints(&fs::read_to_string(fixture("fingerprints.json")).unwrap()).unwrap()
}
fn cli(dir: &Path, results: &Path, format: &str, commit: &str, fp: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .arg("rescore")
        .arg(dir)
        .arg("--results")
        .arg(results)
        .args(["--format", format, "--commit", commit, "--fingerprints"])
        .arg(fp)
        .output()
        .unwrap()
}
fn status(dir: &Path) -> StatusDocument {
    serde_json::from_slice(&fs::read(dir.join("status.json")).unwrap()).unwrap()
}

fn history_contains_renamed_link(dir: &Path) -> bool {
    History::new(dir.join("history.jsonl"))
        .read()
        .unwrap()
        .iter()
        .any(|transition| transition.note.contains("renamed link"))
}

#[test]
fn parser_returns_one_requirement() {
    let spec = Spec::parse("- [R-001] A requirement.").unwrap();
    assert_eq!(spec.lines.len(), 1);
}

#[test]
fn dogfood_verdicts_match_the_auditor() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let spec_dir = root.join("living-spec");
    let spec = Spec::parse(&fs::read_to_string(spec_dir.join("spec.md")).unwrap()).unwrap();
    let annotations = Annotations::load(spec_dir.join("annotations.json")).unwrap();
    let index = TestIndex::build(root).unwrap();
    assert!(annotations.orphans(&spec).is_empty());
    for line in &spec.lines {
        let links = &annotations.requirements[&line.id].links;
        assert!(!links.is_empty(), "{} has no link", line.id);
        let mut admissible = false;
        for link in links {
            assert!(!link.audit.extra.contains_key("override"), "{}", line.id);
            let entry = index.get(&link.test_id).expect("dogfood link must resolve");
            assert_eq!(link.fingerprint, entry.fingerprint, "{}", line.id);
            assert_eq!(link.revision, line.revision, "{}", line.id);
            assert_eq!(link.audit.revision, line.revision, "{}", line.id);
            let fresh = auditor::audit(line, link, entry);
            assert_eq!(
                link.audit.verdict, fresh.verdict,
                "{}: {:?}",
                line.id, fresh.notes
            );
            admissible |= fresh.verdict == AuditVerdict::Accepted;
        }
        assert!(admissible, "{} has no admissible link", line.id);
    }
}

#[test]
fn score_and_rescore_reaudit_exact_links_and_expose_explicit_overrides() {
    use living_spec::core::badge::Reason;
    let temp = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
    let root = temp.path();
    fs::create_dir(root.join("tests")).unwrap();
    let spec_dir = root.join("living-spec");
    fs::create_dir(&spec_dir).unwrap();
    let text = "- [R-001] Export timestamps.\n";
    fs::write(spec_dir.join("spec.md"), text).unwrap();
    let line = Spec::parse(text).unwrap().lines.remove(0);
    let results = root.join("results.txt");
    fs::write(&results, "Running tests/export.rs (target/debug/deps/export-012abc)\nrunning 1 test\ntest timestamp ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n").unwrap();
    for command in ["score", "rescore"] {
        for author in ["human", "linker"] {
            for genuine in [false, true] {
                let source = if genuine {
                    "#[test]\nfn timestamp() { assert_eq!(export_timestamp(), 30); }"
                } else {
                    "#[test]\nfn timestamp() { assert_eq!(unrelated(), 30); }"
                };
                fs::write(root.join("tests/export.rs"), source).unwrap();
                let index = TestIndex::build(root).unwrap();
                let entry = &index.entries[0];
                for override_reason in [None, Some("Reviewed <scope> & limitation")] {
                    let mut extra = BTreeMap::new();
                    if let Some(reason) = override_reason {
                        extra.insert(
                            "override".into(),
                            serde_json::json!({"by": "human", "reason": reason,
                                "test_id":entry.id, "fingerprint":entry.fingerprint, "revision":line.revision}),
                        );
                    }
                    let link = Link {
                        test_id: entry.id.clone(),
                        fingerprint: entry.fingerprint.clone(),
                        rename_hint: Some(entry.rename_hint.clone()),
                        revision: line.revision.clone(),
                        explanation:
                            "Synthetic exact binding with an intentionally wrong stored verdict."
                                .into(),
                        proposed_by: author.into(),
                        audit: Audit {
                            verdict: if genuine {
                                AuditVerdict::Challenged
                            } else {
                                AuditVerdict::Accepted
                            },
                            revision: line.revision.clone(),
                            notes: vec!["hand-set verdict must be recomputed".into()],
                            extra,
                        },
                        extra: BTreeMap::new(),
                    };
                    Annotations {
                        schema: "living-spec.annotations/v1".into(),
                        requirements: BTreeMap::from([(
                            line.id.clone(),
                            RequirementAnnotations {
                                links: vec![link],
                                ..Default::default()
                            },
                        )]),
                        extra: BTreeMap::new(),
                    }
                    .save(spec_dir.join("annotations.json"))
                    .unwrap();
                    let output = Command::new(env!("CARGO_BIN_EXE_living-spec"))
                        .arg(command)
                        .arg(&spec_dir)
                        .arg("--results")
                        .arg(&results)
                        .args(["--format", "libtest", "--commit", "synthetic", "--tests"])
                        .arg(root)
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let document: StatusDocument =
                        serde_json::from_slice(&fs::read(spec_dir.join("status.json")).unwrap())
                            .unwrap();
                    let overridden = !genuine && override_reason.is_some();
                    assert_eq!(document.audit, "fresh");
                    assert_eq!(document.human_overrides, usize::from(overridden));
                    assert_eq!(
                        document.statuses[0].badge,
                        if genuine || overridden {
                            Badge::Proven
                        } else {
                            Badge::Unproven
                        }
                    );
                    assert_eq!(
                        document.statuses[0]
                            .reasons
                            .contains(&Reason::HumanOverride),
                        overridden
                    );
                    let saved = Annotations::load(spec_dir.join("annotations.json")).unwrap();
                    assert_eq!(
                        saved.requirements["R-001"].links[0].audit.verdict,
                        if genuine {
                            AuditVerdict::Accepted
                        } else {
                            AuditVerdict::Challenged
                        }
                    );
                    let page = fs::read_to_string(spec_dir.join("spec.html")).unwrap();
                    assert_eq!(
                        page.contains(
                            "accepted by human override: Reviewed &lt;scope&gt; &amp; limitation"
                        ),
                        overridden
                    );
                }
            }
        }
    }
}

fn copy_sources(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for item in fs::read_dir(from).unwrap() {
        let item = item.unwrap();
        if item.file_type().unwrap().is_dir() {
            copy_sources(&item.path(), &to.join(item.file_name()));
        } else {
            fs::copy(item.path(), to.join(item.file_name())).unwrap();
        }
    }
}

fn spec_with_all_providers() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir_in(root.join("target")).unwrap();
    let fixture = root.join("tests/fixtures/pipeline/runner");
    for file in ["spec.md", "annotations.json"] {
        fs::copy(fixture.join("spec").join(file), temp.path().join(file)).unwrap();
    }
    let path = temp.path().join("annotations.json");
    let mut annotations = Annotations::load(&path).unwrap();
    let bound = living_spec::boundary::parse_bounds("Duration is at most 30 ms.")
        .unwrap()
        .remove(0);
    annotations.requirements.get_mut("R-001").unwrap().bounds =
        vec![serde_json::to_value(bound).unwrap()];
    annotations
        .requirements
        .get_mut("R-002")
        .unwrap()
        .depends_on = vec![living_spec::core::annotations::Dependency {
        id: "R-001".into(),
        evidence: "authored: order depends on retaining the exported records".into(),
        extra: Default::default(),
    }];
    let claim = living_spec::quantifier::load(&living_spec::quantifier::reference_dir())
        .unwrap()
        .claims
        .remove(0);
    annotations
        .requirements
        .get_mut("R-002")
        .unwrap()
        .quantifier = Some(serde_json::to_value(claim.quantifier).unwrap());
    annotations.save(&path).unwrap();
    temp
}

fn score_with_all_providers(dir: &Path, command: &str) -> StatusDocument {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pipeline/runner");
    let html = dir.join("custom.html");
    let output = Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .arg(command)
        .arg(dir)
        .arg("--results")
        .arg(fixture.join("before.txt"))
        .args([
            "--format",
            "libtest",
            "--commit",
            "synthetic-push",
            "--fingerprints",
        ])
        .arg(fixture.join("fingerprints.json"))
        .arg("--html")
        .arg(&html)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: StatusDocument =
        serde_json::from_slice(&fs::read(dir.join("status.json")).unwrap()).unwrap();
    assert_eq!(document.statuses[0].badge, Badge::Unproven);
    assert_eq!(document.statuses[1].badge, Badge::Qualified);
    assert_eq!(document.quantifier_reports.len(), 1);
    assert_eq!(document.boundary_reports.len(), 1);
    assert!(document.quantifier_reports[0].coverage.tested_cases > 0);
    let page = fs::read_to_string(&html).unwrap();
    for label in [
        "Boundary coverage",
        "below",
        "at",
        "above",
        "untested",
        "Tested here",
        "Claimed everywhere",
        "R-002 → R-001",
    ] {
        assert!(page.contains(label), "missing {label}");
    }
    let dependencies = Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .args(["deps", "report"])
        .arg(dir)
        .arg("--status")
        .arg(dir.join("status.json"))
        .output()
        .unwrap();
    assert!(
        dependencies.status.success(),
        "{}",
        String::from_utf8_lossy(&dependencies.stderr)
    );
    document
}

#[test]
fn score_and_rescore_share_boundary_dependencies_and_quantifier_coverage() {
    let temp = spec_with_all_providers();
    let score = score_with_all_providers(temp.path(), "score");
    let rescore = score_with_all_providers(temp.path(), "rescore");
    assert_eq!(score.statuses, rescore.statuses);
    assert_eq!(score.boundary_reports.len(), 1);
    assert_eq!(score.quantifier_reports.len(), 1);
    assert_eq!(score.statuses[0].badge, Badge::Unproven);
    assert_eq!(score.statuses[1].badge, Badge::Qualified);
    assert_eq!(score.audit, "stored");
    assert_eq!(rescore.audit, "stored");
    assert_eq!(score.human_overrides, 0);
}

#[test]
fn score_detects_a_real_failure_in_a_temporary_crate_copy() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir_in(root.join("target")).unwrap();
    let project = temp.path();
    for file in ["Cargo.toml", "Cargo.lock"] {
        fs::copy(root.join(file), project.join(file)).unwrap();
    }
    copy_sources(&root.join("src"), &project.join("src"));
    fs::create_dir_all(project.join("tests")).unwrap();
    let original = TestIndex::build(root).unwrap();
    let entry = original
        .entries
        .iter()
        .find(|entry| entry.name == "parser_returns_one_requirement")
        .unwrap();
    let source = format!("use living_spec::core::spec::Spec;\n{}\n", entry.source);
    let test_path = project.join("tests/pipeline.rs");
    fs::write(&test_path, &source).unwrap();
    let spec_dir = project.join("living-spec");
    fs::create_dir_all(&spec_dir).unwrap();
    let text = "- [R-001] The spec parser returns requirement lines.\n";
    fs::write(spec_dir.join("spec.md"), text).unwrap();
    let line = Spec::parse(text).unwrap().lines.remove(0);
    let index = TestIndex::build(project).unwrap();
    let test = index.get(&entry.id).unwrap();
    let mut link = Link {
        test_id: test.id.clone(),
        fingerprint: test.fingerprint.clone(),
        rename_hint: Some(test.rename_hint.clone()),
        revision: line.revision.clone(),
        explanation: "Checks the parsed requirement count against an expected value.".into(),
        proposed_by: "human".into(),
        audit: Audit {
            verdict: AuditVerdict::Accepted,
            revision: line.revision.clone(),
            notes: vec![],
            extra: BTreeMap::new(),
        },
        extra: BTreeMap::new(),
    };
    link.audit = auditor::audit(&line, &link, test);
    assert_eq!(link.audit.verdict, AuditVerdict::Accepted);
    Annotations {
        schema: "living-spec.annotations/v1".into(),
        requirements: BTreeMap::from([(
            line.id,
            RequirementAnnotations {
                links: vec![link],
                ..Default::default()
            },
        )]),
        extra: BTreeMap::new(),
    }
    .save(spec_dir.join("annotations.json"))
    .unwrap();

    for broken in [false, true] {
        if broken {
            fs::write(
                &test_path,
                source.replace("spec.lines.len(), 1", "spec.lines.len(), 2"),
            )
            .unwrap();
        }
        let run = Command::new(env!("CARGO"))
            .current_dir(project)
            .args([
                "test",
                "--offline",
                "--locked",
                "--test",
                "pipeline",
                "--target-dir",
            ])
            .arg(root.join("target/pipeline-build"))
            .output()
            .unwrap();
        assert_eq!(
            run.status.success(),
            !broken,
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let mut results = run.stderr;
        results.extend(run.stdout);
        fs::write(project.join("results.txt"), results).unwrap();
        for command in ["score", "rescore"] {
            let output = Command::new(env!("CARGO_BIN_EXE_living-spec"))
                .current_dir(project)
                .args([
                    command,
                    "living-spec",
                    "--results",
                    "results.txt",
                    "--format",
                    "libtest",
                    "--tests",
                    ".",
                    "--commit",
                    "synthetic-push",
                ])
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(if broken { 2 } else { 0 }),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let document: StatusDocument =
                serde_json::from_slice(&fs::read(spec_dir.join("status.json")).unwrap()).unwrap();
            assert_eq!(
                document.statuses[0].badge,
                if broken { Badge::Broken } else { Badge::Proven }
            );
            assert!(spec_dir.join("spec.html").is_file());
            assert!(spec_dir.join("history.jsonl").is_file());
        }
        #[cfg(unix)]
        {
            let output = Command::new("sh")
                .arg(root.join("hooks/pre-push"))
                .current_dir(project)
                .env("TMPDIR", project)
                .env("LIVING_SPEC_BIN", env!("CARGO_BIN_EXE_living-spec"))
                .env("LIVING_SPEC_DIR", "living-spec")
                .env("LIVING_SPEC_TESTS", ".")
                .env_remove("LIVING_SPEC_FINGERPRINTS")
                .env("LIVING_SPEC_TEST_CMD", "cat results.txt")
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(if broken { 2 } else { 0 }),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    let annotations = Annotations::load(spec_dir.join("annotations.json")).unwrap();
    assert!(annotations.requirements["R-001"].links[0]
        .audit
        .notes
        .iter()
        .any(|note| note.contains("automatically re-audited")));
}

#[test]
fn rescore_audits_changed_rust_source_before_admitting_the_link() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("tests")).unwrap();
    let spec_dir = dir.path().join("living-spec");
    fs::create_dir(&spec_dir).unwrap();
    fs::write(spec_dir.join("spec.md"), "- [R-001] Export timestamps.\n").unwrap();
    let source = dir.path().join("tests/export.rs");
    fs::write(&source, "#[test]\nfn timestamp() { assert!(true); }").unwrap();
    let original = TestIndex::build(dir.path()).unwrap().entries.remove(0);
    let line = Spec::parse("- [R-001] Export timestamps.\n")
        .unwrap()
        .lines
        .remove(0);
    let old = link_for_revision(&original, &line);
    Annotations {
        schema: "living-spec.annotations/v1".into(),
        requirements: BTreeMap::from([(
            line.id.clone(),
            RequirementAnnotations {
                links: vec![old],
                ..Default::default()
            },
        )]),
        extra: BTreeMap::new(),
    }
    .save(spec_dir.join("annotations.json"))
    .unwrap();
    fs::write(
        &source,
        "#[test]\nfn timestamp() { let value = export_timestamp(); assert_eq!(value, 30); }",
    )
    .unwrap();
    let index = TestIndex::build(dir.path()).unwrap();
    let outcomes = [TestOutcome {
        test_id: original.id.clone(),
        status: TestStatus::Passed,
        duration_ms: None,
        failure_message: None,
    }];
    let report =
        rescore_with_sources(&spec_dir, &outcomes, &index, "synthetic-change", "fixture").unwrap();
    let saved = Annotations::load(spec_dir.join("annotations.json")).unwrap();
    assert_eq!(report.statuses[0].badge, Badge::Proven);
    assert_eq!(
        saved.requirements["R-001"].links[0].audit.verdict,
        AuditVerdict::Accepted
    );
    assert_ne!(
        saved.requirements["R-001"].links[0].fingerprint,
        original.fingerprint
    );
    assert_eq!(
        saved.requirements["R-001"].links[0].fingerprint,
        index.entries[0].fingerprint
    );
}

fn link_for_revision(entry: &TestEntry, line: &living_spec::core::spec::RequirementLine) -> Link {
    let mut old = link(entry);
    old.revision = line.revision.clone();
    old.audit = audit(line, &old, entry);
    old
}

#[test]
fn changed_test_is_reaudited() {
    use living_spec::{
        adapters::parse_libtest,
        core::badge::{Badge, Reason},
        pipeline::rescore::rescore,
        runner::rescore::StatusDocument,
    };
    let dir = tempfile::tempdir().unwrap();
    let spec_dir = dir.path().join("living-spec");
    fs::create_dir(&spec_dir).unwrap();
    fs::create_dir(dir.path().join("tests")).unwrap();
    fs::write(
        spec_dir.join("spec.md"),
        "- [R-001] Export timestamp is 30.\n",
    )
    .unwrap();
    let test_path = dir.path().join("tests/export.rs");
    fs::write(
        &test_path,
        "#[test]\nfn timestamp() { assert_eq!(export_timestamp(30), 30); }\n",
    )
    .unwrap();
    let spec = Spec::parse(&fs::read_to_string(spec_dir.join("spec.md")).unwrap()).unwrap();
    let original = TestIndex::build(dir.path()).unwrap().entries.remove(0);
    let mut authored = link(&original);
    authored.revision = spec.lines[0].revision.clone();
    authored.audit = audit(&spec.lines[0], &authored, &original);
    assert_eq!(authored.audit.verdict, AuditVerdict::Accepted);
    authored
        .extra
        .insert("unit".into(), serde_json::json!("seconds"));
    authored
        .audit
        .extra
        .insert("reviewer".into(), serde_json::json!("synthetic-reviewer"));
    let mut annotations = Annotations {
        schema: "living-spec.annotations/v1".into(),
        requirements: BTreeMap::new(),
        extra: BTreeMap::new(),
    };
    annotations
        .requirements
        .entry("R-001".into())
        .or_default()
        .links
        .push(authored);
    annotations.save(spec_dir.join("annotations.json")).unwrap();
    let output = "Running tests/export.rs (target/debug/deps/export-012abc)\nrunning 1 test\ntest timestamp ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";
    let results_path = dir.path().join("results.txt");
    fs::write(&results_path, output).unwrap();
    let outcomes = parse_libtest(output).unwrap();
    fs::write(
        &test_path,
        "#[test]\nfn timestamp() { assert!(export_timestamp(30) >= 30); }\n",
    )
    .unwrap();
    let changed = TestIndex::build(dir.path()).unwrap();
    assert_ne!(
        original.fingerprint,
        changed.get(&original.id).unwrap().fingerprint
    );
    let without_source = rescore(
        &spec_dir,
        &outcomes,
        &changed.identities(),
        "digest-only",
        "fixture",
    )
    .unwrap();
    assert_eq!(without_source.statuses[0].badge, Badge::Unproven);
    assert!(without_source.statuses[0]
        .reasons
        .contains(&Reason::TestChanged));
    for flag in ["--fingerprints-from", "--tests"] {
        // Each option must trigger re-audit from an old binding.
        annotations.save(spec_dir.join("annotations.json")).unwrap();
        let result = agent_cli(&[
            "rescore",
            spec_dir.to_str().unwrap(),
            "--results",
            results_path.to_str().unwrap(),
            "--format",
            "libtest",
            "--commit",
            "changed",
            flag,
            dir.path().to_str().unwrap(),
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let document: StatusDocument =
            serde_json::from_slice(&fs::read(spec_dir.join("status.json")).unwrap()).unwrap();
        assert_eq!(document.statuses[0].badge, Badge::Proven);
        let saved = Annotations::load(spec_dir.join("annotations.json")).unwrap();
        let refreshed = &saved.requirements["R-001"].links[0];
        assert_eq!(
            refreshed.fingerprint,
            changed.get(&original.id).unwrap().fingerprint
        );
        assert_eq!(refreshed.audit.verdict, AuditVerdict::Accepted);
        assert_eq!(refreshed.audit.revision, spec.lines[0].revision);
        assert_eq!(refreshed.extra["unit"], serde_json::json!("seconds"));
        assert_eq!(
            refreshed.audit.extra["reviewer"],
            serde_json::json!("synthetic-reviewer")
        );
        assert!(refreshed
            .audit
            .notes
            .iter()
            .any(|note| note.contains("automatically re-audited")));
    }
    // Even a passing harness case cannot make a changed placeholder admissible.
    fs::write(&test_path, "#[test]\nfn timestamp() { assert!(true); }\n").unwrap();
    let result = agent_cli(&[
        "rescore",
        spec_dir.to_str().unwrap(),
        "--results",
        results_path.to_str().unwrap(),
        "--format",
        "libtest",
        "--commit",
        "placeholder",
        "--tests",
        dir.path().to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let document: StatusDocument =
        serde_json::from_slice(&fs::read(spec_dir.join("status.json")).unwrap()).unwrap();
    assert_eq!(document.statuses[0].badge, Badge::Unproven);
    assert!(document.statuses[0]
        .reasons
        .contains(&Reason::LinkChallenged));
    let saved = Annotations::load(spec_dir.join("annotations.json")).unwrap();
    assert_eq!(
        saved.requirements["R-001"].links[0].audit.verdict,
        AuditVerdict::Challenged
    );
}

#[test]
fn junit_changed_test_is_reaudited() {
    use living_spec::{core::badge::Badge, runner::rescore::StatusDocument};
    let dir = tempfile::tempdir().unwrap();
    let spec_dir = dir.path().join("living-spec");
    fs::create_dir(&spec_dir).unwrap();
    fs::create_dir(dir.path().join("tests")).unwrap();
    fs::write(
        spec_dir.join("spec.md"),
        "- [R-001] Export timestamp is 30.\n",
    )
    .unwrap();
    let path = dir.path().join("tests/test_export.py");
    fs::write(&path, "class TestExport:\n    def test_timestamp(self):\n        assert export_timestamp(30) == 30\n").unwrap();
    let linked = agent_cli(&[
        "link",
        spec_dir.to_str().unwrap(),
        "--tests",
        dir.path().to_str().unwrap(),
        "--write",
    ]);
    assert!(linked.status.success());
    fs::write(&path, "class TestExport:\n    def test_timestamp(self):\n        assert export_timestamp(30) >= 30\n").unwrap();
    let results = dir.path().join("results.xml");
    fs::write(&results, "<testsuite tests=\"1\"><testcase classname=\"tests.test_export.TestExport\" name=\"test_timestamp\"/></testsuite>").unwrap();
    let result = agent_cli(&[
        "rescore",
        spec_dir.to_str().unwrap(),
        "--results",
        results.to_str().unwrap(),
        "--format",
        "junit",
        "--commit",
        "changed-python",
        "--tests",
        dir.path().to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let document: StatusDocument =
        serde_json::from_slice(&fs::read(spec_dir.join("status.json")).unwrap()).unwrap();
    assert_eq!(document.statuses[0].badge, Badge::Proven);
    assert_eq!(
        document.statuses[0].tests[0].test_id,
        "pytest:tests:tests/test_export.py:TestExport::test_timestamp"
    );
}

fn score_pytest_cases(fail: bool) {
    let dir = tempfile::tempdir_in("target").unwrap();
    let spec_dir = dir.path().join("spec");
    fs::create_dir(&spec_dir).unwrap();
    fs::create_dir(dir.path().join("tests")).unwrap();
    let spec_text = "- [R-001] Export timestamp is 30.\n";
    fs::write(spec_dir.join("spec.md"), spec_text).unwrap();
    fs::write(dir.path().join("tests/test_export.py"), "@pytest.mark.parametrize('value', [30, 31])\ndef test_timestamp(value):\n    assert export_timestamp(value) == value\n").unwrap();
    let spec = Spec::parse(spec_text).unwrap();
    let entry = TestIndex::build(dir.path()).unwrap().entries.remove(0);
    let mut authored = link(&entry);
    authored.revision = spec.lines[0].revision.clone();
    authored.audit = audit(&spec.lines[0], &authored, &entry);
    authored.audit.extra.insert(
        "override".into(),
        serde_json::json!({
            "by": "human", "reason": "Reviewed parametrized assertion", "test_id": entry.id,
            "fingerprint": entry.fingerprint, "revision": spec.lines[0].revision
        }),
    );
    let mut annotations = Annotations {
        schema: "living-spec.annotations/v1".into(),
        requirements: BTreeMap::new(),
        extra: BTreeMap::new(),
    };
    annotations
        .requirements
        .entry("R-001".into())
        .or_default()
        .links
        .push(authored);
    annotations.save(spec_dir.join("annotations.json")).unwrap();
    let results = dir.path().join("results.xml");
    fs::write(&results, format!("<testsuite tests=\"2\"><testcase classname=\"tests.test_export\" name=\"test_timestamp[30]\">{}</testcase><testcase classname=\"tests.test_export\" name=\"test_timestamp[31]\"/></testsuite>", if fail { "<failure/>" } else { "" })).unwrap();
    let output = agent_cli(&[
        "score",
        spec_dir.to_str().unwrap(),
        "--results",
        results.to_str().unwrap(),
        "--format",
        "junit",
        "--tests",
        dir.path().to_str().unwrap(),
    ]);
    assert_eq!(
        output.status.code(),
        Some(if fail { 2 } else { 0 }),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scored = status(&spec_dir);
    assert_eq!(
        scored.statuses[0].badge,
        if fail { Badge::Broken } else { Badge::Proven }
    );
    assert!(scored.statuses[0]
        .tests
        .iter()
        .any(|test| test.test_id == format!("{}#30", entry.id)));
    let rendered = agent_cli(&[
        "render",
        spec_dir.to_str().unwrap(),
        "--out",
        spec_dir.join("again.html").to_str().unwrap(),
    ]);
    assert!(rendered.status.success());
    assert!(!fs::read_to_string(spec_dir.join("again.html"))
        .unwrap()
        .contains("Stale: inputs changed"));
}

#[test]
fn pytest_param_case_failure_breaks_line() {
    score_pytest_cases(true);
}

#[test]
fn pytest_param_cases_all_pass_prove_line() {
    score_pytest_cases(false);
}

#[test]
fn provider_can_clear_boundary_gaps() {
    struct Covered;
    impl BoundaryGapProvider for Covered {
        fn gaps(
            &self,
            _: &living_spec::core::spec::Spec,
            _: &Annotations,
            _: &[living_spec::core::test_identity::TestOutcome],
        ) -> Result<BTreeSet<String>> {
            Ok(BTreeSet::new())
        }
    }
    let dir = copy_spec();
    let mut a = Annotations::load(dir.path().join("annotations.json")).unwrap();
    a.requirements.get_mut("R-001").unwrap().bounds.push(
        serde_json::to_value(
            &living_spec::boundary::parse_bounds("Duration is at most 30 ms.").unwrap()[0],
        )
        .unwrap(),
    );
    a.save(dir.path().join("annotations.json")).unwrap();
    let outcomes = parse_libtest(&fs::read_to_string(fixture("before.txt")).unwrap()).unwrap();
    assert_eq!(
        rescore(dir.path(), &outcomes, &fingerprints(), "a", "fixture")
            .unwrap()
            .statuses[0]
            .badge,
        Badge::Unproven
    );
    assert_eq!(
        rescore_with_providers(
            dir.path(),
            &outcomes,
            &fingerprints(),
            "b",
            "fixture",
            &Covered,
            &NoQualification
        )
        .unwrap()
        .statuses[0]
            .badge,
        Badge::Proven
    );
}

#[test]
fn rescore_applies_boundary_and_dependency_rules() {
    let dir = copy_spec();
    let setup = setup_boundary_dependency(dir.path());
    let report = rescore(
        dir.path(),
        &setup.outcomes,
        &fingerprints(),
        "gap",
        "fixture",
    )
    .unwrap();
    assert_eq!(report.statuses[0].badge, Badge::Unproven);
    assert_eq!(report.statuses[0].reasons, [Reason::BoundaryGap]);
    assert_eq!(report.statuses[1].badge, Badge::Qualified);
    assert_eq!(
        report.statuses[1].reasons,
        [Reason::PrerequisiteUnproven {
            path: vec!["R-002".into(), "R-001".into()],
        }]
    );
    let all_tests_passed = true;
    assert_eq!(all_report_tests_passed(&report), all_tests_passed);
    // Exercise the CLI's default providers too.
    let output = cli(
        dir.path(),
        &fixture("before.txt"),
        "libtest",
        "cli-gap",
        &fixture("fingerprints.json"),
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(status(dir.path()).statuses, report.statuses);

    let covered = add_boundary_cases(dir.path(), setup);
    assert_eq!(covered.statuses[0].badge, Badge::Proven);
    assert_eq!(covered.statuses[1].badge, Badge::Proven);
}

struct BoundaryDependencySetup {
    bound: living_spec::core::boundary::model::Bound,
    outcomes: Vec<living_spec::core::test_identity::TestOutcome>,
}

fn setup_boundary_dependency(dir: &Path) -> BoundaryDependencySetup {
    let path = dir.join("annotations.json");
    let mut annotations = Annotations::load(&path).unwrap();
    let bound = parse_bounds("Duration is at most 30 ms.")
        .unwrap()
        .remove(0);
    annotations.requirements.get_mut("R-001").unwrap().bounds =
        vec![serde_json::to_value(&bound).unwrap()];
    annotations
        .requirements
        .get_mut("R-002")
        .unwrap()
        .depends_on = vec![Dependency {
        id: "R-001".into(),
        evidence: "authored".into(),
        extra: Default::default(),
    }];
    annotations.save(&path).unwrap();
    BoundaryDependencySetup {
        bound,
        outcomes: parse_libtest(&fs::read_to_string(fixture("before.txt")).unwrap()).unwrap(),
    }
}

fn add_boundary_cases(
    dir: &Path,
    setup: BoundaryDependencySetup,
) -> living_spec::runner::rescore::RescoreReport {
    let path = dir.join("annotations.json");
    let mut annotations = Annotations::load(&path).unwrap();
    let mut outcomes = setup.outcomes;
    let links: Vec<_> = generate(&setup.bound)
        .unwrap()
        .into_iter()
        .map(|case| {
            let test_id = format!("libtest:fixture:tests/bounds.rs:{}", case.position);
            outcomes.push(living_spec::core::test_identity::TestOutcome {
                test_id: test_id.clone(),
                status: TestStatus::Passed,
                duration_ms: None,
                failure_message: None,
            });
            CaseLink {
                quantity: setup.bound.quantity.clone(),
                position: case.position,
                value: case.value,
                unit: case.unit,
                test_id,
                test_body: None,
            }
        })
        .collect();
    annotations
        .requirements
        .get_mut("R-001")
        .unwrap()
        .extra
        .insert(
            "boundary_links".into(),
            serde_json::to_value(links).unwrap(),
        );
    annotations.save(&path).unwrap();
    rescore(dir, &outcomes, &fingerprints(), "covered", "fixture").unwrap()
}

fn all_report_tests_passed(report: &living_spec::runner::rescore::RescoreReport) -> bool {
    report
        .statuses
        .iter()
        .all(|status| status.tests[0].status == TestStatus::Passed)
}

#[test]
fn fingerprint_only_json_fallback_supports_exact_resolution() {
    let dir = copy_spec();
    let entries: std::collections::BTreeMap<_, _> = fingerprints()
        .into_iter()
        .map(|(id, identity)| (id, identity.fingerprint))
        .collect();
    let fp = load_fingerprints(&serde_json::to_string(&entries).unwrap()).unwrap();
    let outcomes = parse_libtest(&fs::read_to_string(fixture("before.txt")).unwrap()).unwrap();
    let report = rescore(dir.path(), &outcomes, &fp, "commit", "fixture").unwrap();
    assert!(report.statuses.iter().all(|s| s.badge == Badge::Proven));
}

#[test]
fn simulated_push_flips_line_red() {
    let dir = copy_spec();
    let before = cli(
        dir.path(),
        &fixture("before.txt"),
        "libtest",
        "passing-commit",
        &fixture("fingerprints.json"),
    );
    assert_eq!(
        before.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&before.stderr)
    );
    assert!(status(dir.path())
        .statuses
        .iter()
        .all(|s| s.badge == Badge::Proven));
    let after = cli(
        dir.path(),
        &fixture("after.txt"),
        "libtest",
        "breaking-commit",
        &fixture("fingerprints.json"),
    );
    assert_eq!(after.status.code(), Some(2));
    let text = String::from_utf8(after.stdout).unwrap();
    assert!(text.contains("R-001 proven -> broken"));
    assert!(text.contains("assertion failed: exported timestamp differs"));
    assert_eq!(status(dir.path()).statuses[0].badge, Badge::Broken);
    let history = History::new(dir.path().join("history.jsonl"))
        .read()
        .unwrap();
    assert!(history.iter().any(|t| t.from == Badge::Proven
        && t.to == Badge::Broken
        && t.commit == "breaking-commit"
        && t.test_id.ends_with("::timestamps")));
    let again = cli(
        dir.path(),
        &fixture("after.txt"),
        "libtest",
        "later-commit",
        &fixture("fingerprints.json"),
    );
    assert_eq!(again.status.code(), Some(2));
    assert!(String::from_utf8(again.stdout)
        .unwrap()
        .contains("R-001 already broken"));
    let later_history = History::new(dir.path().join("history.jsonl"))
        .read()
        .unwrap();
    assert_eq!(&later_history[..history.len()], &history);
    assert!(later_history[history.len()..]
        .iter()
        .all(|t| t.from == t.to && t.commit == "later-commit"));
}

#[test]
fn renamed_link_reaudited_in_pipeline() {
    use living_spec::pipeline::rescore::rescore_with_sources;
    let dir = tempfile::tempdir_in("target").unwrap();
    let root = dir.path();
    fs::create_dir(root.join("tests")).unwrap();
    let spec_dir = root.join("spec");
    fs::create_dir(&spec_dir).unwrap();
    let text = "- [R-001] Export timestamps.\n";
    fs::write(spec_dir.join("spec.md"), text).unwrap();
    let line = Spec::parse(text).unwrap().lines.remove(0);
    let path = root.join("tests/export.rs");
    for accepted in [true, false] {
        let assertion = if accepted {
            "assert_eq!(export_timestamp(), 30);"
        } else {
            "assert!(true);"
        };
        fs::write(&path, format!("#[test]\nfn old() {{ {assertion} }}\n")).unwrap();
        let old = TestIndex::build(root).unwrap().entries.remove(0);
        let mut authored = link(&old);
        authored.revision = line.revision.clone();
        authored.audit.verdict = AuditVerdict::Accepted;
        authored.audit.revision = line.revision.clone();
        if !accepted {
            authored.audit.extra.insert(
                "override".into(),
                serde_json::json!({"by": "human", "reason": "old source reviewed"}),
            );
        }
        let mut annotations = Annotations {
            schema: "living-spec.annotations/v1".into(),
            requirements: BTreeMap::new(),
            extra: BTreeMap::new(),
        };
        annotations
            .requirements
            .entry(line.id.clone())
            .or_default()
            .links
            .push(authored);
        annotations.save(spec_dir.join("annotations.json")).unwrap();
        fs::write(&path, format!("#[test]\nfn new() {{ {assertion} }}\n")).unwrap();
        let index = TestIndex::build(root).unwrap();
        let entry = &index.entries[0];
        let outcomes = vec![living_spec::core::test_identity::TestOutcome {
            test_id: entry.id.clone(),
            status: TestStatus::Passed,
            duration_ms: None,
            failure_message: None,
        }];
        let result =
            rescore_with_sources(&spec_dir, &outcomes, &index, "rename", "fixture").unwrap();
        assert_eq!(result.renamed_links.len(), 1);
        assert_eq!(
            result.statuses[0].badge,
            if accepted {
                Badge::Proven
            } else {
                Badge::Unproven
            }
        );
        let saved = Annotations::load(spec_dir.join("annotations.json")).unwrap();
        let link = &saved.requirements["R-001"].links[0];
        assert_eq!(link.test_id, entry.id);
        assert!(!link.audit.extra.contains_key("override"));
        assert_eq!(
            link.audit.verdict,
            if accepted {
                AuditVerdict::Accepted
            } else {
                AuditVerdict::Challenged
            }
        );
    }
}

#[test]
fn first_run_broken_exits_2() {
    let dir = copy_spec();
    let result = cli(
        dir.path(),
        &fixture("after.txt"),
        "libtest",
        "first-red",
        &fixture("fingerprints.json"),
    );
    assert_eq!(result.status.code(), Some(2));
    assert!(History::new(dir.path().join("history.jsonl"))
        .read()
        .unwrap()
        .iter()
        .any(|t| t.from == Badge::Unproven && t.to == Badge::Broken && t.commit == "first-red"));
}

#[test]
fn renamed_link_is_rewritten_with_history_note() {
    let dir = copy_spec();
    let annotations_path = dir.path().join("annotations.json");
    let mut original = Annotations::load(&annotations_path).unwrap();
    original.requirements.get_mut("R-001").unwrap().links[0]
        .extra
        .insert("unit".into(), serde_json::json!("ticks"));
    original.save(&annotations_path).unwrap();
    let result = cli(
        dir.path(),
        &fixture("renamed.xml"),
        "junit",
        "renaming-commit",
        &fixture("renamed-fingerprints.json"),
    );
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let annotations = Annotations::load(dir.path().join("annotations.json")).unwrap();
    let renamed_identity_is_present = true;
    assert_eq!(
        annotations.requirements["R-001"].links[0]
            .test_id
            .ends_with("::timestamps_renamed"),
        renamed_identity_is_present
    );
    assert_eq!(
        annotations.requirements["R-001"].links[0].extra["unit"],
        "ticks"
    );
    let future_metadata_is_preserved = true;
    assert_eq!(
        annotations.requirements["R-001"].extra["future"]["preserved"],
        future_metadata_is_preserved
    );
    let scored = status(dir.path());
    assert_eq!(scored.statuses[0].badge, Badge::Unproven);
    let test_changed_reason_is_present = true;
    assert_eq!(
        scored.statuses[0].reasons.contains(&Reason::TestChanged),
        test_changed_reason_is_present
    );
    let again = cli(
        dir.path(),
        &fixture("renamed.xml"),
        "junit",
        "later",
        &fixture("renamed-fingerprints.json"),
    );
    assert_eq!(again.status.code(), Some(0));
    assert_eq!(status(dir.path()).statuses[0].badge, Badge::Unproven);
    let renamed_link_history_note_is_present = true;
    assert_eq!(
        history_contains_renamed_link(dir.path()),
        renamed_link_history_note_is_present
    );
}

#[test]
fn garbled_results_exit_1_without_writing_status() {
    let dir = copy_spec();
    let file = dir.path().join("garbled.txt");
    fs::write(&file, "unrelated output").unwrap();
    let result = cli(
        dir.path(),
        &file,
        "libtest",
        "commit",
        &fixture("fingerprints.json"),
    );
    assert_eq!(result.status.code(), Some(1));
    let output_files_absent = false;
    assert_eq!(dir.path().join("status.json").exists(), output_files_absent);
    assert_eq!(
        dir.path().join("history.jsonl").exists(),
        output_files_absent
    );
}

#[test]
fn malformed_previous_status_is_not_reset() {
    let dir = copy_spec();
    fs::write(dir.path().join("status.json"), "broken json").unwrap();
    let result = cli(
        dir.path(),
        &fixture("before.txt"),
        "libtest",
        "commit",
        &fixture("fingerprints.json"),
    );
    assert_eq!(result.status.code(), Some(1));
    let history_file_absent = false;
    assert_eq!(
        dir.path().join("history.jsonl").exists(),
        history_file_absent
    );
}

#[test]
fn default_commit_uses_specs_repository_or_uncommitted() {
    let dir = copy_spec();
    let result = Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .arg("rescore")
        .arg(dir.path())
        .arg("--results")
        .arg(fixture("before.txt"))
        .args(["--format", "libtest", "--fingerprints"])
        .arg(fixture("fingerprints.json"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(0));
    let expected = Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(
        status(dir.path()).commit,
        String::from_utf8(expected.stdout).unwrap().trim()
    );
}

#[test]
fn opt_in_hook_blocks_red_and_failed_commands() {
    let dir = copy_spec();
    let hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("hooks/pre-push");
    let run = |file: &str, fail: bool| {
        let source = dir.path().join("captured.txt");
        fs::copy(fixture(file), &source).unwrap();
        Command::new("sh")
            .arg(&hook)
            .env("TMPDIR", dir.path())
            .env("LIVING_SPEC_DIR", dir.path())
            .env("LIVING_SPEC_BIN", env!("CARGO_BIN_EXE_living-spec"))
            .env("LIVING_SPEC_FINGERPRINTS", fixture("fingerprints.json"))
            .env(
                "LIVING_SPEC_TEST_CMD",
                if fail {
                    "cat \"$RESULT_FIXTURE\"; exit 7"
                } else {
                    "cat \"$RESULT_FIXTURE\""
                },
            )
            .env("RESULT_FIXTURE", &source)
            .output()
            .unwrap()
    };
    assert_eq!(run("before.txt", false).status.code(), Some(0));
    assert_eq!(run("after.txt", true).status.code(), Some(2));
    assert_eq!(run("before.txt", true).status.code(), Some(1));
}

#[test]
fn hook_keeps_results_file_so_status_stays_fresh() {
    let dir = copy_spec();
    fs::write(dir.path().join(".gitignore"), "existing-rule").unwrap();
    let result = Command::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("hooks/pre-push"))
        .env("LIVING_SPEC_DIR", dir.path())
        .env("LIVING_SPEC_BIN", env!("CARGO_BIN_EXE_living-spec"))
        .env("LIVING_SPEC_FINGERPRINTS", fixture("fingerprints.json"))
        .env("LIVING_SPEC_TEST_CMD", "cat \"$RESULT_FIXTURE\"")
        .env("RESULT_FIXTURE", fixture("before.txt"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(dir.path().join(".results/latest.txt").is_file());
    assert_eq!(
        fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
        "existing-rule\n/.results/\n"
    );
    let rendered = agent_cli(&[
        "render",
        dir.path().to_str().unwrap(),
        "--out",
        dir.path().join("again.html").to_str().unwrap(),
    ]);
    assert!(rendered.status.success());
    assert!(!fs::read_to_string(dir.path().join("again.html"))
        .unwrap()
        .contains("Stale: inputs changed"));
}

#[test]
fn finding_2_documented_hook_invokes_built_runner_and_preserves_exit_contract() {
    let hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("hooks/pre-push");
    for (input, expected) in [("before.txt", 0), ("after.txt", 2), ("malformed", 1)] {
        // Each case starts without status/history, including the broken case.
        let dir = copy_spec();
        let source = dir.path().join("captured.txt");
        if input == "malformed" {
            fs::write(&source, "unrelated output\n").unwrap();
        } else {
            fs::copy(fixture(input), &source).unwrap();
        }
        let run = || {
            Command::new("sh")
                .arg(&hook)
                .env("TMPDIR", dir.path())
                .env("LIVING_SPEC_DIR", dir.path())
                .env("LIVING_SPEC_BIN", env!("CARGO_BIN_EXE_living-spec"))
                .env("LIVING_SPEC_FINGERPRINTS", fixture("fingerprints.json"))
                .env("LIVING_SPEC_FORMAT", "libtest")
                .env("LIVING_SPEC_TEST_CMD", "cat \"$RESULT_FIXTURE\"")
                .env("RESULT_FIXTURE", &source)
                .output()
                .unwrap()
        };
        let result = run();
        assert_eq!(
            result.status.code(),
            Some(expected),
            "{input}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        if expected == 1 {
            assert!(!dir.path().join("status.json").exists());
            assert!(!dir.path().join("history.jsonl").exists());
            assert!(
                String::from_utf8_lossy(&result.stderr).contains("results could not be rescored")
            );
        } else {
            assert_eq!(
                status(dir.path()).statuses[0].badge,
                if expected == 0 {
                    Badge::Proven
                } else {
                    Badge::Broken
                }
            );
            assert!(!History::new(dir.path().join("history.jsonl"))
                .read()
                .unwrap()
                .is_empty());
            if expected == 2 {
                assert!(
                    String::from_utf8_lossy(&result.stdout).contains("R-001 unproven -> broken")
                );
                let again = run();
                assert_eq!(again.status.code(), Some(2));
                assert!(String::from_utf8_lossy(&again.stdout).contains("R-001 already broken"));
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn pre_push_hook_blocks_a_broken_scored_line() {
    let dir = copy_spec();
    let hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("hooks/pre-push");
    let result = Command::new("sh")
        .arg(&hook)
        .env("TMPDIR", dir.path())
        .env("LIVING_SPEC_DIR", dir.path())
        .env("LIVING_SPEC_BIN", env!("CARGO_BIN_EXE_living-spec"))
        .env("LIVING_SPEC_FINGERPRINTS", fixture("fingerprints.json"))
        .env("LIVING_SPEC_TEST_CMD", "cat \"$RESULT_FIXTURE\"")
        .env("RESULT_FIXTURE", fixture("after.txt"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(status(dir.path()).statuses[0].badge, Badge::Broken);
}

#[test]
fn hook_blocks_a_broken_line() {
    let dir = copy_spec();
    let hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("hooks/pre-push");
    let result = Command::new("sh")
        .arg(&hook)
        .env("TMPDIR", dir.path())
        .env("LIVING_SPEC_DIR", dir.path())
        .env("LIVING_SPEC_BIN", env!("CARGO_BIN_EXE_living-spec"))
        .env("LIVING_SPEC_FINGERPRINTS", fixture("fingerprints.json"))
        .env("LIVING_SPEC_TEST_CMD", "cat \"$RESULT_FIXTURE\"")
        .env("RESULT_FIXTURE", fixture("after.txt"))
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(status(dir.path()).statuses[0].badge, Badge::Broken);
}

#[test]
fn invalid_dependencies_fail_rescore_before_writes() {
    let dir = copy_spec();
    let path = dir.path().join("annotations.json");
    let mut annotations = Annotations::load(&path).unwrap();
    annotations
        .requirements
        .get_mut("R-002")
        .unwrap()
        .depends_on
        .push(living_spec::core::annotations::Dependency {
            id: "R-999".into(),
            evidence: "authored".into(),
            extra: Default::default(),
        });
    annotations.save(&path).unwrap();
    let outcomes = parse_libtest(&fs::read_to_string(fixture("before.txt")).unwrap()).unwrap();
    assert!(rescore(dir.path(), &outcomes, &fingerprints(), "invalid", "fixture").is_err());
    assert!(!dir.path().join("history.jsonl").exists());
    assert!(!dir.path().join("status.json").exists());
}

#[test]
fn rollup_includes_nested_crate_spec_from_project_root() {
    // A live test capture is incomplete while this suite runs, so exercise the
    // nested crate layout with its own complete, current scoring inputs.
    let temp = tempfile::tempdir_in("target").unwrap();
    let repo = temp.path().join("nested-project");
    let spec_dir = repo.join("living-spec/living-spec");
    fs::create_dir_all(&spec_dir).unwrap();
    fs::write(repo.join("living-spec/Cargo.toml"), "[package]\n").unwrap();
    for name in ["spec.md", "annotations.json"] {
        fs::copy(fixture("spec").join(name), spec_dir.join(name)).unwrap();
    }
    let results = fixture("before.txt");
    let outcomes = parse_libtest(&fs::read_to_string(&results).unwrap()).unwrap();
    rescore(
        &spec_dir,
        &outcomes,
        &fingerprints(),
        "nested-fixture",
        results.to_str().unwrap(),
    )
    .unwrap();
    let rows = collect_rollup(
        Path::new("tests/fixtures/pipeline/registry/portfolio-truth-mini.json"),
        std::slice::from_ref(&repo),
    )
    .unwrap();
    let row = rows
        .iter()
        .find(|row| row.path == repo.display().to_string())
        .unwrap();
    assert_eq!(row.state, ProjectState::Scored);
    assert!(
        row.proven > 0,
        "included nested spec should have proven lines"
    );
    assert!(row.proven_pct > 0.0);
}
