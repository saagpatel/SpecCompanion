use crate::web::{collect_rollup, export_rollup_json, render_rollup, write_text};
use std::{error::Error, path::PathBuf};

const USAGE: &str = "usage: living-spec rollup [--registry PATH] [--include DIR]... --out <file.html> [--json <file.json>]";

struct Options {
    registry: Option<String>,
    includes: Vec<PathBuf>,
    out: Option<PathBuf>,
    json: Option<PathBuf>,
}

pub(crate) fn run(args: &[String], registry_env: Option<&str>) -> Result<(), Box<dyn Error>> {
    let options = parse(args)?;
    let registry_env = registry_env.filter(|value| !value.is_empty());
    let registry = match (options.registry, registry_env) {
        (Some(path), _) => PathBuf::from(path),
        (None, Some(path)) => PathBuf::from(path),
        (None, None) => {
            return Err(
                format!("{USAGE}\npass --registry PATH or set LIVING_SPEC_REGISTRY").into(),
            );
        }
    };
    let out = options
        .out
        .ok_or_else(|| format!("{USAGE}\nmissing --out"))?;
    let rows = collect_rollup(&registry, &options.includes)?;
    write_text(&out, &render_rollup(&rows))?;
    if let Some(json_path) = options.json {
        write_text(&json_path, &export_rollup_json(&rows))?;
    }
    Ok(())
}

fn parse(args: &[String]) -> Result<Options, Box<dyn Error>> {
    let mut options = Options {
        registry: None,
        includes: Vec::new(),
        out: None,
        json: None,
    };
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg.starts_with("--") {
            let (flag, inline) = arg
                .split_once('=')
                .map(|(flag, value)| (flag, Some(value)))
                .unwrap_or((arg.as_str(), None));
            if !matches!(flag, "--registry" | "--include" | "--out" | "--json") {
                return Err(format!("{USAGE}\nunknown argument {flag}").into());
            }
            let value = if let Some(inline) = inline {
                index += 1;
                if inline.is_empty() {
                    return Err(format!("{USAGE}\nmissing value for {flag}").into());
                }
                inline.to_owned()
            } else {
                next_value(args, &mut index, flag)?
            };
            match flag {
                "--registry" => {
                    if options.registry.is_some() {
                        return Err(format!("{USAGE}\nduplicate --registry").into());
                    }
                    options.registry = Some(value);
                }
                "--include" => options.includes.push(PathBuf::from(value)),
                "--out" => {
                    if options.out.is_some() {
                        return Err(format!("{USAGE}\nduplicate --out").into());
                    }
                    options.out = Some(PathBuf::from(value));
                }
                "--json" => {
                    if options.json.is_some() {
                        return Err(format!("{USAGE}\nduplicate --json").into());
                    }
                    options.json = Some(PathBuf::from(value));
                }
                _ => return Err(format!("{USAGE}\nunknown argument {flag}").into()),
            }
            continue;
        }
        return Err(format!("{USAGE}\nunexpected argument {arg}").into());
    }
    Ok(options)
}

fn next_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, Box<dyn Error>> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("{USAGE}\nmissing value for {flag}"))?;
    if value.starts_with("--") {
        return Err(format!("{USAGE}\nmissing value for {flag}").into());
    }
    *index += 2;
    Ok(value.clone())
}
