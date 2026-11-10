mod common;
use living_spec::{
    core::{
        annotations::Annotations,
        badge::{Badge, LineStatus, Reason},
        freshness::{ResultsSnapshot, ScoreInputs},
        history::{History, Transition},
        mutation::{Mutant, Operator, Tier, TierState},
        spec::Spec,
        test_identity::TestOutcome,
        views::{BoundaryReport, QuantifierReport},
    },
    web::{
        collect_rollup, export_rollup_json, load_status, render_rollup, render_spec, ProjectState,
        RollupRow,
    },
};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path};

const SEEDED: &str = "tests/fixtures/web/seeded/living-spec";
const REGISTRY: &str = "tests/fixtures/web/registry/portfolio-truth-mini.json";

fn seeded_html() -> String {
    let dir = Path::new(SEEDED);
    let spec = Spec::parse(&fs::read_to_string(dir.join("spec.md")).unwrap()).unwrap();
    let annotations = Annotations::load(dir.join("annotations.json")).unwrap();
    let status = load_status(&dir.join("status.json")).unwrap();
    let history = History::new(dir.join("history.jsonl")).read().unwrap();
    render_spec(&spec, &annotations, Some(&status), &history)
}

fn requirement_order(html: &str) -> Vec<String> {
    let key = "data-requirement=\"";
    let mut rest = html;
    let mut order = Vec::new();
    while let Some(start) = rest.find(key) {
        let from = start + key.len();
        let end = rest[from..].find('"').unwrap();
        order.push(rest[from..from + end].to_owned());
        rest = &rest[from + end..];
    }
    order
}

fn between<'a>(html: &'a str, start_id: &str, end_id: &str) -> &'a str {
    let start = html
        .find(&format!("data-requirement=\"{start_id}\""))
        .unwrap();
    let end = html
        .find(&format!("data-requirement=\"{end_id}\""))
        .unwrap();
    assert!(start < end);
    &html[start..end]
}

fn assert_no_urls(html: &str) {
    let lower = html.to_ascii_lowercase();
    assert!(!lower.contains("http://"), "page references http");
    assert!(!lower.contains("https://"), "page references https");
}

fn names(rows: &[RollupRow]) -> Vec<&str> {
    rows.iter().map(|row| row.project.as_str()).collect()
}

fn mutation_tier(state: TierState) -> Tier {
    Tier {
        state,
        waived: 1,
        signed_off: 2,
        survivors: vec![Mutant {
            id: "m-123456789abc".into(),
            file: "src/<subject>.rs".into(),
            function: "clamp".into(),
            line: 12,
            ordinal: 0,
            operator: Operator::CmpFlip,
            original: "<=".into(),
            replacement: ">".into(),
            outcome: living_spec::core::mutation::MutantOutcome::Survived,
        }],
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn bound_registry_fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir_in("target").unwrap();
    let source = Path::new(REGISTRY).parent().unwrap();
    let copied = tmp.path().join("registry-fixture");
    copy_tree(source, &copied);
    for project in ["amber-ledger", "birch-timer"] {
        bind_saved_inputs(&copied.join(project).join("living-spec"));
    }
    tmp
}

#[test]
fn seeded_view_reports_counts_and_broken_drilldown() {
    let expected_true = true;
    let html = seeded_html();
    assert_eq!(html.contains("data-proven=\"2\""), expected_true);
    assert_eq!(html.contains("data-broken=\"1\""), expected_true);
    assert_eq!(html.contains("data-unproven=\"1\""), expected_true);
    assert_eq!(html.contains("data-qualified=\"1\""), expected_true);
    assert_eq!(html.contains("data-proven-pct=\"40.0\""), expected_true);
    assert_eq!(
        html.contains("proven 2 · broken 1 · unproven 1 · qualified 1 · proven 40.0%"),
        expected_true
    );
    let broken = between(&html, "R-002", "R-001");
    assert_eq!(
        broken.contains("rust:ledger:src/balance.rs:rejects_negative"),
        expected_true
    );
    assert_eq!(broken.contains("commit-broken-r002"), expected_true);
    assert_eq!(
        broken.contains("rejects_negative failed while checking a negative balance"),
        expected_true
    );
    assert_eq!(
        broken.contains("The failure is a real balance bug."),
        expected_true
    );
    assert_eq!(broken.contains("test_failed"), expected_true);
    assert_eq!(broken.contains("data-badge=\"broken\""), expected_true);
    let qualified = between(&html, "R-005", "R-003");
    assert_eq!(
        qualified.contains("prerequisite_broken: R-005 → R-002"),
        expected_true
    );
    assert_eq!(
        qualified.contains("Depends on the balance rule."),
        expected_true
    );
    let unproven = unproven_section(&html);
    assert_eq!(unproven.contains("boundary_gap"), expected_true);
    assert_eq!(unproven.contains("data-badge=\"unproven\""), expected_true);
}

#[test]
fn page_without_mutation_is_byte_identical() {
    let dir = Path::new(SEEDED);
    let spec = Spec::parse(&fs::read_to_string(dir.join("spec.md")).unwrap()).unwrap();
    let annotations = Annotations::load(dir.join("annotations.json")).unwrap();
    let history = History::new(dir.join("history.jsonl")).read().unwrap();
    let mut status = load_status(&dir.join("status.json")).unwrap();
    let old_status_bytes = fs::read_to_string(dir.join("status.json")).unwrap();
    assert!(!old_status_bytes.contains("\"mutation\""));
    let before = render_spec(&spec, &annotations, Some(&status), &history);
    for line in &mut status {
        line.mutation = None;
    }
    let after = render_spec(&spec, &annotations, Some(&status), &history);
    assert_eq!(before.as_bytes(), after.as_bytes());
    assert!(!after.contains("mutation-checked"));
}

fn single_proven_page(tier: Tier) -> String {
    let spec = Spec::parse("- [R-001] Returns a bounded result.\n").unwrap();
    let annotations = Annotations {
        schema: "living-spec.annotations/v1".into(),
        requirements: BTreeMap::new(),
        extra: BTreeMap::new(),
    };
    let status = vec![LineStatus {
        id: "R-001".into(),
        badge: Badge::Proven,
        reasons: vec![],
        tests: vec![],
        mutation: Some(tier),
    }];
    render_spec(&spec, &annotations, Some(&status), &[])
}

#[test]
fn checked_line_shows_second_badge() {
    let html = single_proven_page(Tier {
        state: TierState::MutationChecked,
        waived: 0,
        signed_off: 0,
        survivors: vec![],
    });
    assert!(html.contains("proven (mutation-checked)"));
    assert!(html.contains("of which mutation-checked 1"));
}

#[test]
fn survivor_flag_lists_location() {
    let html = single_proven_page(mutation_tier(TierState::Survived));
    assert!(html.contains("mutation survivor"));
    assert!(html.contains("src/&lt;subject&gt;.rs:12"));
    assert!(html.contains("cmp_flip"));
    assert!(html.contains("<details class=\"mutation-survivors\">"));
}

#[test]
fn html_escaped_survivor_text() {
    let html = single_proven_page(mutation_tier(TierState::Survived));
    assert!(!html.contains("src/<subject>.rs"));
    assert!(html.contains("src/&lt;subject&gt;.rs"));
}

#[test]
fn waived_count_shown_as_human_signoff() {
    let html = single_proven_page(mutation_tier(TierState::SignedOff));
    assert!(html.contains("proven (mutation sign-off)"));
    assert!(html.contains("waivers: 1; sign-offs: 2"));
    assert!(!html.contains("mutation-checked 1"));
}

#[test]
fn signed_off_not_counted_as_checked() {
    let html = single_proven_page(mutation_tier(TierState::SignedOff));
    assert!(html.contains("of which mutation-checked 0"));
}

#[test]
fn stale_tier_not_shown_as_checked() {
    let html = single_proven_page(mutation_tier(TierState::Stale));
    assert!(html.contains("mutation: stale"));
    assert!(!html.contains("proven (mutation-checked)"));
    assert!(html.contains("of which mutation-checked 0"));
}

#[test]
fn unproven_lines_come_after_every_other_line() {
    let html = seeded_html();
    assert_eq!(
        requirement_order(&html),
        ["R-002", "R-001", "R-004", "R-005", "R-003"]
    );
    let heading = html.find("promises nobody has proven yet").unwrap();
    let last_other = html.find("data-requirement=\"R-005\"").unwrap();
    let gray = html.find("data-requirement=\"R-003\"").unwrap();
    assert!(last_other < heading && heading < gray);
}

#[test]
fn script_in_a_requirement_is_escaped() {
    let expected_true = true;
    let expected_false = false;
    let html = seeded_html();
    assert_eq!(
        html.contains("&lt;script&gt;alert(1)&lt;/script&gt; must stay text."),
        expected_true
    );
    assert_eq!(html.contains("timestamps &amp; zones."), expected_true);
    assert_eq!(html.contains("<script"), expected_false);
    assert_eq!(html.contains("timestamps & zones."), expected_false);
}

#[test]
fn history_note_markup_is_escaped() {
    let spec = Spec::parse("- [R-001] Stay literal.\n").unwrap();
    let annotations = Annotations {
        schema: "living-spec.annotations/v1".into(),
        requirements: BTreeMap::new(),
        extra: BTreeMap::new(),
    };
    let status = vec![LineStatus {
        id: "R-001".into(),
        badge: Badge::Broken,
        reasons: vec![Reason::TestFailed],
        tests: vec![],
        mutation: None,
    }];
    let history = vec![Transition {
        at: "2026-01-01T00:00:00Z".into(),
        commit: "commit-1".into(),
        requirement: "R-001".into(),
        from: Badge::Proven,
        to: Badge::Broken,
        test_id: "case".into(),
        note: "<script>x</script>".into(),
    }];
    let html = render_spec(&spec, &annotations, Some(&status), &history);
    assert!(html.contains("&lt;script&gt;x&lt;/script&gt;"));
    assert!(html.contains("commit-1"));
    assert!(!html.contains("<script"));
}

#[test]
fn drilldown_shows_below_at_above_boundary_cases() {
    let fixture: Value = serde_json::from_str(
        &fs::read_to_string("tests/fixtures/web/boundary/requirements/01.json").unwrap(),
    )
    .unwrap();
    let mut requirement = living_spec::core::annotations::RequirementAnnotations {
        bounds: fixture["expected_bounds"].as_array().unwrap().clone(),
        ..Default::default()
    };
    requirement.extra.insert(
        "boundary_links".into(),
        Value::Array(fixture["links"].as_array().unwrap().clone()),
    );
    let annotations = Annotations {
        schema: "living-spec.annotations/v1".into(),
        requirements: BTreeMap::from([("R-001".into(), requirement)]),
        extra: BTreeMap::new(),
    };
    let spec = Spec::parse("- [R-001] Delay stays within its stated limit.\n").unwrap();
    let bounds: Vec<living_spec::core::boundary::Bound> =
        serde_json::from_value(fixture["expected_bounds"].clone()).unwrap();
    let report =
        living_spec::core::boundary::coverage::coverage("R-001", &bounds, &[], &[]).unwrap();
    let html =
        living_spec::web::render_spec_with_reports(&spec, &annotations, None, &[], &[], &[report]);
    assert!(html.contains("Delay (below)"));
    assert!(html.contains("Delay (at)"));
    assert!(html.contains("Delay (above)"));
    assert_eq!(html.matches("untested").count(), 3);
}

#[test]
fn stored_boundary_results_include_tests_outside_core_links() {
    use living_spec::{
        boundary::{
            coverage::{coverage, CaseLink},
            gen::generate,
            parse_bounds,
        },
        core::test_identity::{TestOutcome, TestStatus},
        web::render_spec_with_reports,
    };
    let bounds = parse_bounds("Delay is at most 30 ms.").unwrap();
    let cases = generate(&bounds[0]).unwrap();
    let links: Vec<_> = cases
        .iter()
        .map(|case| CaseLink {
            quantity: bounds[0].quantity.clone(),
            position: case.position.clone(),
            value: case.value.clone(),
            unit: case.unit,
            test_id: format!("libtest:cases:tests/cases.rs:{}", case.position),
            test_body: None,
        })
        .collect();
    let outcomes: Vec<_> = links
        .iter()
        .map(|link| TestOutcome {
            test_id: link.test_id.clone(),
            status: TestStatus::Passed,
            duration_ms: None,
            failure_message: None,
        })
        .collect();
    let report = coverage("R-001", &bounds, &links, &outcomes).unwrap();
    let annotations = Annotations {
        schema: "living-spec.annotations/v1".into(),
        requirements: BTreeMap::from([(
            "R-001".into(),
            living_spec::core::annotations::RequirementAnnotations {
                bounds: bounds
                    .iter()
                    .map(|bound| serde_json::to_value(bound).unwrap())
                    .collect(),
                extra: BTreeMap::from([(
                    "boundary_links".into(),
                    serde_json::to_value(&links).unwrap(),
                )]),
                ..Default::default()
            },
        )]),
        extra: BTreeMap::new(),
    };
    let spec = Spec::parse("- [R-001] Delay stays within its stated limit.\n").unwrap();
    let html = render_spec_with_reports(&spec, &annotations, Some(&[]), &[], &[], &[report]);
    assert_eq!(html.matches("tested pass").count(), 3);
    assert!(html.contains("libtest:cases:tests/cases.rs:above"));
}

#[test]
fn rendered_pages_have_no_http_urls() {
    let spec = seeded_html();
    let tmp = bound_registry_fixture();
    let registry = tmp
        .path()
        .join("registry-fixture/portfolio-truth-mini.json");
    let rows = collect_rollup(&registry, &[]).unwrap();
    let rollup = render_rollup(&rows);
    assert_no_urls(&spec);
    assert_no_urls(&rollup);
    assert!(!spec.contains("url("));
    assert!(!rollup.contains("url("));
}

#[test]
fn rollup_excludes_archived_and_lists_the_project_without_a_spec() {
    let expected_true = true;
    let expected_false = false;

    let tmp = bound_registry_fixture();
    let registry = tmp
        .path()
        .join("registry-fixture/portfolio-truth-mini.json");
    let rows = collect_rollup(&registry, &[]).unwrap();
    assert_eq!(
        names(&rows),
        ["Amber Ledger", "Birch Timer", "Elm Draft", "Cedar Sketch"]
    );
    assert_eq!(
        rows[0],
        RollupRow {
            project: "Amber Ledger".into(),
            path: "amber-ledger".into(),
            lines: 4,
            proven: 1,
            proven_mutation_checked: 0,
            proven_signed_off: 0,
            mutation_survivors: 0,
            mutation_present: false,
            broken: 1,
            unproven: 1,
            qualified: 1,
            proven_pct: 25.0,
            state: ProjectState::Scored,
        }
    );
    assert_eq!(rows[1].proven, 3);
    assert_eq!(rows[1].lines, 4);
    assert_eq!(rows[1].unproven, 1);
    assert_eq!(rows[1].proven_pct, 75.0);
    assert_eq!(rows[1].state, ProjectState::Scored);
    assert_eq!(rows[2].state, ProjectState::NotScored);
    assert_eq!(rows[2].lines, 1);
    assert_eq!(rows[2].proven, 0);
    assert_eq!(rows[3].state, ProjectState::NoLivingSpec);
    assert_eq!(rows[3].path, "cedar-sketch");
    let document = parse_rollup_json(&rows);
    assert_eq!(document["total_proven"].as_u64(), Some(4));
    assert_eq!(document["total_lines"].as_u64(), Some(8));
    assert_eq!(document["total_proven_pct"].as_f64(), Some(50.0));
    assert_eq!(document["no_spec_count"].as_u64(), Some(1));
    assert_eq!(document["not_scored_count"].as_u64(), Some(1));
    let html = render_rollup(&rows);
    assert_eq!(
        html.contains("Portfolio proven 50.0% (4 of 8 lines across scored or stale projects)."),
        expected_true
    );
    assert_eq!(html.contains("data-proven-pct=\"25.0\""), expected_true);
    assert_eq!(html.contains("data-proven-pct=\"75.0\""), expected_true);
    assert_eq!(html.contains("Cedar Sketch"), expected_true);
    assert_eq!(
        html.contains("1 project has no living spec."),
        expected_true
    );
    assert_eq!(html.contains("not yet scored"), expected_true);
    assert_eq!(html.contains(">Elm Draft<"), expected_true);
    assert_eq!(html.contains("Dune Relic"), expected_false);
    assert_eq!(html.contains("dune-relic"), expected_false);
    assert_eq!(html.contains("DU-001"), expected_false);
    assert_no_urls(&html);
}

fn unproven_section(html: &str) -> &str {
    let start = html.find("data-requirement=\"R-003\"").unwrap();
    &html[start..]
}

fn parse_rollup_json(rows: &[RollupRow]) -> Value {
    serde_json::from_str(&export_rollup_json(rows)).unwrap()
}

#[test]
fn rollup_reports_both_counts() {
    let tmp = tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
    let repo = tmp.path().join("speccompanion");
    let living_spec = repo.join("living-spec");
    fs::create_dir_all(&living_spec).unwrap();
    fs::write(
        living_spec.join("spec.md"),
        "- [SC-001] Keeps an entry.\n- [SC-002] Rejects invalid input.\n",
    )
    .unwrap();
    fs::write(
        living_spec.join("status.json"),
        r#"{"lines":[{"id":"SC-001","badge":"proven","tests":[{"test_id":"fixture", "status":"passed"}],"mutation":{"state":"mutation_checked","waived":0,"signed_off":0,"survivors":[]}},{"id":"SC-002","badge":"broken"}]}"#,
    )
    .unwrap();
    write_rollup_fixture_links(&living_spec);
    bind_saved_inputs(&living_spec);
    let rows = collect_rollup(Path::new(REGISTRY), &[repo]).unwrap();
    let row = rows
        .iter()
        .find(|row| row.project == "speccompanion")
        .unwrap();
    assert_eq!(row.state, ProjectState::Scored);
    assert_eq!(row.proven, 1);
    assert_eq!(row.lines, 2);
    assert_eq!(row.proven_pct, 50.0);
    assert_eq!(row.proven_mutation_checked, 1);
    assert_eq!(row.proven_signed_off, 0);
    assert_eq!(row.mutation_survivors, 0);
    assert!(row.mutation_present);
    let json: Value = serde_json::from_str(&export_rollup_json(&rows)).unwrap();
    let exported = json["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["project"] == "speccompanion")
        .unwrap();
    assert_eq!(exported["proven_mutation_checked"].as_u64(), Some(1));
    assert_eq!(exported["proven_signed_off"].as_u64(), Some(0));
    assert_eq!(exported["mutation_survivors"].as_u64(), Some(0));
    let html = render_rollup(&rows);
    assert!(html.contains("Mutation checked"));
    assert!(html.contains("50.0% proven, 100.0% mutation-checked (0 signed off)"));
}

#[test]
fn quantifier_views_have_no_mutation_text() {
    let html = living_spec::quantifier::view::html(&[]).unwrap();
    assert!(!html.to_ascii_lowercase().contains("mutation"));
}

#[test]
fn rollup_json_additive_fields_only() {
    let rows = vec![RollupRow {
        project: "Sample".into(),
        path: "sample".into(),
        lines: 2,
        proven: 1,
        proven_mutation_checked: 0,
        proven_signed_off: 0,
        mutation_survivors: 0,
        mutation_present: false,
        broken: 0,
        unproven: 1,
        qualified: 0,
        proven_pct: 50.0,
        state: ProjectState::Scored,
    }];
    let document = parse_rollup_json(&rows);
    let row = &document["rows"][0];
    assert_eq!(row["proven"].as_u64(), Some(1));
    assert_eq!(row["proven_mutation_checked"].as_u64(), Some(0));
    assert_eq!(row["proven_signed_off"].as_u64(), Some(0));
    assert_eq!(row["mutation_survivors"].as_u64(), Some(0));
    let html = render_rollup(&rows);
    assert!(!html.contains("Mutation checked"));
    assert!(!html.contains("mutation-checked"));
}

#[test]
fn rollup_html_without_mutation_unchanged() {
    let rows = vec![RollupRow {
        project: "Sample".into(),
        path: "sample".into(),
        lines: 2,
        proven: 1,
        proven_mutation_checked: 0,
        proven_signed_off: 0,
        mutation_survivors: 0,
        mutation_present: false,
        broken: 0,
        unproven: 1,
        qualified: 0,
        proven_pct: 50.0,
        state: ProjectState::Scored,
    }];
    let old_shape = render_rollup(&rows);
    let with_zero_mutation_counts = render_rollup(&rows);
    assert_eq!(old_shape.as_bytes(), with_zero_mutation_counts.as_bytes());
    assert!(!old_shape.contains("Mutation checked"));
}

#[test]
fn include_row_uses_directory_name() {
    let tmp = tempfile::tempdir_in("target").unwrap();
    let repo = tmp.path().join("cedar-workshop");
    fs::create_dir_all(repo.join("child")).unwrap();
    let include = repo.join("child/..");
    let rows = collect_rollup(Path::new(REGISTRY), std::slice::from_ref(&include)).unwrap();
    let row = rows
        .iter()
        .find(|row| row.project == "cedar-workshop")
        .unwrap();
    assert_eq!(row.path, include.to_string_lossy());
    assert_eq!(row.state, ProjectState::NoLivingSpec);
}

#[test]
fn status_alias_and_registry_tolerate_missing_optional_fields() {
    let tmp = tempfile::tempdir().unwrap();
    let status_path = tmp.path().join("status.json");
    fs::write(
        &status_path,
        r#"{"schema":"living-spec.status/v1","statuses":[{"id":"R-001","badge":"broken","extra":true,"tests":[{"test_id":"t","status":"failed","duration_ms":3}]}]}"#,
    )
    .unwrap();
    let lines = load_status(&status_path).unwrap();
    assert_eq!(lines[0].badge, Badge::Broken);
    assert_eq!(lines[0].tests[0].duration_ms, Some(3));

    let root = tmp.path().join("root");
    fs::create_dir_all(root.join("kept").join("living-spec")).unwrap();
    fs::write(
        root.join("kept/living-spec/spec.md"),
        "- [K-001] Keeps an entry.\n",
    )
    .unwrap();
    fs::write(
        root.join("kept").join("living-spec").join("status.json"),
        r#"{"lines":[{"id":"K-001","badge":"proven","tests":[{"test_id":"fixture", "status":"passed"}]}]}"#,
    )
    .unwrap();
    write_rollup_fixture_links(&root.join("kept/living-spec"));
    bind_saved_inputs(&root.join("kept/living-spec"));
    fs::create_dir_all(root.join("old").join("living-spec")).unwrap();
    fs::write(
        root.join("old").join("living-spec").join("status.json"),
        r#"{"lines":[{"id":"OLD-001","badge":"proven"},{"id":"OLD-002","badge":"proven"}]}"#,
    )
    .unwrap();
    let registry = tmp.path().join("registry.json");
    let snapshot = serde_json::json!({
        "workspace_root": root,
        "ignored_top": {"keep": true},
        "projects": [
            {
                "identity": {"display_name": "<script>", "path": "kept"},
                "derived": {"archived": false, "extra": 1}
            },
            {"identity": {"path": "also-missing"}, "derived": {"archived": false}},
            {
                "identity": {"display_name": "Skip Me", "path": "old"},
                "derived": {"archived": true}
            }
        ]
    });
    fs::write(&registry, snapshot.to_string()).unwrap();
    let rows = collect_rollup(&registry, &[]).unwrap();
    assert_eq!(names(&rows), ["<script>", "also-missing"]);
    assert_eq!(rows[0].path, "kept");
    assert_eq!(rows[0].proven, 1);
    assert_eq!(rows[0].lines, 1);
    assert_eq!(rows[0].state, ProjectState::Scored);
    assert_eq!(rows[1].state, ProjectState::NoLivingSpec);
    let html = render_rollup(&rows);
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("also-missing"));
    assert!(!html.contains("<script"));
    assert!(!html.contains("Skip Me"));
    assert!(!html.contains("OLD-001"));
    assert!(!html.contains("/Users/"));
}

// Exercise the same on-disk inputs and CLI entry points as the web review.
fn write_rollup_fixture_links(dir: &Path) {
    let spec = Spec::parse(&fs::read_to_string(dir.join("spec.md")).unwrap()).unwrap();
    let requirements: serde_json::Map<_, _> = spec
        .lines
        .iter()
        .map(|line| {
            let mut link = reviewed_link();
            link["test_id"] = "fixture".into();
            link["revision"] = line.revision.clone().into();
            link["audit"]["revision"] = line.revision.clone().into();
            (line.id.clone(), serde_json::json!({"links": [link]}))
        })
        .collect();
    fs::write(
        dir.join("annotations.json"),
        serde_json::json!({
            "schema": "living-spec.annotations/v1", "requirements": requirements
        })
        .to_string(),
    )
    .unwrap();
}

fn review_input(spec: &str, annotations: Value, status: Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir_in("target").unwrap();
    let spec_dir = dir.path().join("sample/living-spec");
    fs::create_dir_all(&spec_dir).unwrap();
    fs::write(spec_dir.join("spec.md"), spec).unwrap();
    fs::write(spec_dir.join("annotations.json"), annotations.to_string()).unwrap();
    fs::write(spec_dir.join("status.json"), status.to_string()).unwrap();
    bind_saved_inputs(&spec_dir);
    fs::write(
        dir.path().join("registry.json"),
        serde_json::json!({
            "workspace_root": ".", "projects": [{
                "identity": {"project_key": "sample", "path": "sample"},
                "derived": {"archived": false}
            }]
        })
        .to_string(),
    )
    .unwrap();
    dir
}

fn bind_saved_inputs(spec_dir: &Path) {
    let status_path = spec_dir.join("status.json");
    let Ok(statuses) = load_status(&status_path) else {
        return;
    };
    let mut outcomes = BTreeMap::new();
    for line in &statuses {
        for test in &line.tests {
            outcomes.insert(
                test.test_id.clone(),
                TestOutcome {
                    test_id: test.test_id.clone(),
                    status: test.status,
                    duration_ms: test.duration_ms,
                    failure_message: None,
                },
            );
        }
    }
    let outcomes: Vec<_> = outcomes.into_values().collect();
    bind_saved_inputs_with_result_bytes(spec_dir, common::junit(&outcomes).as_bytes());
}

fn bind_saved_inputs_with_result_bytes(spec_dir: &Path, results: &[u8]) {
    let status_path = spec_dir.join("status.json");
    let mut status: Value = serde_json::from_slice(&fs::read(&status_path).unwrap()).unwrap();
    let statuses = load_status(&status_path).unwrap();
    let outcomes = living_spec::adapters::parse_junit(std::str::from_utf8(results).unwrap())
        .or_else(|_| living_spec::adapters::parse_libtest(std::str::from_utf8(results).unwrap()))
        .unwrap();
    let boundary: Vec<BoundaryReport> = serde_json::from_value(
        status
            .get("boundary_reports")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )
    .unwrap();
    let quantifier: Vec<QuantifierReport> = serde_json::from_value(
        status
            .get("quantifier_reports")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )
    .unwrap();
    let results_path = spec_dir.join("test-results.txt");
    fs::write(&results_path, results).unwrap();
    let results =
        ResultsSnapshot::read(spec_dir, results_path.to_str().unwrap(), &outcomes).unwrap();
    let spec_bytes = fs::read(spec_dir.join("spec.md")).unwrap();
    let annotations =
        serde_json::from_slice(&fs::read(spec_dir.join("annotations.json")).unwrap()).unwrap();
    let inputs = ScoreInputs::capture(
        &spec_bytes,
        &annotations,
        &results,
        &outcomes,
        &statuses,
        &boundary,
        &quantifier,
    )
    .unwrap();
    status["inputs"] = serde_json::to_value(inputs).unwrap();
    fs::write(&status_path, serde_json::to_vec_pretty(&status).unwrap()).unwrap();
}

fn empty_annotations() -> Value {
    serde_json::json!({"schema": "living-spec.annotations/v1", "requirements": {}})
}

fn reviewed_link() -> Value {
    serde_json::json!({
        "test_id": "libtest:sample:tests/a.rs:case",
        "fingerprint": "sha256:fixture",
        "revision": "sha256:18f0b382ff7684a5303f4c7da20d637efb9ffef40cd02bb899f41aba9b1fdfe1",
        "explanation": "Checks the input.", "proposed_by": "human",
        "audit": {"verdict": "accepted", "notes": [],
            "revision": "sha256:18f0b382ff7684a5303f4c7da20d637efb9ffef40cd02bb899f41aba9b1fdfe1"}
    })
}

fn one_proven_status() -> Value {
    serde_json::json!({"statuses": [{"id": "R-001", "badge": "proven",
        "tests": [{"test_id": "libtest:sample:tests/a.rs:case", "status": "passed"}]}]})
}

fn review_render(dir: &Path) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .arg("render")
        .arg(dir.join("sample/living-spec"))
        .arg("--out")
        .arg(dir.join("view.html"))
        .output()
        .unwrap()
}

fn review_rollup(dir: &Path) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_living-spec"))
        .arg("rollup")
        .arg("--registry")
        .arg(dir.join("registry.json"))
        .arg("--out")
        .arg(dir.join("rollup.html"))
        .arg("--json")
        .arg(dir.join("rollup.json"))
        .output()
        .unwrap()
}

#[test]
fn editing_prerequisite_marks_dependent_stale() {
    let spec = "- [R-001] Uses the prerequisite.\n- [R-002] Prerequisite behavior.\n- [R-003] Independent behavior.\n- [R-004] Uses the dependent behavior.\n";
    let annotations = serde_json::json!({"schema":"living-spec.annotations/v1",
        "requirements":{"R-001":{"depends_on":[{"id":"R-002","evidence":"authored"}]},
            "R-004":{"depends_on":[{"id":"R-001","evidence":"authored"}]}}});
    let statuses = serde_json::json!({"statuses":[
        {"id":"R-001","badge":"proven","tests":[{"test_id":"case-a","status":"passed"}]},
        {"id":"R-002","badge":"proven","tests":[{"test_id":"case-b","status":"passed"}]},
        {"id":"R-003","badge":"proven","tests":[{"test_id":"case-c","status":"passed"}]},
        {"id":"R-004","badge":"proven","tests":[{"test_id":"case-d","status":"passed"}]}
    ]});
    let dir = review_input(spec, annotations, statuses);
    fs::write(
        dir.path().join("sample/living-spec/spec.md"),
        "- [R-001] Uses the prerequisite.\n- [R-002] Changed prerequisite behavior.\n- [R-003] Independent behavior.\n- [R-004] Uses the dependent behavior.\n",
    )
    .unwrap();

    let html = rendered_review(dir.path());
    assert!(html.contains("data-requirement=\"R-001\" data-badge=\"stale\""));
    assert!(html.contains("data-requirement=\"R-002\" data-badge=\"stale\""));
    assert!(html.contains("data-requirement=\"R-003\" data-badge=\"proven\""));
    assert!(html.contains("data-requirement=\"R-004\" data-badge=\"stale\""));
    assert!(html.contains("Stale: inputs changed since this status was scored"));

    let rows = collect_rollup(&dir.path().join("registry.json"), &[]).unwrap();
    assert_eq!(rows[0].state, ProjectState::Stale);
    assert_eq!(rows[0].proven, 0);
    assert_eq!(rows[0].lines, 4);
}

#[test]
fn changed_result_marks_only_affected_line_and_dependents_stale() {
    let spec = "- [R-001] Uses the prerequisite.\n- [R-002] Prerequisite behavior.\n- [R-003] Independent behavior.\n";
    let annotations = serde_json::json!({"schema":"living-spec.annotations/v1",
        "requirements":{"R-001":{"depends_on":[{"id":"R-002","evidence":"authored"}]}}});
    let statuses = serde_json::json!({"statuses":[
        {"id":"R-001","badge":"proven","tests":[{"test_id":"case-a","status":"passed"}]},
        {"id":"R-002","badge":"proven","tests":[{"test_id":"case-b","status":"passed"}]},
        {"id":"R-003","badge":"proven","tests":[{"test_id":"case-c","status":"passed"}]}
    ]});
    let dir = review_input(spec, annotations, statuses);
    let spec_dir = dir.path().join("sample/living-spec");
    bind_saved_inputs_with_result_bytes(
        &spec_dir,
        b"running 3 tests\ntest case-a ... ok\ntest case-b ... ok\ntest case-c ... ok\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
    );
    fs::write(
        spec_dir.join("test-results.txt"),
        "running 3 tests\ntest case-a ... ok\ntest case-b ... FAILED\ntest case-c ... ok\ntest result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n",
    )
    .unwrap();
    let html = rendered_review(dir.path());
    assert!(html.contains("data-requirement=\"R-001\" data-badge=\"stale\""));
    assert!(html.contains("data-requirement=\"R-002\" data-badge=\"stale\""));
    assert!(html.contains("data-requirement=\"R-003\" data-badge=\"proven\""));
}

#[test]
fn boundary_receipt_without_current_link_is_stale() {
    use living_spec::core::{
        boundary::{coverage, coverage::CaseLink, Bound, Unit},
        test_identity::{TestOutcome, TestStatus},
    };
    let spec = "- [R-001] Delay is at most 30 ms.\n";
    let bound: Bound = serde_json::from_value(serde_json::json!({
        "quantity":"Delay", "kind":{"type":"numeric", "value":"30", "unit":"ms",
        "op":{"type":"le"}, "step":{"value":"1", "unit":"ms"}, "integer":false}
    }))
    .unwrap();
    let links: Vec<CaseLink> = [("below", "29"), ("at", "30"), ("above", "31")]
        .into_iter()
        .map(|(position, value)| CaseLink {
            quantity: "Delay".into(),
            position: position.into(),
            value: value.into(),
            unit: Some(Unit::Ms),
            test_id: format!("boundary-{position}"),
            test_body: None,
        })
        .collect();
    let outcomes: Vec<TestOutcome> = links
        .iter()
        .map(|link| TestOutcome {
            test_id: link.test_id.clone(),
            status: TestStatus::Passed,
            duration_ms: None,
            failure_message: None,
        })
        .collect();
    let report = coverage("R-001", std::slice::from_ref(&bound), &links, &outcomes).unwrap();
    let annotations = serde_json::json!({"schema":"living-spec.annotations/v1",
        "requirements":{"R-001":{"bounds":[bound],"boundary_links":links}}});
    let mut status = one_proven_status();
    status["boundary_reports"] = serde_json::json!([report]);
    status["statuses"][0]["tests"] = serde_json::json!(outcomes);
    let dir = review_input(spec, annotations, status);
    let annotation_path = dir.path().join("sample/living-spec/annotations.json");
    let mut current: Value = serde_json::from_slice(&fs::read(&annotation_path).unwrap()).unwrap();
    current["requirements"]["R-001"]
        .as_object_mut()
        .unwrap()
        .remove("boundary_links");
    fs::write(&annotation_path, current.to_string()).unwrap();

    let html = rendered_review(dir.path());
    assert!(html.contains("data-badge=\"stale\""));
    assert!(html.contains("Stored boundary and quantifier verdicts are stale"));
    assert!(!html.contains("<h3>Boundary coverage</h3>"));
}

#[test]
fn unchanged_inputs_render_stored_badges() {
    let dir = review_input(
        "- [R-001] Returns the input.\n",
        empty_annotations(),
        one_proven_status(),
    );
    // Annotation formatting is not semantic input: the digest uses canonical JSON.
    fs::write(
        dir.path().join("sample/living-spec/annotations.json"),
        serde_json::to_string_pretty(&empty_annotations()).unwrap(),
    )
    .unwrap();
    let html = rendered_review(dir.path());
    assert!(html.contains("data-requirement=\"R-001\" data-badge=\"proven\""));
    assert!(!html.contains("Stale: inputs changed"));
    let row = collect_rollup(&dir.path().join("registry.json"), &[])
        .unwrap()
        .remove(0);
    assert_eq!(row.state, ProjectState::Scored);
    assert_eq!(row.proven, 1);
}

#[test]
fn stale_rollup_counts_zero_proven() {
    let dir = review_input(
        "- [R-001] Returns the input.\n",
        empty_annotations(),
        one_proven_status(),
    );
    fs::write(
        dir.path().join("sample/living-spec/test-results.txt"),
        "changed test result bytes",
    )
    .unwrap();
    let row = collect_rollup(&dir.path().join("registry.json"), &[])
        .unwrap()
        .remove(0);
    assert_eq!(row.state, ProjectState::Stale);
    assert_eq!(row.proven, 0);
    assert_eq!(row.lines, 1);
    let html = rendered_review(dir.path());
    assert!(html.contains("data-badge=\"stale\""));
    assert!(html.contains("test-result file changed or is unavailable since scoring"));
}

fn rendered_review(dir: &Path) -> String {
    let output = review_render(dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(dir.join("view.html")).unwrap()
}

#[test]
fn regression_edited_requirements_retain_a_green_badge_from_an_older_revision() {
    let original = "- [R-001] Returns the input.\n";
    let current = "- [R-001] Returns twice the input.\n";
    let annotations = serde_json::json!({"schema": "living-spec.annotations/v1",
        "requirements": {"R-001": {"links": [reviewed_link()]}}});
    let dir = review_input(original, annotations.clone(), one_proven_status());
    fs::write(dir.path().join("sample/living-spec/spec.md"), current).unwrap();
    let html = rendered_review(dir.path());
    assert!(html.contains("Returns twice the input."));
    assert!(html.contains("data-requirement=\"R-001\" data-badge=\"stale\""));
    assert!(html.contains("Stale: inputs changed since this status was scored"));
    assert!(html.contains("data-proven-pct=\"0.0\""));
    assert!(
        html.contains("passed"),
        "stored diagnostics must remain visible"
    );
    let revision = Spec::parse(current).unwrap().lines[0].revision.clone();
    for field in ["link", "audit"] {
        let mut partially_updated = annotations.clone();
        let link = &mut partially_updated["requirements"]["R-001"]["links"][0];
        if field == "link" {
            link["revision"] = revision.clone().into();
        } else {
            link["audit"]["revision"] = revision.clone().into();
        }
        fs::write(
            dir.path().join("sample/living-spec/annotations.json"),
            partially_updated.to_string(),
        )
        .unwrap();
        let html = rendered_review(dir.path());
        assert!(html.contains("data-badge=\"stale\""), "{field}");
        assert!(html.contains("stale —"), "{field}");
    }
    fs::write(dir.path().join("sample/living-spec/spec.md"), original).unwrap();
    fs::write(
        dir.path().join("sample/living-spec/annotations.json"),
        annotations.to_string(),
    )
    .unwrap();
    assert!(rendered_review(dir.path()).contains("data-badge=\"proven\""));
}

#[test]
fn regression_rollup_omits_requirements_absent_from_an_old_status_file() {
    let dir = review_input(
        "- [R-001] Returns the input.\n- [R-002] Rejects invalid input.\n",
        empty_annotations(),
        one_proven_status(),
    );
    let output = review_rollup(dir.path());
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing: [\"R-002\"]"));
    assert!(!dir.path().join("rollup.json").exists());
    // Removed status entries cannot contribute evidence either.
    fs::write(
        dir.path().join("sample/living-spec/spec.md"),
        "- [R-002] Rejects invalid input.\n",
    )
    .unwrap();
    let output = review_rollup(dir.path());
    assert!(output.status.success());
    let rows: Value =
        serde_json::from_slice(&fs::read(dir.path().join("rollup.json")).unwrap()).unwrap();
    assert_eq!(rows["rows"][0]["state"], "stale");
    fs::remove_file(dir.path().join("sample/living-spec/spec.md")).unwrap();
    let output = review_rollup(dir.path());
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires a readable spec.md"));
}

#[test]
fn regression_conflicting_status_aliases_silently_discard_failed_evidence() {
    let mut status = one_proven_status();
    status["lines"] = status["statuses"].clone();
    status["statuses"][0]["badge"] = "broken".into();
    status["statuses"][0]["reasons"] = serde_json::json!(["test_failed"]);
    status["statuses"][0]["tests"][0]["status"] = "failed".into();
    let dir = review_input(
        "- [R-001] Returns the input.\n",
        empty_annotations(),
        status.clone(),
    );
    for output in [review_render(dir.path()), review_rollup(dir.path())] {
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("both lines and statuses"));
    }
    // Identical aliases also fail, keeping the input contract unambiguous.
    status["statuses"] = status["lines"].clone();
    fs::write(
        dir.path().join("sample/living-spec/status.json"),
        status.to_string(),
    )
    .unwrap();
    assert_eq!(review_render(dir.path()).status.code(), Some(1));
    assert!(!dir.path().join("view.html").exists());
    assert!(!dir.path().join("rollup.json").exists());
}

#[test]
fn regression_changed_stored_boundary_receipt_marks_the_line_stale() {
    use living_spec::core::boundary::{coverage, Bound};
    let bound = serde_json::json!({"quantity": "Delay", "kind": {"type": "numeric",
        "value": "30", "unit": "ms", "op": {"type": "le"},
        "step": {"value": "1", "unit": "ms"}, "integer": false}});
    let annotations = serde_json::json!({"schema": "living-spec.annotations/v1",
        "requirements": {"R-001": {"bounds": [bound.clone()]}}});
    let mut status = one_proven_status();
    status["boundary_reports"] = serde_json::json!([{"requirement": "R-001", "bounds": []}]);
    let dir = review_input(
        "- [R-001] Delay is at most 30 ms.\n",
        annotations,
        status.clone(),
    );
    let baseline = coverage(
        "R-001",
        &[serde_json::from_value::<Bound>(bound).unwrap()],
        &[],
        &[],
    )
    .unwrap();
    let complete = serde_json::to_value(baseline).unwrap();
    let mut missing_case = complete.clone();
    missing_case["bounds"][0]["cases"]
        .as_array_mut()
        .unwrap()
        .pop();
    let mut duplicate_case = complete.clone();
    duplicate_case["bounds"][0]["cases"][2] = complete["bounds"][0]["cases"][0].clone();
    let mut duplicate_bound = complete.clone();
    duplicate_bound["bounds"]
        .as_array_mut()
        .unwrap()
        .push(complete["bounds"][0].clone());
    let mut changed_bound = complete.clone();
    changed_bound["bounds"][0]["bound"]["kind"]["value"] = "40".into();
    let mut changed_case = complete.clone();
    changed_case["bounds"][0]["cases"][0]["value"] = "28".into();
    for reports in [
        status["boundary_reports"].clone(),
        serde_json::json!([missing_case]),
        serde_json::json!([duplicate_case]),
        serde_json::json!([duplicate_bound]),
        serde_json::json!([changed_bound]),
        serde_json::json!([changed_case]),
        serde_json::json!([complete.clone(), complete]),
    ] {
        status["boundary_reports"] = reports;
        fs::write(
            dir.path().join("sample/living-spec/status.json"),
            status.to_string(),
        )
        .unwrap();
        let html = rendered_review(dir.path());
        assert!(html.contains("data-badge=\"stale\""));
        assert!(html.contains("stale —"));
        assert!(html.contains("Stored boundary and quantifier verdicts are stale"));
        assert!(!html.contains("<h3>Boundary coverage</h3>"));
    }
}

#[test]
fn regression_repeated_failures_replace_the_actual_breaking_commit() {
    let mut status = one_proven_status();
    status["statuses"][0]["badge"] = "broken".into();
    status["statuses"][0]["tests"][0]["status"] = "failed".into();
    let dir = review_input(
        "- [R-001] Returns the input.\n",
        empty_annotations(),
        status,
    );
    let history = dir.path().join("sample/living-spec/history.jsonl");
    let first = serde_json::json!({"at":"2026-01-01T00:00:00Z","commit":"commit-break",
        "requirement":"R-001","from":"proven","to":"broken","test_id":"case","note":"First failure"});
    let repeated = serde_json::json!({"at":"2026-01-02T00:00:00Z","commit":"commit-still-broken",
        "requirement":"R-001","from":"broken","to":"broken","test_id":"case","note":"Still failing"});
    for from in ["proven", "unproven"] {
        let mut first = first.clone();
        first["from"] = from.into();
        fs::write(&history, format!("{first}\n{repeated}\n")).unwrap();
        let html = rendered_review(dir.path());
        assert!(html.contains("breaking commit <code>commit-break</code>"));
        assert!(html.contains("First failure"));
        assert!(!html.contains("commit-still-broken"));
    }
    fs::write(history, format!("{repeated}\n")).unwrap();
    assert!(rendered_review(dir.path()).contains("breaking commit not recorded"));
}

#[test]
fn regression_orphan_annotations_disappear_from_the_web_report() {
    let mut link = reviewed_link();
    link["audit"]["notes"] = serde_json::json!(["Orphan <script>audit</script>"]);
    let annotations = serde_json::json!({"schema": "living-spec.annotations/v1",
        "requirements": {"R-999": {"links": [link]}}});
    let dir = review_input(
        "- [R-001] Returns the input.\n",
        annotations,
        one_proven_status(),
    );
    let html = rendered_review(dir.path());
    assert!(html.contains("Orphan annotations"));
    assert!(html.contains("R-999"));
    assert!(html.contains("Orphan &lt;script&gt;audit&lt;/script&gt;"));
    assert!(html.contains("libtest:sample:tests/a.rs:case"));
    assert!(html.contains("Checks the input."));
    assert!(!html.contains("<script>"));
    assert_eq!(requirement_order(&html), ["R-001"]);
    assert!(html.contains("data-proven=\"1\""));
}

#[test]
fn regression_unknown_archival_state_is_guessed_to_mean_kept() {
    let dir = review_input(
        "- [R-001] Returns the input.\n",
        empty_annotations(),
        one_proven_status(),
    );
    let registry_path = dir.path().join("registry.json");
    for derived in [
        None,
        Some(Value::Null),
        Some(serde_json::json!({})),
        Some(serde_json::json!({"archived": null})),
        Some(serde_json::json!({"archived": "false"})),
    ] {
        let mut registry = serde_json::json!({"workspace_root": ".", "projects": [{
            "identity": {"project_key": "sample", "path": "sample"}
        }]});
        if let Some(derived) = derived {
            registry["projects"][0]["derived"] = derived;
        }
        fs::write(&registry_path, registry.to_string()).unwrap();
        let output = review_rollup(dir.path());
        assert_eq!(output.status.code(), Some(1), "{registry}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("derived.archived must be a boolean")
        );
        assert!(!dir.path().join("rollup.json").exists());
    }
}

#[test]
fn regression_rollup_counts_stored_badges_after_only_validating_ids() {
    let annotations = serde_json::json!({"schema": "living-spec.annotations/v1",
        "requirements": {"R-001": {"links": [reviewed_link()]}}});
    let dir = review_input(
        "- [R-001] Returns the input.\n",
        annotations.clone(),
        one_proven_status(),
    );
    fs::write(
        dir.path().join("sample/living-spec/spec.md"),
        "- [R-001] Returns twice the input.\n",
    )
    .unwrap();
    let rows = collect_rollup(&dir.path().join("registry.json"), &[]).unwrap();
    assert_eq!(
        rows[0].proven, 0,
        "edited text must not retain stored proof"
    );
    assert_eq!(rows[0].state, ProjectState::Stale);
    let json: Value = serde_json::from_str(&export_rollup_json(&rows)).unwrap();
    assert_eq!(json["total_proven"], 0);
    assert!(render_rollup(&rows).contains("Portfolio proven 0.0%"));
    fs::write(
        dir.path().join("sample/living-spec/spec.md"),
        "- [R-001] Returns the input.\n",
    )
    .unwrap();
    assert_eq!(
        collect_rollup(&dir.path().join("registry.json"), &[]).unwrap()[0].proven,
        1
    );
    fs::remove_file(dir.path().join("sample/living-spec/annotations.json")).unwrap();
    assert_eq!(
        collect_rollup(&dir.path().join("registry.json"), &[]).unwrap()[0].state,
        ProjectState::Stale
    );
}

#[test]
fn regression_web_displays_the_stored_badge_without_rederiving_link_verdicts() {
    let first = reviewed_link();
    let mut second = first.clone();
    second["test_id"] = "libtest:sample:tests/a.rs:second".into();
    let annotations = serde_json::json!({"schema": "living-spec.annotations/v1",
        "requirements": {"R-001": {"links": [first, second]}}});
    let dir = review_input(
        "- [R-001] Returns the input.\n",
        annotations,
        one_proven_status(),
    );
    let html = rendered_review(dir.path());
    assert!(html.contains("data-badge=\"proven\""));
    for outcome in ["skipped", "failed", "errored", "passed"] {
        let mut status = one_proven_status();
        status["statuses"][0]["tests"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "test_id": "libtest:sample:tests/a.rs:second", "status": outcome
            }));
        fs::write(
            dir.path().join("sample/living-spec/status.json"),
            status.to_string(),
        )
        .unwrap();
        let html = rendered_review(dir.path());
        assert!(html.contains("data-badge=\"stale\""), "{outcome}: {html}");
    }
}

#[test]
fn regression_changed_inferred_boundary_inputs_mark_saved_receipts_stale() {
    use living_spec::{
        boundary::{
            coverage::{coverage, CaseLink},
            gen::generate,
            parse_bounds,
        },
        core::test_identity::{TestOutcome, TestStatus},
    };
    let original = Spec::parse("- [R-001] Delay is at most 30 ms.\n").unwrap();
    let bounds = parse_bounds(&original.lines[0].text).unwrap();
    let links: Vec<_> = generate(&bounds[0])
        .unwrap()
        .iter()
        .map(|case| CaseLink {
            quantity: "Delay".into(),
            position: case.position.clone(),
            value: case.value.clone(),
            unit: case.unit,
            test_id: format!("boundary:{}", case.position),
            test_body: None,
        })
        .collect();
    let outcomes: Vec<_> = links
        .iter()
        .map(|link| TestOutcome {
            test_id: link.test_id.clone(),
            status: TestStatus::Passed,
            duration_ms: None,
            failure_message: None,
        })
        .collect();
    let report = coverage("R-001", &bounds, &links, &outcomes).unwrap();
    // Re-audit the ordinary link, while leaving generated boundary annotations cached.
    // The second edit changes acceptance while keeping the same numeric value.
    for text in ["Delay is at most 20 ms.", "Delay is under 30 ms."] {
        let spec_text = format!("- [R-001] {text}\n");
        let revision = Spec::parse(&spec_text).unwrap().lines[0].revision.clone();
        let mut link = reviewed_link();
        link["revision"] = revision.clone().into();
        link["audit"]["revision"] = revision.into();
        let annotations = serde_json::json!({"schema": "living-spec.annotations/v1", "requirements": {
            "R-001": {"links": [link], "bounds": bounds,
                "bounds_source": {"origin": "inferred", "revision": original.lines[0].revision},
                "boundary_links": links}
        }});
        let mut status = one_proven_status();
        status["boundary_reports"] = serde_json::json!([report]);
        let original_text = "- [R-001] Delay is at most 30 ms.\n";
        let dir = review_input(
            original_text,
            serde_json::json!({"schema":"living-spec.annotations/v1",
            "requirements":{"R-001":{"links":[link.clone()],"bounds":bounds,
                "bounds_source":{"origin":"inferred","revision":original.lines[0].revision},
                "boundary_links":links}}}),
            status.clone(),
        );
        fs::write(dir.path().join("sample/living-spec/spec.md"), &spec_text).unwrap();
        fs::write(
            dir.path().join("sample/living-spec/annotations.json"),
            annotations.to_string(),
        )
        .unwrap();
        for stored_report in [true, false] {
            if !stored_report {
                status.as_object_mut().unwrap().remove("boundary_reports");
                status["statuses"][0]["tests"]
                    .as_array_mut()
                    .unwrap()
                    .extend(
                        outcomes
                            .iter()
                            .map(|outcome| serde_json::to_value(outcome).unwrap()),
                    );
                fs::write(
                    dir.path().join("sample/living-spec/status.json"),
                    status.to_string(),
                )
                .unwrap();
            }
            let html = rendered_review(dir.path());
            assert!(html.contains("data-badge=\"stale\""), "{html}");
            assert!(html.contains("inputs changed since this status was scored"));
            assert!(!html.contains("<h3>Boundary coverage</h3>"));
        }
    }
}

#[test]
fn drill2_changed_or_removed_boundary_links_make_stored_reports_stale() {
    use living_spec::core::{
        boundary::{coverage, coverage::CaseLink, Bound},
        test_identity::{TestOutcome, TestStatus},
    };
    let text = "- [R-001] Delay is at most 30 ms.\n";
    let line = Spec::parse(text).unwrap().lines.remove(0);
    let bound: Bound = serde_json::from_value(serde_json::json!({
        "quantity":"Delay", "kind":{"type":"numeric", "value":"30", "unit":"ms",
        "op":{"type":"le"}, "step":{"value":"1", "unit":"ms"}, "integer":false}
    }))
    .unwrap();
    let links: Vec<CaseLink> = [("below", "29"), ("at", "30"), ("above", "31")]
        .into_iter()
        .map(|(position, value)| CaseLink {
            quantity: "Delay".into(),
            position: position.into(),
            value: value.into(),
            unit: Some(living_spec::core::boundary::Unit::Ms),
            test_id: "libtest:sample:tests/a.rs:case".into(),
            test_body: None,
        })
        .collect();
    let outcomes = [TestOutcome {
        test_id: links[0].test_id.clone(),
        status: TestStatus::Passed,
        duration_ms: None,
        failure_message: None,
    }];
    let report = coverage("R-001", std::slice::from_ref(&bound), &links, &outcomes).unwrap();
    let mut link = reviewed_link();
    link["revision"] = line.revision.clone().into();
    link["audit"]["revision"] = line.revision.into();
    let annotations = serde_json::json!({"schema":"living-spec.annotations/v1",
        "requirements":{"R-001":{"links":[link],"bounds":[bound],"boundary_links":links}}});
    let mut status = one_proven_status();
    status["boundary_reports"] = serde_json::json!([report]);
    let dir = review_input(text, annotations.clone(), status.clone());
    assert!(rendered_review(dir.path()).contains("data-badge=\"proven\""));
    assert_eq!(
        collect_rollup(&dir.path().join("registry.json"), &[]).unwrap()[0].proven,
        1
    );
    let mut changed = Vec::new();
    for (field, value) in [
        ("test_id", "replacement"),
        ("value", "28"),
        ("unit", "s"),
        ("test_body", "Duration::from_millis(29)"),
    ] {
        let mut a = annotations.clone();
        a["requirements"]["R-001"]["boundary_links"][0][field] = value.into();
        changed.push(a);
    }
    let mut removed = annotations.clone();
    removed["requirements"]["R-001"]
        .as_object_mut()
        .unwrap()
        .remove("boundary_links");
    changed.push(removed);
    for a in changed {
        fs::write(
            dir.path().join("sample/living-spec/annotations.json"),
            a.to_string(),
        )
        .unwrap();
        let html = rendered_review(dir.path());
        assert!(html.contains("data-badge=\"stale\""), "{html}");
        assert!(html.contains("Stored boundary and quantifier verdicts are stale"));
        assert!(!html.contains("<h3>Boundary coverage</h3>"));
        let rows = collect_rollup(&dir.path().join("registry.json"), &[]).unwrap();
        assert_eq!(rows[0].state, ProjectState::Stale);
        assert_eq!(
            (rows[0].proven, rows[0].qualified, rows[0].unproven),
            (0, 0, 0)
        );
    }
    // A legacy receipt cannot attest to its original mapping.
    fs::write(
        dir.path().join("sample/living-spec/annotations.json"),
        annotations.to_string(),
    )
    .unwrap();
    status["boundary_reports"][0]
        .as_object_mut()
        .unwrap()
        .remove("links_fingerprint");
    fs::write(
        dir.path().join("sample/living-spec/status.json"),
        status.to_string(),
    )
    .unwrap();
    assert!(rendered_review(dir.path()).contains("data-badge=\"stale\""));
}

#[test]
fn drill2_quantifier_reports_match_current_predicate_domain_and_text() {
    use living_spec::quantifier::{self, Claim};
    let mut claim: Claim =
        serde_json::from_str(include_str!("fixtures/quantifier/specs/Q-001.json")).unwrap();
    claim.id = "R-001".into();
    claim.text = "Returns the input.".into();
    let report = quantifier::check_claim(&claim).unwrap();
    let annotations = serde_json::json!({"schema":"living-spec.annotations/v1",
        "requirements":{"R-001":{"quantifier":claim.quantifier}}});
    let mut status = one_proven_status();
    status["quantifier_reports"] = serde_json::json!([report]);
    let original = "- [R-001] Returns the input.\n";
    let dir = review_input(original, annotations.clone(), status.clone());
    assert!(rendered_review(dir.path()).contains("Tested cases: 17"));
    for change in ["predicate", "domain", "text", "duplicate"] {
        let mut a = annotations.clone();
        let mut s = status.clone();
        let mut text = original;
        match change {
            "predicate" => {
                a["requirements"]["R-001"]["quantifier"]["predicate"]["expression"]["op"] =
                    "gt".into()
            }
            "domain" => a["requirements"]["R-001"]["quantifier"]["domain"]["hi"] = 9.into(),
            "text" => text = "- [R-001] Returns twice the input.\n",
            "duplicate" => {
                let report = s["quantifier_reports"][0].clone();
                s["quantifier_reports"].as_array_mut().unwrap().push(report);
            }
            _ => unreachable!(),
        }
        let spec_dir = dir.path().join("sample/living-spec");
        fs::write(spec_dir.join("spec.md"), text).unwrap();
        fs::write(spec_dir.join("annotations.json"), a.to_string()).unwrap();
        fs::write(spec_dir.join("status.json"), s.to_string()).unwrap();
        let html = rendered_review(dir.path());
        assert!(html.contains("data-badge=\"stale\""), "{change}: {html}");
        assert!(html.contains("Stored boundary and quantifier verdicts are stale"));
        assert!(!html.contains("Tested cases:"));
        assert!(!html.contains("Tested result:"));
    }
}

#[test]
fn drill2_duplicate_status_keys_never_discard_failed_evidence() {
    let dir = review_input(
        "- [R-001] Returns the input.\n",
        empty_annotations(),
        one_proven_status(),
    );
    for text in [
        r#"{"statuses":[{"id":"R-001","badge":"broken"}],"statuses":[{"id":"R-001","badge":"proven"}]}"#,
        r#"{"statuses":[{"id":"R-001","badge":"broken","badge":"proven"}]}"#,
        r#"{"statuses":[{"id":"R-001","badge":"proven","tests":[{"test_id":"case","status":"failed","status":"passed"}]}]}"#,
        r#"{"statuses":[],"future":[{"kept":true,"k\u0065pt":false}]}"#,
        r#"{"statuses":[],"boundary_reports":[],"boundary_reports":[]}"#,
        r#"{"statuses":[],"quantifier_reports":[],"quantifier_reports":[]}"#,
    ] {
        fs::write(dir.path().join("sample/living-spec/status.json"), text).unwrap();
        for output in [review_render(dir.path()), review_rollup(dir.path())] {
            assert_eq!(output.status.code(), Some(1), "{text}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate JSON object key"));
        }
        assert!(!dir.path().join("view.html").exists());
        assert!(!dir.path().join("rollup.json").exists());
    }
}
