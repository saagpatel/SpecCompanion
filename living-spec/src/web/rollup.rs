use super::{
    html::{escape, format_percentage, percentage, STYLE},
    spec_page::display_statuses,
    status::{load_boundary_reports, load_status},
};
use crate::core::{annotations::Annotations, spec::Spec};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectState {
    Scored,
    NotScored,
    NoLivingSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RollupRow {
    pub project: String,
    pub path: String,
    pub lines: u64,
    pub proven: u64,
    pub broken: u64,
    pub unproven: u64,
    pub qualified: u64,
    pub proven_pct: f64,
    pub state: ProjectState,
}

struct ParsedProject {
    name: String,
    path: String,
    archived: bool,
}

#[derive(Serialize)]
struct RollupDocument<'a> {
    total_proven: u64,
    total_lines: u64,
    total_proven_pct: f64,
    no_spec_count: usize,
    not_scored_count: usize,
    rows: &'a [RollupRow],
}

struct Summary {
    total_proven: u64,
    total_lines: u64,
    total_proven_pct: f64,
    no_spec_count: usize,
    not_scored_count: usize,
}

/// Kept projects (`derived.archived == false`) plus extra `--include` roots.
///
/// A project with `living-spec/status.json` is scored only if its IDs match the
/// current parsed spec exactly; incomplete, extra, or spec-less status is invalid input.
/// Positive badges also require current accepted annotation links with passing
/// outcomes and current boundary evidence, as on the spec page. A project
/// with `spec.md` and no status file is `not_scored` and is not given badges.
/// Anything else is `no_living_spec`. Archived registry projects are omitted.
/// Relative `workspace_root` is resolved from the registry file's directory.
pub fn collect_rollup(
    registry_path: &Path,
    includes: &[PathBuf],
) -> Result<Vec<RollupRow>, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(registry_path)
        .map_err(|error| format!("failed to read {}: {error}", registry_path.display()))?;
    let (workspace_raw, projects) = parse_registry(&text)?;
    let workspace = resolve_against(
        registry_path.parent().unwrap_or(Path::new(".")),
        &workspace_raw,
    );
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    let mut archived = BTreeSet::new();
    for project in projects {
        let root = resolve_against(&workspace, &project.path);
        let key = dedup_key(&root);
        if project.archived {
            archived.insert(key);
            continue;
        }
        if !seen.insert(key) {
            continue;
        }
        rows.push(row_for_root(project.name, project.path, &root)?);
    }
    for include in includes {
        if !include.is_dir() {
            return Err(format!("include path is not a directory: {}", include.display()).into());
        }
        let canonical = fs::canonicalize(include)?;
        if archived.contains(&canonical) || !seen.insert(canonical.clone()) {
            continue;
        }
        let name = canonical
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| canonical.to_string_lossy().into_owned());
        let path_label = include.to_string_lossy().into_owned();
        rows.push(row_for_root(name, path_label, include)?);
    }
    rows.sort_by(|left, right| {
        state_rank(left.state)
            .cmp(&state_rank(right.state))
            .then_with(|| left.proven_pct.total_cmp(&right.proven_pct))
            .then_with(|| left.project.cmp(&right.project))
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(rows)
}

pub fn export_rollup_json(rows: &[RollupRow]) -> String {
    let summary = summarize(rows);
    let document = RollupDocument {
        total_proven: summary.total_proven,
        total_lines: summary.total_lines,
        total_proven_pct: summary.total_proven_pct,
        no_spec_count: summary.no_spec_count,
        not_scored_count: summary.not_scored_count,
        rows,
    };
    let mut json = serde_json::to_string_pretty(&document).expect("rollup rows can be serialized");
    json.push('\n');
    json
}

/// Self-contained HTML table of scored and not-yet-scored rows, a separate
/// no-living-spec tally, and an embedded JSON export.
pub fn render_rollup(rows: &[RollupRow]) -> String {
    let summary = summarize(rows);
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<title>Living spec portfolio</title>\n<style>");
    html.push_str(STYLE);
    html.push_str("</style>\n</head>\n<body>\n<header>\n<h1>Living spec portfolio</h1>\n");
    html.push_str("<p class=\"counts\" data-total-proven=\"");
    html.push_str(&summary.total_proven.to_string());
    html.push_str("\" data-total-lines=\"");
    html.push_str(&summary.total_lines.to_string());
    html.push_str("\" data-total-proven-pct=\"");
    html.push_str(&format_percentage(
        summary.total_proven,
        summary.total_lines,
    ));
    html.push_str("\" data-no-spec=\"");
    html.push_str(&summary.no_spec_count.to_string());
    html.push_str("\">Portfolio proven ");
    html.push_str(&format_percentage(
        summary.total_proven,
        summary.total_lines,
    ));
    html.push_str("% (");
    html.push_str(&summary.total_proven.to_string());
    html.push_str(" of ");
    html.push_str(&summary.total_lines.to_string());
    html.push_str(" lines across scored projects).</p>\n</header>\n");
    html.push_str("<table>\n<caption>Scored projects by proven percentage, then projects not yet scored.</caption>\n<thead>\n<tr>");
    for heading in [
        "Project",
        "Path",
        "Lines",
        "Proven",
        "Broken",
        "Unproven",
        "Qualified",
        "Proven %",
        "State",
    ] {
        html.push_str("<th scope=\"col\">");
        html.push_str(heading);
        html.push_str("</th>");
    }
    html.push_str("</tr>\n</thead>\n<tbody>\n");
    for row in rows
        .iter()
        .filter(|row| row.state != ProjectState::NoLivingSpec)
    {
        push_row(&mut html, row);
    }
    html.push_str("</tbody>\n</table>\n");
    push_tally(&mut html, rows, summary.no_spec_count);
    html.push_str("<section id=\"json-export\">\n<h2>JSON export</h2>\n<pre>");
    html.push_str(&escape(&export_rollup_json(rows)));
    html.push_str("</pre>\n</section>\n<footer><p>Offline view. This file does not load remote resources.</p></footer>\n</body>\n</html>\n");
    html
}

fn push_row(out: &mut String, row: &RollupRow) {
    out.push_str("<tr data-project=\"");
    out.push_str(&escape(&row.project));
    out.push_str("\" data-state=\"");
    out.push_str(state_token(row.state));
    out.push_str("\" data-lines=\"");
    out.push_str(&row.lines.to_string());
    out.push_str("\" data-proven=\"");
    out.push_str(&row.proven.to_string());
    out.push_str("\" data-broken=\"");
    out.push_str(&row.broken.to_string());
    out.push_str("\" data-unproven=\"");
    out.push_str(&row.unproven.to_string());
    out.push_str("\" data-qualified=\"");
    out.push_str(&row.qualified.to_string());
    out.push_str("\" data-proven-pct=\"");
    if row.state == ProjectState::Scored {
        out.push_str(&format_percentage(row.proven, row.lines));
    }
    out.push_str("\">");
    push_cell(out, &row.project);
    push_cell(out, &row.path);
    push_cell(out, &row.lines.to_string());
    push_cell(out, &row.proven.to_string());
    push_cell(out, &row.broken.to_string());
    push_cell(out, &row.unproven.to_string());
    push_cell(out, &row.qualified.to_string());
    push_cell(out, &pct_label(row));
    push_cell(out, state_label(row.state));
    out.push_str("</tr>\n");
}

fn push_cell(out: &mut String, text: &str) {
    out.push_str("<td>");
    out.push_str(&escape(text));
    out.push_str("</td>");
}

fn push_tally(out: &mut String, rows: &[RollupRow], count: usize) {
    out.push_str("<section id=\"no-living-spec\">\n<h2>No living spec</h2>\n<p class=\"tally\">");
    out.push_str(&count.to_string());
    out.push(' ');
    if count == 1 {
        out.push_str("project has");
    } else {
        out.push_str("projects have");
    }
    out.push_str(" no living spec.</p>\n<ul>\n");
    for row in rows
        .iter()
        .filter(|row| row.state == ProjectState::NoLivingSpec)
    {
        out.push_str("<li>");
        out.push_str(&escape(&row.project));
        out.push_str(" <code>");
        out.push_str(&escape(&row.path));
        out.push_str("</code></li>\n");
    }
    out.push_str("</ul>\n</section>\n");
}

fn pct_label(row: &RollupRow) -> String {
    match row.state {
        ProjectState::Scored => format!("{}%", format_percentage(row.proven, row.lines)),
        ProjectState::NotScored => "not yet scored".to_owned(),
        ProjectState::NoLivingSpec => "no living spec".to_owned(),
    }
}

fn state_label(state: ProjectState) -> &'static str {
    match state {
        ProjectState::Scored => "scored",
        ProjectState::NotScored => "not yet scored",
        ProjectState::NoLivingSpec => "no living spec",
    }
}

fn state_token(state: ProjectState) -> &'static str {
    match state {
        ProjectState::Scored => "scored",
        ProjectState::NotScored => "not_scored",
        ProjectState::NoLivingSpec => "no_living_spec",
    }
}

fn state_rank(state: ProjectState) -> u8 {
    match state {
        ProjectState::Scored => 0,
        ProjectState::NotScored => 1,
        ProjectState::NoLivingSpec => 2,
    }
}

fn summarize(rows: &[RollupRow]) -> Summary {
    let mut total_proven = 0;
    let mut total_lines = 0;
    let mut no_spec_count = 0;
    let mut not_scored_count = 0;
    for row in rows {
        match row.state {
            ProjectState::Scored => {
                total_proven += row.proven;
                total_lines += row.lines;
            }
            ProjectState::NotScored => not_scored_count += 1,
            ProjectState::NoLivingSpec => no_spec_count += 1,
        }
    }
    Summary {
        total_proven_pct: percentage(total_proven, total_lines),
        total_proven,
        total_lines,
        no_spec_count,
        not_scored_count,
    }
}

fn row_for_root(
    project: String,
    path: String,
    root: &Path,
) -> Result<RollupRow, Box<dyn std::error::Error>> {
    let direct_spec_dir = root.join("living-spec");
    let nested_spec_dir = direct_spec_dir.join("living-spec");
    let spec_dir = if !direct_spec_dir.join("spec.md").is_file()
        && !direct_spec_dir.join("status.json").is_file()
        && direct_spec_dir.join("Cargo.toml").is_file()
        && nested_spec_dir.join("spec.md").is_file()
    {
        nested_spec_dir
    } else {
        direct_spec_dir
    };
    let status_path = spec_dir.join("status.json");
    let spec_path = spec_dir.join("spec.md");
    if status_path.is_file() {
        let spec = Spec::parse(&fs::read_to_string(&spec_path).map_err(|error| {
            format!(
                "status.json requires a readable spec.md at {}: {error}",
                spec_path.display()
            )
        })?)?;
        let lines = load_status(&status_path)?;
        let spec_ids: BTreeSet<_> = spec.lines.iter().map(|line| line.id.as_str()).collect();
        let status_ids: BTreeSet<_> = lines.iter().map(|line| line.id.as_str()).collect();
        if spec_ids != status_ids {
            return Err(format!(
                "{}: status IDs do not match spec.md; missing: {:?}; extra: {:?}",
                status_path.display(),
                spec_ids.difference(&status_ids).collect::<Vec<_>>(),
                status_ids.difference(&spec_ids).collect::<Vec<_>>()
            )
            .into());
        }
        let annotations_path = spec_dir.join("annotations.json");
        let annotations = if annotations_path.is_file() {
            Annotations::load(&annotations_path)?
        } else {
            Annotations {
                schema: "living-spec.annotations/v1".into(),
                requirements: Default::default(),
                extra: Default::default(),
            }
        };
        let reports = load_boundary_reports(&status_path)?;
        let lines = display_statuses(&spec, &annotations, &lines, &reports);
        let mut proven = 0;
        let mut broken = 0;
        let mut unproven = 0;
        let mut qualified = 0;
        for line in &lines {
            match line.badge {
                crate::core::badge::Badge::Proven => proven += 1,
                crate::core::badge::Badge::Broken => broken += 1,
                crate::core::badge::Badge::Unproven => unproven += 1,
                crate::core::badge::Badge::Qualified => qualified += 1,
            }
        }
        let count = proven + broken + unproven + qualified;
        return Ok(RollupRow {
            project,
            path,
            lines: count,
            proven,
            broken,
            unproven,
            qualified,
            proven_pct: percentage(proven, count),
            state: ProjectState::Scored,
        });
    }
    if spec_path.is_file() {
        let spec = Spec::parse(&fs::read_to_string(&spec_path)?)?;
        let count = u64::try_from(spec.lines.len()).unwrap_or(u64::MAX);
        return Ok(RollupRow {
            project,
            path,
            lines: count,
            proven: 0,
            broken: 0,
            unproven: 0,
            qualified: 0,
            proven_pct: 0.0,
            state: ProjectState::NotScored,
        });
    }
    Ok(RollupRow {
        project,
        path,
        lines: 0,
        proven: 0,
        broken: 0,
        unproven: 0,
        qualified: 0,
        proven_pct: 0.0,
        state: ProjectState::NoLivingSpec,
    })
}

fn parse_registry(text: &str) -> Result<(String, Vec<ParsedProject>), String> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("invalid registry: {error}"))?;
    let object = value.as_object().ok_or("registry must be a JSON object")?;
    let workspace = object
        .get("workspace_root")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("registry is missing workspace_root")?
        .to_owned();
    let projects = object
        .get("projects")
        .and_then(Value::as_array)
        .ok_or("registry is missing projects")?;
    let mut parsed = Vec::with_capacity(projects.len());
    for (index, project) in projects.iter().enumerate() {
        parsed.push(parse_project(index, project)?);
    }
    Ok((workspace, parsed))
}

fn parse_project(index: usize, value: &Value) -> Result<ParsedProject, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("registry project {index} is not an object"))?;
    let identity = object
        .get("identity")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("registry project {index} is missing identity"))?;
    let path = identity
        .get("path")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("registry project {index} is missing identity.path"))?
        .to_owned();
    let display_name = optional_string(identity, "display_name");
    let project_key = optional_string(identity, "project_key");
    let archived = archived_flag(object, index)?;
    Ok(ParsedProject {
        name: project_name(&display_name, &project_key, &path),
        path,
        archived,
    })
}

fn optional_string(object: &Map<String, Value>, key: &str) -> String {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

fn archived_flag(object: &Map<String, Value>, index: usize) -> Result<bool, String> {
    object
        .get("derived")
        .and_then(|derived| derived.get("archived"))
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("registry project {index} derived.archived must be a boolean"))
}

fn project_name(display_name: &str, project_key: &str, path: &str) -> String {
    if !display_name.trim().is_empty() {
        display_name.to_owned()
    } else if !project_key.trim().is_empty() {
        project_key.to_owned()
    } else {
        path.to_owned()
    }
}

fn resolve_against(base: &Path, raw: &str) -> PathBuf {
    let path = Path::new(raw);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn dedup_key(path: &Path) -> PathBuf {
    fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}
