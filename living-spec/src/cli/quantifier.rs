//! Command line entry point for offline quantified fixture checks and exports.
//! Check/export commands report counterexamples as data and exit successfully;
//! invalid inputs and I/O/checker errors return a nonzero process status.
use crate::quantifier::{self, annotations::Kind, view, Claim, Loaded, Report, Result};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Finite,
    Sampled,
}

pub fn run(args: &[String]) -> std::result::Result<(), Box<dyn Error>> {
    let Some((command, rest)) = args.split_first() else {
        return Err(usage().into());
    };
    match command.as_str() {
        "count" if rest.is_empty() => {
            let loaded = quantifier::load(&quantifier::reference_dir())?;
            println!("{}", loaded.claims.len());
            Ok(())
        }
        "check" => {
            let (source, filter) = parse_check(rest)?;
            let loaded = load_source(&source)?;
            let reports = selected_reports(&loaded, filter)?;
            println!("{}", view::json(&reports)?);
            Ok(())
        }
        "view" => {
            let (source, output) = parse_output(rest, "--out")?;
            let format = match output.extension().and_then(|ext| ext.to_str()) {
                Some("html") => "html",
                Some("md" | "markdown") => "markdown",
                _ => return Err(usage().into()),
            };
            let loaded = load_source(&source)?;
            let reports = all_reports(&loaded)?;
            let rendered = if format == "html" {
                view::html(&reports)?
            } else {
                view::markdown(&reports)?
            };
            fs::write(output, rendered)?;
            Ok(())
        }
        "edition" => {
            let (source, output_dir) = parse_output(rest, "--out-dir")?;
            let loaded = load_source(&source)?;
            let reports = all_reports(&loaded)?;
            let edition = quantifier::export::edition::build(&loaded, reports);
            quantifier::export::edition::write(&edition, &output_dir)?;
            Ok(())
        }
        "counterexample" => {
            let (source, output_dir) = parse_output(rest, "--out-dir")?;
            let loaded = load_source(&source)?;
            let reports = all_reports(&loaded)?;
            quantifier::export::counterexample::write(&reports, &output_dir)?;
            Ok(())
        }
        _ => Err(usage().into()),
    }
}

fn usage() -> &'static str {
    "usage: living-spec quantifier check <spec-or-fixture|reference> [--finite-only|--sampled-only] | view <spec-or-fixture|reference> --out <file.md|file.html> | edition <spec-or-fixture|reference> --out-dir <dir> | counterexample <spec-or-fixture|reference> --out-dir <dir> | count"
}

fn parse_check(args: &[String]) -> std::result::Result<(String, Filter), &'static str> {
    if args.len() != 1 && args.len() != 2 {
        return Err(usage());
    }
    let filter = match args.get(1).map(String::as_str) {
        None => Filter::All,
        Some("--finite-only") => Filter::Finite,
        Some("--sampled-only") => Filter::Sampled,
        _ => return Err(usage()),
    };
    Ok((args[0].clone(), filter))
}

fn parse_output(
    args: &[String],
    option: &str,
) -> std::result::Result<(String, PathBuf), &'static str> {
    if args.len() != 3 || args[1] != option || args[0].is_empty() || args[2].is_empty() {
        return Err(usage());
    }
    Ok((args[0].clone(), PathBuf::from(&args[2])))
}

fn load_source(source: &str) -> Result<Loaded> {
    let path = if source == "reference" {
        quantifier::reference_dir()
    } else {
        Path::new(source).to_owned()
    };
    quantifier::load(&path)
}

fn selected_reports(loaded: &Loaded, filter: Filter) -> Result<Vec<Report>> {
    let mut reports = Vec::new();
    for claim in &loaded.claims {
        if selected(claim, filter) {
            reports.push(quantifier::check_claim(claim)?);
        }
    }
    Ok(reports)
}

fn all_reports(loaded: &Loaded) -> Result<Vec<Report>> {
    loaded.claims.iter().map(quantifier::check_claim).collect()
}

fn selected(claim: &Claim, filter: Filter) -> bool {
    let sampled =
        claim.quantifier.kind == Kind::InfiniteUniversal || !claim.quantifier.domain.is_finite();
    match filter {
        Filter::All => true,
        Filter::Finite => !sampled,
        Filter::Sampled => sampled,
    }
}
