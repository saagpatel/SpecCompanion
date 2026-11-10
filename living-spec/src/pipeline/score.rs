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
    agents::test_index::{identity_root, TestIndex},
    core::{badge::Badge, test_identity::Fingerprints},
    pipeline::rescore::{
        rescore_with_results, rescore_with_results_and_mutation, MutationScoreInput,
        RescoreSnapshot,
    },
    runner::rescore::default_commit,
};
use std::{collections::BTreeMap, error::Error, fs, path::Path};
pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let Some(dir) = args.first().filter(|s| !s.starts_with('-')) else {
        return Err(CommandExit::Usage(usage()).into());
    };
    let mut options = BTreeMap::new();
    let mut mutation_path = None;
    let mut mutation_seen = false;
    let mut index_arg = 1;
    while index_arg < args.len() {
        let flag = args[index_arg].as_str();
        if flag == "--mutation" {
            if mutation_seen {
                return Err(CommandExit::Usage(usage()).into());
            }
            mutation_seen = true;
            if let Some(path) = args
                .get(index_arg + 1)
                .filter(|value| !value.starts_with('-'))
            {
                mutation_path = Some(path.as_str());
                index_arg += 2;
            } else {
                index_arg += 1;
            }
            continue;
        }
        if ![
            "--results",
            "--format",
            "--commit",
            "--fingerprints",
            "--fingerprints-from",
            "--tests",
            "--html",
        ]
        .contains(&flag)
        {
            return Err(CommandExit::Usage(usage()).into());
        }
        let value = args
            .get(index_arg + 1)
            .ok_or_else(|| CommandExit::Usage(usage()))?;
        if value.starts_with('-') || options.insert(flag, value.as_str()).is_some() {
            return Err(CommandExit::Usage(usage()).into());
        }
        index_arg += 2;
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
    let report = if mutation_seen {
        let mutation_path = mutation_path
            .map(Path::new)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| dir.join("mutation.json"));
        let mutation_bytes = fs::read(&mutation_path)
            .map_err(|error| format!("mutation file {}: {error}", mutation_path.display()))?;
        let mutation = crate::core::mutation::MutationFile::from_bytes(&mutation_bytes)
            .map_err(|error| format!("invalid mutation file: {error}"))?;
        let digest = crate::core::freshness::sha256_bytes(&mutation_bytes);
        let locator = relative_locator(dir, &mutation_path)?;
        let tests_root = options
            .get("--tests")
            .or_else(|| options.get("--fingerprints-from"))
            .map(Path::new)
            .unwrap_or(Path::new("."));
        let tests_root = identity_root(tests_root);
        rescore_with_results_and_mutation(
            dir,
            &outcomes,
            &fingerprints,
            &commit,
            file,
            RescoreSnapshot {
                results: &results,
                index: index.as_ref(),
                mutation: Some(MutationScoreInput {
                    file: &mutation,
                    digest: &digest,
                    tests_root,
                    locator: &locator,
                }),
            },
        )?
    } else {
        rescore_with_results(
            dir,
            &outcomes,
            &fingerprints,
            &commit,
            file,
            &results,
            index.as_ref(),
        )?
    };
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

fn relative_locator(base: &Path, path: &Path) -> Result<String, Box<dyn Error>> {
    let base = base.canonicalize()?;
    let path = path.canonicalize()?;
    let left: Vec<_> = base.components().collect();
    let right: Vec<_> = path.components().collect();
    let common = left.iter().zip(&right).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return Err("mutation path cannot be made relative to spec directory".into());
    }
    let mut relative = std::path::PathBuf::new();
    for _ in common..left.len() {
        relative.push("..");
    }
    for component in &right[common..] {
        relative.push(component);
    }
    Ok(relative.to_string_lossy().into_owned())
}

fn usage() -> String {
    "usage: living-spec score|rescore <spec-dir> --results <file> --format libtest|junit [--commit SHA] [--html out.html] [--fingerprints <json> | --fingerprints-from <source-root> | --tests <source-root>] [--mutation [<path>]]".into()
}
fn badge_name(badge: Badge) -> &'static str {
    match badge {
        Badge::Proven => "proven",
        Badge::Broken => "broken",
        Badge::Unproven => "unproven",
        Badge::Qualified => "qualified",
    }
}
