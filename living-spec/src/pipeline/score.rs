pub use crate::core::test_identity::load_fingerprints;
#[derive(Debug)]
pub enum CommandExit {
    Usage(String),
    Broken,
}
impl std::fmt::Display for CommandExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) => f.write_str(message),
            Self::Broken => f.write_str("broken requirement lines found"),
        }
    }
}
impl std::error::Error for CommandExit {}

use crate::{
    adapters::{parse_junit, parse_junit_with, parse_libtest},
    agents::test_index::TestIndex,
    core::{badge::Badge, test_identity::Fingerprints},
    pipeline::rescore::rescore_with_results,
    runner::rescore::default_commit,
};
use std::{collections::BTreeMap, error::Error, fs, path::Path};
pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let Some(dir) = args.first().filter(|s| !s.starts_with('-')) else {
        return Err(CommandExit::Usage(usage()).into());
    };
    let mut options = BTreeMap::new();
    for pair in args[1..].chunks(2) {
        if pair.len() != 2
            || ![
                "--results",
                "--format",
                "--commit",
                "--fingerprints",
                "--fingerprints-from",
                "--tests",
                "--html",
            ]
            .contains(&pair[0].as_str())
            || options.insert(pair[0].as_str(), pair[1].as_str()).is_some()
        {
            return Err(CommandExit::Usage(usage()).into());
        }
    }
    if ["--fingerprints", "--fingerprints-from", "--tests"]
        .iter()
        .filter(|option| options.contains_key(**option))
        .count()
        > 1
    {
        return Err(CommandExit::Usage(usage()).into());
    }
    let index = options
        .get("--fingerprints-from")
        .or_else(|| options.get("--tests"))
        .map(|root| TestIndex::build(Path::new(root)))
        .transpose()?;
    let file = options
        .get("--results")
        .ok_or_else(|| CommandExit::Usage(usage()))?;
    let format = options.get("--format").copied();
    if !matches!(format, Some("libtest" | "junit")) {
        return Err(CommandExit::Usage(usage()).into());
    }
    let source = fs::read_to_string(file)?;
    let mut outcome_ids = BTreeMap::new();
    let outcomes = match format {
        Some("libtest") => parse_libtest(&source)?,
        Some("junit") => match index.as_ref() {
            Some(index) => parse_junit_with(&source, |class, name| {
                let raw_id = if class.is_empty() {
                    name.to_owned()
                } else {
                    format!("{class}::{name}")
                };
                let scoped_id = index.junit_id(class, name);
                outcome_ids.insert(raw_id, scoped_id.clone());
                scoped_id
            })?,
            None => parse_junit(&source)?,
        },
        _ => unreachable!("format validated before reading results"),
    };
    let fingerprints = match options.get("--fingerprints") {
        Some(path) => load_fingerprints(&fs::read_to_string(path)?)?,
        None => Fingerprints::new(),
    };
    let dir = Path::new(dir);
    let commit = options
        .get("--commit")
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_commit(dir));
    let results = crate::core::freshness::ResultsSnapshot::from_bytes(
        dir,
        Path::new(file),
        source.into_bytes(),
        format.expect("validated format"),
        outcome_ids,
    )?;
    let report = rescore_with_results(
        dir,
        &outcomes,
        &fingerprints,
        &commit,
        file,
        &results,
        index.as_ref(),
    )?;
    let html = crate::web::render_dir(dir)?;
    let destination = options
        .get("--html")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| dir.join("spec.html"));
    crate::web::write_text(&destination, &html)?;
    for badge in [
        Badge::Proven,
        Badge::Broken,
        Badge::Unproven,
        Badge::Qualified,
    ] {
        println!(
            "{}: {}",
            badge_name(badge),
            report.statuses.iter().filter(|s| s.badge == badge).count()
        );
    }
    for t in &report.transitions {
        println!(
            "{} {} -> {} ({})",
            t.requirement,
            badge_name(t.from),
            badge_name(t.to),
            t.note
        );
    }
    for status in report.statuses.iter().filter(|s| {
        s.badge == Badge::Broken && !report.transitions.iter().any(|t| t.requirement == s.id)
    }) {
        println!("{} already broken", status.id);
    }
    for orphan in &report.orphans {
        println!("orphan annotation: {orphan}");
    }
    if report.exit_code() == 2 {
        return Err(CommandExit::Broken.into());
    }
    Ok(())
}

fn usage() -> String {
    "usage: living-spec score|rescore <spec-dir> --results <file> --format libtest|junit [--commit SHA] [--html out.html] [--fingerprints <json> | --fingerprints-from <source-root> | --tests <source-root>]".into()
}
fn badge_name(badge: Badge) -> &'static str {
    match badge {
        Badge::Proven => "proven",
        Badge::Broken => "broken",
        Badge::Unproven => "unproven",
        Badge::Qualified => "qualified",
    }
}
