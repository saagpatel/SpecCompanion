use crate::web::{render_dir, write_text};
use std::{error::Error, path::PathBuf};

const USAGE: &str = "usage: living-spec render <spec-dir> --out <file.html>";

pub(crate) fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let (dir, out) = parse(args)?;
    let html = render_dir(&dir)?;
    write_text(&out, &html)?;
    Ok(())
}

fn parse(args: &[String]) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    let mut dir = None;
    let mut out = None;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--out" {
            let value = next_value(args, &mut index, "--out")?;
            if out.is_some() {
                return Err(format!("{USAGE}\nduplicate --out").into());
            }
            out = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = arg.strip_prefix("--out=") {
            if value.is_empty() || out.is_some() {
                return Err(format!("{USAGE}\nmissing or duplicate --out").into());
            }
            out = Some(PathBuf::from(value));
            index += 1;
            continue;
        }
        if arg.starts_with('-') && arg != "-" {
            return Err(format!("{USAGE}\nunknown argument {arg}").into());
        }
        if dir.is_some() {
            return Err(format!("{USAGE}\nunexpected argument {arg}").into());
        }
        dir = Some(PathBuf::from(arg));
        index += 1;
    }
    let dir = dir.ok_or_else(|| format!("{USAGE}\nmissing <spec-dir>"))?;
    let out = out.ok_or_else(|| format!("{USAGE}\nmissing --out"))?;
    Ok((dir, out))
}

fn next_value<'a>(
    args: &'a [String],
    index: &mut usize,
    flag: &str,
) -> Result<&'a str, Box<dyn Error>> {
    let value = args
        .get(*index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{USAGE}\nmissing value for {flag}"))?;
    if value.starts_with("--") {
        return Err(format!("{USAGE}\nmissing value for {flag}").into());
    }
    *index += 2;
    Ok(value)
}
