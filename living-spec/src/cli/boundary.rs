use super::Exit;
use crate::{
    boundary::{
        coverage::{boundary_gaps, coverage},
        gen::generate,
        parser::{requires_resolution, resolve_requirement},
    },
    core::{annotations::Annotations, spec::Spec, test_identity::TestOutcome},
};
use std::{collections::BTreeSet, error::Error, fs, path::Path};

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let [command, dir, rest @ ..] = args else {
        return Err(Exit::Usage(
            "usage: living-spec boundary gen|coverage <spec-dir> [options]".into(),
        )
        .into());
    };
    if command != "coverage" && !(command == "gen" && rest.is_empty()) {
        return Err(Exit::Usage("unknown boundary command/options".into()).into());
    }
    let dir = Path::new(dir);
    let spec = Spec::parse(&fs::read_to_string(dir.join("spec.md"))?)?;
    let path = dir.join("annotations.json");
    let raw: serde_json::Value = if path.exists() {
        serde_json::from_str(&fs::read_to_string(&path)?)?
    } else {
        serde_json::json!({"schema":"living-spec.annotations/v1","requirements":{}})
    };
    let mut annotations = if path.exists() {
        Annotations::load(&path)?
    } else {
        serde_json::from_value(
            serde_json::json!({"schema":"living-spec.annotations/v1","requirements":{}}),
        )?
    };
    if annotations.schema != "living-spec.annotations/v1" {
        return Err("unsupported annotations schema".into());
    }
    if !annotations.orphans(&spec).is_empty() {
        return Err("orphan annotations found".into());
    }
    let boundary_lines: Vec<_> = spec
        .lines
        .iter()
        .filter(|line| requires_resolution(line, annotations.requirements.get(&line.id)))
        .collect();
    if command == "gen" && rest.is_empty() {
        // Prepare the complete update in memory; uncertainty never leaves a partial file.
        for line in &boundary_lines {
            let resolved = resolve_requirement(line, annotations.requirements.get(&line.id))?;
            let entry = annotations.requirements.entry(line.id.clone()).or_default();
            let bounds = resolved.bounds;
            entry.bounds_present = true;
            entry.extra.insert(
                "bounds_source".into(),
                serde_json::to_value(resolved.source)?,
            );
            if entry.extra.contains_key("boundary_links") {
                entry.extra.insert(
                    "boundary_links".into(),
                    serde_json::to_value(resolved.links)?,
                );
            }
            let cases = bounds
                .iter()
                .map(|b| Ok(serde_json::json!({"quantity": b.quantity, "cases": generate(b)?})))
                .collect::<Result<Vec<_>, crate::boundary::model::BoundaryError>>()?;
            entry.bounds = bounds
                .into_iter()
                .map(serde_json::to_value)
                .collect::<Result<_, _>>()?;
            entry
                .extra
                .insert("boundary_cases".into(), serde_json::to_value(cases)?);
        }
        // Patch only our sections. Retain even unknown fields inside another
        // capability's known typed sections, and retain authored empty bounds.
        let mut serialized = raw.clone();
        for line in &boundary_lines {
            let entry = &annotations.requirements[&line.id];
            serialized["requirements"][&line.id]["bounds"] = serde_json::to_value(&entry.bounds)?;
            for key in ["boundary_cases", "bounds_source", "boundary_links"] {
                if let Some(value) = entry.extra.get(key) {
                    serialized["requirements"][&line.id][key] = value.clone();
                }
            }
        }
        fs::write(
            path,
            format!("{}\n", serde_json::to_string_pretty(&serialized)?),
        )?;
        return Ok(());
    }
    let mut results = None;
    let mut format = None;
    let mut md = None;
    let mut json = None;
    let mut seen = BTreeSet::new();
    for pair in rest.chunks(2) {
        let [flag, value] = pair else {
            return Err(Exit::Usage("missing boundary option value".into()).into());
        };
        if !seen.insert(flag) {
            return Err(Exit::Usage(format!("duplicate option {flag}")).into());
        }
        match flag.as_str() {
            "--results" => results = Some(value),
            "--format" => format = Some(value.as_str()),
            "--md" => md = Some(value),
            "--json" => json = Some(value),
            _ => return Err(Exit::Usage(format!("unknown option {flag}")).into()),
        }
    }
    let results = results.ok_or_else(|| Exit::Usage("--results required".into()))?;
    let format = format.ok_or_else(|| Exit::Usage("--format required".into()))?;
    if !matches!(format, "libtest" | "junit") {
        return Err(Exit::Usage("format must be libtest or junit".into()).into());
    }
    let outcomes = parse_results(&fs::read_to_string(results)?, format)?;
    let mut maps = Vec::new();
    for line in &boundary_lines {
        let resolved = resolve_requirement(line, annotations.requirements.get(&line.id))?;
        maps.push(coverage(
            &line.id,
            &resolved.bounds,
            &resolved.links,
            &outcomes,
        )?);
    }
    let markdown = maps
        .iter()
        .map(|m| m.to_markdown())
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(path) = md {
        fs::write(path, &markdown)?;
    }
    if let Some(path) = json {
        fs::write(path, serde_json::to_string_pretty(&maps)?)?;
    }
    print!("{markdown}");
    if !boundary_gaps(&maps).is_empty() {
        return Err(Exit::BoundaryCoverage.into());
    }
    Ok(())
}

/// Both commands consume the same validated runner receipts.
pub fn parse_results(text: &str, format: &str) -> Result<Vec<TestOutcome>, Box<dyn Error>> {
    use crate::adapters::{parse_junit_with, parse_libtest};
    match format {
        "libtest" => Ok(parse_libtest(text)?),
        // Boundary links use literal case names; the shared adapter validates
        // completeness and rejects duplicate identities after this mapping.
        "junit" => Ok(parse_junit_with(text, |_, name| name.to_owned())?),
        _ => Err("format must be libtest or junit".into()),
    }
}
