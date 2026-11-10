use super::Exit;
use crate::{
    core::{
        annotations::Annotations,
        badge::{Badge, LineStatus, Reason},
        freshness::{missing_inputs, ScoreInputs},
        spec::Spec,
    },
    deps::{model::DepGraph, propagate::apply_qualified, report},
};
use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    io::{self, BufWriter, Write},
    path::Path,
};

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.len() < 2
        || !matches!(args[0].as_str(), "check" | "report")
        || (args[0] == "check" && args.len() != 2)
    {
        return Err(Exit::Usage("usage: living-spec deps check|report <spec-dir> [--status path] [--allow-stale] [--md path] [--json path]".into()).into());
    }
    let dir = Path::new(&args[1]);
    let spec = Spec::parse(&fs::read_to_string(dir.join("spec.md"))?)?;
    let annotations = Annotations::load(dir.join("annotations.json"))?;
    let graph = DepGraph::new(&spec, &annotations)?;
    match args[0].as_str() {
        "check" if args.len() == 2 => {
            let cycles = graph.detect_cycles();
            if cycles.is_empty() {
                println!("No dependency cycles");
            } else {
                for cycle in cycles {
                    eprintln!("{}", cycle.join(" → "));
                }
                return Err(Exit::DependencyCycles.into());
            }
        }
        "report" => {
            let mut status = None;
            let mut md = None;
            let mut json = None;
            let mut allow_stale = false;
            let mut options = args[2..].iter();
            while let Some(option) = options.next() {
                if option == "--allow-stale" && !allow_stale {
                    allow_stale = true;
                    continue;
                }
                let value = options.next().ok_or("option requires a path")?;
                match option.as_str() {
                    "--status" if status.is_none() => status = Some(value),
                    "--md" if md.is_none() => md = Some(value),
                    "--json" if json.is_none() => json = Some(value),
                    _ => return Err(Exit::Usage("unknown or duplicate option".into()).into()),
                }
            }
            let mut lines: Vec<LineStatus> = if let Some(path) = status {
                crate::web::load_status(Path::new(path))?
            } else {
                spec.lines
                    .iter()
                    .map(|l| LineStatus {
                        id: l.id.clone(),
                        badge: Badge::Unproven,
                        reasons: Vec::new(),
                        tests: Vec::new(),
                        mutation: None,
                    })
                    .collect()
            };
            if let Some(path) = status {
                let bytes = fs::read(path)?;
                crate::core::json_input::reject_duplicate_keys(&bytes)?;
                let document: serde_json::Value = serde_json::from_slice(&bytes)?;
                let boundary: Vec<crate::core::views::BoundaryReport> = serde_json::from_value(
                    document
                        .get("boundary_reports")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([])),
                )?;
                let quantifier: Vec<crate::core::views::QuantifierReport> = serde_json::from_value(
                    document
                        .get("quantifier_reports")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!([])),
                )?;
                let inputs: Option<ScoreInputs> = document
                    .get("inputs")
                    .cloned()
                    .filter(|v| !v.is_null())
                    .map(serde_json::from_value)
                    .transpose()?;
                let mut freshness = match &inputs {
                    Some(inputs) => inputs.check(dir, &spec, &lines, &boundary, &quantifier)?,
                    None => missing_inputs(&spec),
                };
                let ids: BTreeSet<_> = lines.iter().map(|l| l.id.as_str()).collect();
                if ids.len() != lines.len()
                    || ids.len() != graph.edges.len()
                    || ids.iter().any(|id| !graph.edges.contains_key(*id))
                {
                    freshness.stale = true;
                }
                if let Some(inputs) = &inputs {
                    for line in &lines {
                        let own = inputs
                            .requirements
                            .get(&line.id)
                            .and_then(|input| input.own_badge);
                        let expected = if line.badge == Badge::Qualified {
                            Badge::Proven
                        } else {
                            line.badge
                        };
                        if own != Some(expected) {
                            freshness.stale = true;
                        }
                    }
                }
                if freshness.stale {
                    if !allow_stale {
                        return Err(
                            "status is stale; rescore required (or use --allow-stale)".into()
                        );
                    }
                    let stale: Vec<_> = spec
                        .lines
                        .iter()
                        .map(|line| serde_json::json!({"id": line.id, "badge": "stale"}))
                        .collect();
                    let mut output: Box<dyn Write> = if let Some(path) = md {
                        Box::new(BufWriter::new(fs::File::create(path)?))
                    } else {
                        Box::new(BufWriter::new(io::stdout()))
                    };
                    write!(
                        output,
                        "# Dependency impact report\n\nStatus is stale; rescore required.\n\n"
                    )?;
                    for line in &spec.lines {
                        writeln!(output, "- {} — stale", line.id)?;
                    }
                    output.flush()?;
                    if let Some(path) = json {
                        let mut output = BufWriter::new(fs::File::create(path)?);
                        serde_json::to_writer_pretty(&mut output, &stale)?;
                        output.flush()?;
                    }
                    return Ok(());
                }
                let inputs = inputs.expect("fresh status has inputs");
                for line in &mut lines {
                    line.badge = inputs.requirements[&line.id]
                        .own_badge
                        .expect("fresh status has own badge");
                    line.reasons.retain(|reason| {
                        !matches!(
                            reason,
                            Reason::PrerequisiteBroken { .. }
                                | Reason::PrerequisiteUnproven { .. }
                                | Reason::PrerequisiteQualified { .. }
                                | Reason::DependencyCycle { .. }
                        )
                    });
                }
            }
            apply_qualified(&mut lines, &graph);
            let reports = report::impacts(&graph, &lines);
            let mut output: Box<dyn Write> = if let Some(path) = md {
                Box::new(BufWriter::new(fs::File::create(path)?))
            } else {
                Box::new(BufWriter::new(io::stdout()))
            };
            report::write_markdown(&mut output, &reports)?;
            output.flush()?;
            if let Some(path) = json {
                let mut output = BufWriter::new(fs::File::create(path)?);
                // Compact streaming JSON keeps shared graph exports within the size budget.
                serde_json::to_writer(&mut output, &reports)?;
                output.flush()?;
            }
        }
        _ => unreachable!("deps command validated before loading"),
    }
    Ok(())
}
