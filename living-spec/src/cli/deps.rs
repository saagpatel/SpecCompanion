use super::Exit;
use crate::{
    core::{
        annotations::Annotations,
        badge::{Badge, LineStatus},
        spec::Spec,
    },
    deps::{model::DepGraph, propagate::apply_qualified, report},
};
use std::{collections::BTreeSet, error::Error, fs, path::Path};

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.len() < 2
        || !matches!(args[0].as_str(), "check" | "report")
        || (args[0] == "check" && args.len() != 2)
    {
        return Err(Exit::Usage("usage: living-spec deps check|report <spec-dir> [--status path] [--md path] [--json path]".into()).into());
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
            for pair in args[2..].chunks(2) {
                if pair.len() != 2 {
                    return Err(Exit::Usage("option requires a path".into()).into());
                }
                match pair[0].as_str() {
                    "--status" if status.is_none() => status = Some(&pair[1]),
                    "--md" if md.is_none() => md = Some(&pair[1]),
                    "--json" if json.is_none() => json = Some(&pair[1]),
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
                    })
                    .collect()
            };
            let ids: BTreeSet<_> = lines.iter().map(|l| l.id.as_str()).collect();
            if ids.len() != lines.len()
                || ids.len() != graph.edges.len()
                || ids.iter().any(|id| !graph.edges.contains_key(*id))
            {
                return Err("status must contain each requirement exactly once".into());
            }
            apply_qualified(&mut lines, &graph);
            let reports = report::impacts(&graph, &lines);
            let text = report::markdown(&reports);
            if let Some(path) = md {
                fs::write(path, &text)?;
            } else {
                print!("{text}");
            }
            if let Some(path) = json {
                fs::write(path, serde_json::to_vec_pretty(&reports)?)?;
            }
        }
        _ => unreachable!("deps command validated before loading"),
    }
    Ok(())
}
