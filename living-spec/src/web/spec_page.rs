use super::{
    html::{escape, format_percentage, STYLE},
    status::{load_boundary_reports, load_quantifier_reports, load_status},
};
use crate::core::{
    annotations::{Annotations, RequirementAnnotations},
    badge::{Badge, LineStatus, Reason},
    boundary::{
        coverage,
        parser::{requires_resolution, resolve_requirement},
        CaseStatus, CoverageMap,
    },
    history::{History, Transition},
    spec::{RequirementLine, Spec},
    test_identity::TestStatus,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ShownBadge {
    Proven,
    Broken,
    Unproven,
    Qualified,
    Unscored,
}

impl ShownBadge {
    fn label(self) -> &'static str {
        match self {
            Self::Proven => "proven",
            Self::Broken => "broken",
            Self::Unproven => "unproven",
            Self::Qualified => "qualified",
            Self::Unscored => "unscored",
        }
    }
}

struct Counts {
    proven: usize,
    broken: usize,
    unproven: usize,
    qualified: usize,
    unscored: usize,
}

struct Page<'a> {
    spec: &'a Spec,
    annotations: &'a Annotations,
    by_id: &'a BTreeMap<&'a str, &'a LineStatus>,
    history: &'a [Transition],
    scored: bool,
    quantifier_reports: &'a [crate::core::views::QuantifierReport],
    boundary_reports: &'a [crate::core::views::BoundaryReport],
}

impl Counts {
    fn total(&self) -> usize {
        self.proven + self.broken + self.unproven + self.qualified + self.unscored
    }
}

struct Classified {
    broken: Vec<usize>,
    middle: Vec<usize>,
    unscored: Vec<usize>,
    unproven: Vec<usize>,
    counts: Counts,
}

/// Render one self-contained HTML page.
///
/// `status` is `None` when the spec has no status report. Those lines use a
/// gray `unscored` badge and the page says they have not been scored.
/// Broken lines are rendered first, then proven and qualified lines grouped
/// under their headings. Unproven lines are last, under
/// "promises nobody has proven yet".
pub fn render_spec(
    spec: &Spec,
    annotations: &Annotations,
    status: Option<&[LineStatus]>,
    history: &[Transition],
) -> String {
    render_spec_with_reports(spec, annotations, status, history, &[], &[])
}

pub fn render_spec_with_reports(
    spec: &Spec,
    annotations: &Annotations,
    status: Option<&[LineStatus]>,
    history: &[Transition],
    quantifier_reports: &[crate::core::views::QuantifierReport],
    boundary_reports: &[crate::core::views::BoundaryReport],
) -> String {
    let scored = status.is_some();
    let current_status =
        status.map(|lines| display_statuses(spec, annotations, lines, boundary_reports));
    let mut by_id: BTreeMap<&str, &LineStatus> = BTreeMap::new();
    if let Some(lines) = current_status.as_deref() {
        for line in lines {
            by_id.insert(line.id.as_str(), line);
        }
    }
    let classified = classify(spec, scored, &by_id);
    let page = Page {
        spec,
        annotations,
        by_id: &by_id,
        history,
        scored,
        quantifier_reports,
        boundary_reports,
    };
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<title>Living spec</title>\n<style>");
    html.push_str(STYLE);
    html.push_str("</style>\n</head>\n<body>\n<header>\n<h1>Living spec</h1>\n");
    push_counts(&mut html, &classified.counts, scored);
    html.push_str("</header>\n");
    if !scored {
        html.push_str(
            "<p class=\"banner\">This living spec has not been scored yet. Lines below use a gray unscored badge.</p>\n",
        );
    }
    push_extra_status(&mut html, &page);
    push_orphan_annotations(&mut html, &page);
    push_groups(
        &mut html,
        &page,
        "page-summary",
        "Page summary",
        &classified.broken,
    );
    push_groups(
        &mut html,
        &page,
        "requirements",
        "Requirements",
        &classified.middle,
    );
    push_groups(
        &mut html,
        &page,
        "unscored",
        "not yet scored",
        &classified.unscored,
    );
    push_groups(
        &mut html,
        &page,
        "unproven",
        "promises nobody has proven yet",
        &classified.unproven,
    );
    html.push_str(
        "<footer><p>Offline view. This file does not load remote resources.</p></footer>\n</body>\n</html>\n",
    );
    html
}

// Stored outcomes remain visible as diagnostics. Only current, accepted
// link/audit revisions and complete boundary evidence may retain a positive badge.
pub(super) fn display_statuses(
    spec: &Spec,
    annotations: &Annotations,
    statuses: &[LineStatus],
    boundary_reports: &[CoverageMap],
) -> Vec<LineStatus> {
    let mut statuses = statuses.to_vec();
    for status in &mut statuses {
        if !matches!(status.badge, Badge::Proven | Badge::Qualified) {
            continue;
        }
        let Some(line) = spec.lines.iter().find(|line| line.id == status.id) else {
            continue;
        };
        let requirement = annotations.requirements.get(&line.id);
        let mut gaps = Vec::new();
        match requirement.filter(|requirement| !requirement.links.is_empty()) {
            None => gaps.push(Reason::NoLink),
            Some(requirement) => {
                let accepted: Vec<_> = requirement
                    .links
                    .iter()
                    .filter(|link| link.is_accepted())
                    .collect();
                if accepted.is_empty() {
                    gaps.push(Reason::LinkChallenged);
                } else {
                    let current: Vec<_> = accepted
                        .iter()
                        .filter(|link| {
                            link.revision == line.revision && link.audit.revision == line.revision
                        })
                        .collect();
                    if current.is_empty() {
                        gaps.push(Reason::StaleRevision);
                    } else {
                        for link in current {
                            let base = link
                                .test_id
                                .split_once('#')
                                .map_or(link.test_id.as_str(), |(base, _)| base);
                            let case_prefix = format!("{base}#");
                            let cases: Vec<_> = status
                                .tests
                                .iter()
                                .filter(|test| test.test_id.starts_with(&case_prefix))
                                .collect();
                            let tests: Vec<_> = if cases.is_empty() {
                                status
                                    .tests
                                    .iter()
                                    .filter(|test| test.test_id == link.test_id)
                                    .collect()
                            } else {
                                cases
                            };
                            if tests.iter().any(|test| {
                                matches!(test.status, TestStatus::Failed | TestStatus::Errored)
                            }) {
                                gaps.push(Reason::TestFailed);
                            }
                            if tests.is_empty()
                                || tests.iter().any(|test| test.status == TestStatus::Skipped)
                            {
                                gaps.push(Reason::NotExecuted);
                            }
                        }
                    }
                }
            }
        }
        if requires_resolution(line, requirement) {
            let (coverage, _) =
                boundary_projection(line, boundary_reports, requirement, Some(status));
            if coverage.as_ref().is_none_or(|map| {
                map.bounds.iter().any(|bound| {
                    bound
                        .cases
                        .iter()
                        .any(|case| case.status != CaseStatus::TestedPass)
                })
            }) {
                gaps.push(Reason::BoundaryGap);
            }
        }
        if !gaps.is_empty() {
            status.badge = if gaps.contains(&Reason::TestFailed) {
                Badge::Broken
            } else {
                Badge::Unproven
            };
            for reason in gaps {
                if !status.reasons.contains(&reason) {
                    status.reasons.push(reason);
                }
            }
        }
    }
    statuses
}

pub(crate) fn render_dir(dir: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let spec_path = dir.join("spec.md");
    let spec_text = fs::read_to_string(&spec_path)
        .map_err(|error| format!("failed to read {}: {error}", spec_path.display()))?;
    let spec = Spec::parse(&spec_text)?;
    let annotations = Annotations::load(dir.join("annotations.json"))?;
    let status_path = dir.join("status.json");
    let status = if status_path.is_file() {
        Some(load_status(&status_path)?)
    } else {
        None
    };
    let quantifier_reports = if status_path.is_file() {
        load_quantifier_reports(&status_path)?
    } else {
        Vec::new()
    };
    let boundary_reports = if status_path.is_file() {
        load_boundary_reports(&status_path)?
    } else {
        Vec::new()
    };
    let history = History::new(dir.join("history.jsonl")).read()?;
    Ok(render_spec_with_reports(
        &spec,
        &annotations,
        status.as_deref(),
        &history,
        &quantifier_reports,
        &boundary_reports,
    ))
}

fn classify(spec: &Spec, scored: bool, by_id: &BTreeMap<&str, &LineStatus>) -> Classified {
    let mut broken = Vec::new();
    let mut middle = Vec::new();
    let mut unscored = Vec::new();
    let mut unproven = Vec::new();
    let mut counts = Counts {
        proven: 0,
        broken: 0,
        unproven: 0,
        qualified: 0,
        unscored: 0,
    };
    for (index, line) in spec.lines.iter().enumerate() {
        let status = if scored {
            by_id.get(line.id.as_str()).copied()
        } else {
            None
        };
        match shown_badge(scored, status) {
            ShownBadge::Broken => {
                counts.broken += 1;
                broken.push(index);
            }
            ShownBadge::Proven => {
                counts.proven += 1;
                middle.push(index);
            }
            ShownBadge::Qualified => {
                counts.qualified += 1;
                middle.push(index);
            }
            ShownBadge::Unproven => {
                counts.unproven += 1;
                unproven.push(index);
            }
            ShownBadge::Unscored => {
                counts.unscored += 1;
                unscored.push(index);
            }
        }
    }
    Classified {
        broken,
        middle,
        unscored,
        unproven,
        counts,
    }
}

fn shown_badge(scored: bool, status: Option<&LineStatus>) -> ShownBadge {
    if !scored {
        return ShownBadge::Unscored;
    }
    match status.map(|line| line.badge) {
        Some(Badge::Proven) => ShownBadge::Proven,
        Some(Badge::Broken) => ShownBadge::Broken,
        Some(Badge::Unproven) => ShownBadge::Unproven,
        Some(Badge::Qualified) => ShownBadge::Qualified,
        None => ShownBadge::Unscored,
    }
}

fn push_counts(out: &mut String, counts: &Counts, scored: bool) {
    let total = counts.total();
    out.push_str("<p class=\"counts\" data-scored=\"");
    out.push_str(if scored { "true" } else { "false" });
    out.push_str("\" data-proven=\"");
    push_usize(out, counts.proven);
    out.push_str("\" data-broken=\"");
    push_usize(out, counts.broken);
    out.push_str("\" data-unproven=\"");
    push_usize(out, counts.unproven);
    out.push_str("\" data-qualified=\"");
    push_usize(out, counts.qualified);
    out.push_str("\" data-unscored=\"");
    push_usize(out, counts.unscored);
    out.push_str("\" data-proven-pct=\"");
    if scored {
        out.push_str(&format_percentage(counts.proven as u64, total as u64));
    } else {
        out.push_str("unscored");
    }
    out.push_str("\">proven ");
    push_usize(out, counts.proven);
    out.push_str(" · broken ");
    push_usize(out, counts.broken);
    out.push_str(" · unproven ");
    push_usize(out, counts.unproven);
    out.push_str(" · qualified ");
    push_usize(out, counts.qualified);
    if counts.unscored > 0 {
        out.push_str(" · unscored ");
        push_usize(out, counts.unscored);
    }
    out.push_str(" · ");
    if scored {
        out.push_str("proven ");
        out.push_str(&format_percentage(counts.proven as u64, total as u64));
        out.push('%');
    } else {
        out.push_str("not yet scored");
    }
    out.push_str("</p>\n");
}

fn push_usize(out: &mut String, value: usize) {
    out.push_str(&value.to_string());
}

fn push_extra_status(out: &mut String, page: &Page<'_>) {
    let known: BTreeSet<&str> = page
        .spec
        .lines
        .iter()
        .map(|line| line.id.as_str())
        .collect();
    let extra: Vec<&str> = page
        .by_id
        .keys()
        .copied()
        .filter(|id| !known.contains(id))
        .collect();
    if extra.is_empty() {
        return;
    }
    out.push_str("<p class=\"extra-status\">Status lines absent from spec.md:");
    for id in extra {
        out.push(' ');
        out.push_str(&escape(id));
    }
    out.push_str("</p>\n");
}

fn push_orphan_annotations(out: &mut String, page: &Page<'_>) {
    let orphans = page.annotations.orphans(page.spec);
    if orphans.is_empty() {
        return;
    }
    out.push_str("<section id=\"orphan-annotations\"><h2>Orphan annotations</h2><p>These IDs are absent from spec.md and are not counted as current requirements.</p>\n");
    for id in orphans {
        let requirement = page.annotations.requirements.get(&id);
        out.push_str("<article><h3>");
        out.push_str(&escape(&id));
        out.push_str("</h3>\n");
        push_tests(out, requirement, None);
        push_notes(out, requirement, None);
        out.push_str("<pre>");
        out.push_str(&escape(
            &serde_json::to_string_pretty(&requirement).expect("annotations serialize"),
        ));
        out.push_str("</pre></article>\n");
    }
    out.push_str("</section>\n");
}

fn push_groups(out: &mut String, page: &Page<'_>, id: &str, title: &str, indexes: &[usize]) {
    if indexes.is_empty() {
        return;
    }
    out.push_str("<section id=\"");
    out.push_str(id);
    out.push_str("\">\n<h2>");
    out.push_str(&escape(title));
    out.push_str("</h2>\n");
    if id == "page-summary" {
        out.push_str("<p>Broken lines are listed first.</p>\n");
    }
    for (heading, members) in group_indexes(page.spec, indexes) {
        out.push_str("<section class=\"heading-group\">\n<h3>");
        out.push_str(&escape(&heading));
        out.push_str("</h3>\n");
        for index in members {
            push_line(out, page, index);
        }
        out.push_str("</section>\n");
    }
    out.push_str("</section>\n");
}

fn group_indexes(spec: &Spec, indexes: &[usize]) -> Vec<(String, Vec<usize>)> {
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for &index in indexes {
        let label = heading_label(&spec.lines[index]);
        if let Some((_, members)) = groups.iter_mut().find(|(key, _)| key == &label) {
            members.push(index);
        } else {
            groups.push((label, vec![index]));
        }
    }
    groups
}

fn heading_label(line: &RequirementLine) -> String {
    if line.heading_path.is_empty() {
        "No heading".to_owned()
    } else {
        line.heading_path.join(" / ")
    }
}

fn push_line(out: &mut String, page: &Page<'_>, index: usize) {
    let line = &page.spec.lines[index];
    let status = if page.scored {
        page.by_id.get(line.id.as_str()).copied()
    } else {
        None
    };
    let badge = shown_badge(page.scored, status);
    let requirement = page.annotations.requirements.get(&line.id);
    out.push_str("<details class=\"line\" data-requirement=\"");
    out.push_str(&escape(&line.id));
    out.push_str("\" data-badge=\"");
    out.push_str(badge.label());
    out.push_str("\">\n<summary>");
    push_badge(out, badge);
    out.push_str(" <span class=\"req-text\">");
    out.push_str(&escape(&line.text));
    out.push_str("</span> <code class=\"req-id\">");
    out.push_str(&escape(&line.id));
    out.push_str("</code></summary>\n<div class=\"drill\">\n");
    push_tests(out, requirement, status);
    if badge == ShownBadge::Broken {
        push_breaking(out, page.history, &line.id);
    }
    push_notes(out, requirement, Some(&line.revision));
    push_reasons(out, status);
    push_boundary(out, line, page.boundary_reports, requirement, status);
    push_quantifier(out, line, page.quantifier_reports, requirement);
    push_dependency_path(out, requirement, status);
    out.push_str("</div></details>\n");
}

fn push_badge(out: &mut String, badge: ShownBadge) {
    out.push_str("<span class=\"badge badge-");
    out.push_str(badge.label());
    out.push_str("\">");
    out.push_str(badge.label());
    out.push_str("</span>");
}

fn push_tests(
    out: &mut String,
    requirement: Option<&RequirementAnnotations>,
    status: Option<&LineStatus>,
) {
    out.push_str("<h3>Linked tests</h3>\n<ul>\n");
    let tests = linked_tests(requirement, status);
    if tests.is_empty() {
        out.push_str("<li>none</li>\n");
    } else {
        for (id, label) in tests {
            out.push_str("<li><code>");
            out.push_str(&escape(&id));
            out.push_str("</code> <span class=\"test-status\">");
            out.push_str(&escape(&label));
            out.push_str("</span></li>\n");
        }
    }
    out.push_str("</ul>\n");
}

fn linked_tests(
    requirement: Option<&RequirementAnnotations>,
    status: Option<&LineStatus>,
) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some(requirement) = requirement {
        for link in &requirement.links {
            let label = status
                .and_then(|line| line.tests.iter().find(|test| test.test_id == link.test_id))
                .map(|test| status_label(test.status))
                .unwrap_or("not recorded");
            seen.insert(link.test_id.clone());
            rows.push((link.test_id.clone(), label.to_owned()));
        }
    }
    if let Some(line) = status {
        for test in &line.tests {
            if seen.insert(test.test_id.clone()) {
                rows.push((test.test_id.clone(), status_label(test.status).to_owned()));
            }
        }
    }
    rows
}

fn status_label(status: TestStatus) -> &'static str {
    match status {
        TestStatus::Passed => "passed",
        TestStatus::Failed => "failed",
        TestStatus::Errored => "errored",
        TestStatus::Skipped => "skipped",
    }
}

fn push_breaking(out: &mut String, history: &[Transition], id: &str) {
    out.push_str("<h3>Breaking commit</h3>\n");
    if let Some(transition) = history.iter().rev().find(|transition| {
        transition.requirement == id
            && transition.to == Badge::Broken
            && transition.from != Badge::Broken
    }) {
        out.push_str("<p class=\"breaking-commit\">breaking commit <code>");
        out.push_str(&escape(&transition.commit));
        out.push_str("</code></p>\n<p class=\"history-note\">");
        out.push_str(&escape(&transition.note));
        out.push_str("</p>\n");
    } else {
        out.push_str("<p class=\"breaking-commit\">breaking commit not recorded</p>\n");
    }
}

fn push_notes(
    out: &mut String,
    requirement: Option<&RequirementAnnotations>,
    revision: Option<&str>,
) {
    out.push_str("<h3>Auditor notes</h3>\n<ul>\n");
    if let Some(requirement) = requirement {
        for link in &requirement.links {
            if link.audit.verdict == crate::core::annotations::AuditVerdict::Challenged
                && revision == Some(link.revision.as_str())
            {
                if let Some(reason) = link.human_override_reason() {
                    out.push_str("<li class=\"human-override\">accepted by human override: ");
                    out.push_str(&escape(reason));
                    out.push_str("</li>\n");
                }
            }
        }
    }
    let notes = auditor_notes(requirement);
    if notes.is_empty() {
        out.push_str("<li>none</li>\n");
    } else {
        for note in notes {
            out.push_str("<li>");
            out.push_str(&escape(note));
            out.push_str("</li>\n");
        }
    }
    out.push_str("</ul>\n");
}

fn auditor_notes(requirement: Option<&RequirementAnnotations>) -> Vec<&str> {
    let Some(requirement) = requirement else {
        return Vec::new();
    };
    requirement
        .links
        .iter()
        .flat_map(|link| link.audit.notes.iter().map(String::as_str))
        .filter(|note| !note.is_empty())
        .collect()
}

fn push_reasons(out: &mut String, status: Option<&LineStatus>) {
    out.push_str("<h3>Reasons</h3>\n<ul>\n");
    match status {
        Some(line) if !line.reasons.is_empty() => {
            for reason in &line.reasons {
                out.push_str("<li>");
                out.push_str(&escape(&reason_text(reason)));
                out.push_str("</li>\n");
            }
        }
        _ => out.push_str("<li>none</li>\n"),
    }
    out.push_str("</ul>\n");
}

fn reason_text(reason: &Reason) -> String {
    match reason {
        Reason::NoLink => "no_link — no accepted test link is available.".to_owned(),
        Reason::LinkChallenged => {
            "link_challenged — the auditor challenged the linked test.".to_owned()
        }
        Reason::HumanOverride => {
            "human_override — a human explicitly accepted an auditor challenge.".to_owned()
        }
        Reason::StaleRevision => {
            "stale_revision — the requirement changed after this link was reviewed.".to_owned()
        }
        Reason::TestChanged => {
            "test_changed — the linked test source changed and needs a fresh audit.".to_owned()
        }
        Reason::NotExecuted => {
            "not_executed — the linked test did not execute in this result set.".to_owned()
        }
        Reason::TestFailed => {
            "test_failed — at least one accepted linked test failed or errored.".to_owned()
        }
        Reason::LinkUnresolved => {
            "link_unresolved — a linked test could not be resolved in the current source."
                .to_owned()
        }
        Reason::BoundaryGap => {
            "boundary_gap — one or more required boundary cases lack an executed passing test."
                .to_owned()
        }
        Reason::PrerequisiteBroken { path } => {
            let joined = path.join(" → ");
            format!(
                "prerequisite_broken: {joined} — a prerequisite is broken along this minimal path."
            )
        }
        Reason::PrerequisiteUnproven { path } => {
            format!(
                "prerequisite_unproven — a prerequisite is still unproven along this minimal path: {}.",
                path.join(" → ")
            )
        }
        Reason::PrerequisiteQualified { path } => {
            format!(
                "prerequisite_qualified — a prerequisite is qualified along this minimal path: {}.",
                path.join(" → ")
            )
        }
        Reason::DependencyCycle { path } => {
            format!(
                "dependency_cycle — this dependency cycle needs review: {}.",
                path.join(" → ")
            )
        }
    }
}

// Validate the entire stored shape against generated current cases before using
// any receipt. An invalid stored report falls back to every current case untested.
fn boundary_projection(
    line: &RequirementLine,
    reports: &[CoverageMap],
    requirement: Option<&RequirementAnnotations>,
    status: Option<&LineStatus>,
) -> (Option<CoverageMap>, bool) {
    let Ok(resolved) = resolve_requirement(line, requirement) else {
        return (None, true);
    };
    let Ok(expected) = coverage(&line.id, &resolved.bounds, &[], &[]) else {
        return (None, true);
    };
    // Inference is bound to the entire revision, including acceptance wording.
    if resolved.stale {
        return (Some(expected), true);
    }
    let stored: Vec<_> = reports
        .iter()
        .filter(|report| report.requirement == line.id)
        .collect();
    if !stored.is_empty() {
        return if stored.len() == 1
            && stored[0].links_fingerprint.as_deref()
                == Some(
                    crate::core::boundary::coverage::links_fingerprint(&resolved.links).as_str(),
                )
            && boundary_shape_matches(stored[0], &expected)
        {
            (Some(stored[0].clone()), false)
        } else {
            (Some(expected), true)
        };
    }
    let outcomes = status
        .map(|line| {
            line.tests
                .iter()
                .map(|test| crate::core::test_identity::TestOutcome {
                    test_id: test.test_id.clone(),
                    status: test.status,
                    duration_ms: test.duration_ms,
                    failure_message: None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    (
        coverage(&line.id, &resolved.bounds, &resolved.links, &outcomes).ok(),
        false,
    )
}

fn boundary_shape_matches(stored: &CoverageMap, expected: &CoverageMap) -> bool {
    if stored.bounds.len() != expected.bounds.len() {
        return false;
    }
    expected.bounds.iter().all(|wanted| {
        let matches: Vec<_> = stored
            .bounds
            .iter()
            .filter(|bound| bound.bound == wanted.bound)
            .collect();
        if matches.len() != 1 || matches[0].cases.len() != wanted.cases.len() {
            return false;
        }
        wanted.cases.iter().all(|case| {
            let matches: Vec<_> = matches[0]
                .cases
                .iter()
                .filter(|stored| {
                    stored.position == case.position
                        && stored.value == case.value
                        && stored.unit == case.unit
                        && stored.expected == case.expected
                })
                .collect();
            matches.len() == 1
                && match matches[0].status {
                    CaseStatus::Untested => matches[0].reason.is_some(),
                    CaseStatus::TestedPass | CaseStatus::TestedFail => {
                        matches[0]
                            .test_id
                            .as_ref()
                            .is_some_and(|id| !id.trim().is_empty())
                            && matches[0].reason.is_none()
                    }
                }
        })
    })
}

fn push_boundary(
    out: &mut String,
    line: &RequirementLine,
    reports: &[CoverageMap],
    requirement: Option<&RequirementAnnotations>,
    status: Option<&LineStatus>,
) {
    if !requires_resolution(line, requirement) {
        return;
    }
    let (map, invalid_stored) = boundary_projection(line, reports, requirement, status);
    if map.as_ref().is_some_and(|map| map.bounds.is_empty()) {
        return;
    }
    if invalid_stored {
        out.push_str("<p class=\"banner\">Stored boundary report is incomplete, duplicated, mismatched, or stale. Current cases are untested until rescoring.</p>\n");
    }
    out.push_str("<h3>Boundary coverage</h3>\n<table><thead><tr><th>Boundary</th><th>Value</th><th>Test</th><th>Status</th></tr></thead><tbody>\n");
    if let Some(map) = map {
        for bound in &map.bounds {
            for case in &bound.cases {
                out.push_str("<tr><td>");
                out.push_str(&escape(&format!(
                    "{} ({})",
                    bound.bound.quantity, case.position
                )));
                out.push_str("</td><td>");
                out.push_str(&escape(&case.value));
                if let Some(unit) = case.unit {
                    out.push(' ');
                    out.push_str(&escape(&unit.to_string()));
                }
                out.push_str("</td><td><code>");
                out.push_str(&escape(case.test_id.as_deref().unwrap_or("none")));
                out.push_str("</code></td><td>");
                out.push_str(match case.status {
                    CaseStatus::TestedPass => "tested pass",
                    CaseStatus::TestedFail => "tested fail",
                    CaseStatus::Untested => "untested",
                });
                if let Some(reason) = &case.reason {
                    out.push_str(" ( ");
                    out.push_str(&escape(&format!("{reason:?}")));
                    out.push_str(" )");
                }
                out.push_str("</td></tr>\n");
            }
        }
    } else {
        out.push_str("<tr><td colspan=\"4\">Boundary coverage is unavailable because its annotations or result data are invalid.</td></tr>\n");
    }
    out.push_str("</tbody></table>\n");
}

fn push_quantifier(
    out: &mut String,
    line: &RequirementLine,
    reports: &[crate::core::views::QuantifierReport],
    requirement: Option<&RequirementAnnotations>,
) {
    let Some(section) = requirement.and_then(|a| a.quantifier.as_ref()) else {
        return;
    };
    out.push_str("<h3>Quantifier Map</h3>\n");
    let mut matching = reports.iter().filter(|report| report.id == line.id);
    let Some(report) = matching.next() else {
        out.push_str("<p>Quantifier claim has not been scored.</p>\n");
        return;
    };
    let current = serde_json::from_value::<crate::core::quantifier::Quantifier>(section.clone());
    if matching.next().is_some()
        || report.text != line.text
        || current
            .as_ref()
            .ok()
            .is_none_or(|claim| claim != &report.quantifier)
    {
        out.push_str("<p>Stored quantifier report is stale or mismatched. Current claim has not been scored.</p>\n");
        return;
    }
    match crate::core::views::quantifier_html_report(report) {
        Ok(html) => {
            out.push_str(&html);
        }
        Err(_) => out.push_str("<p>Quantifier report is unavailable.</p>\n"),
    }
}

fn push_dependency_path(
    out: &mut String,
    requirement: Option<&RequirementAnnotations>,
    status: Option<&LineStatus>,
) {
    let Some(path) = status.and_then(|line| {
        line.reasons.iter().find_map(|r| match r {
            Reason::PrerequisiteBroken { path }
            | Reason::PrerequisiteUnproven { path }
            | Reason::PrerequisiteQualified { path } => Some(path.as_slice()),
            Reason::DependencyCycle { path } => Some(path.as_slice()),
            _ => None,
        })
    }) else {
        return;
    };
    if requirement.is_some_and(|a| !a.depends_on.is_empty()) {
        out.push_str("<h3>Minimal dependency path</h3><p>");
        out.push_str(&escape(&path.join(" → ")));
        out.push_str("</p>\n");
    }
}
