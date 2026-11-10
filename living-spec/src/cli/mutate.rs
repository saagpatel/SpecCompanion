use super::{link::load, Exit};
use crate::{
    agents::test_index::{identity_root, TestEntry, TestIndex},
    core::mutation::{LinkRecord, MutationFile},
    mutation::{run_link, Options},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const USAGE: &str = "usage: living-spec mutate <spec dir> --tests <root> [--crate <dir>] [--max-mutants N] [--jobs N] [--timeout-secs N] [--only REQ-ID]... [--out <file>] (completed runs record error links and exit successfully)";

struct Args {
    dir: PathBuf,
    tests: PathBuf,
    crate_dir: Option<PathBuf>,
    out: Option<PathBuf>,
    cargo: PathBuf,
    max_mutants: usize,
    jobs: usize,
    timeout: u64,
    only: BTreeSet<String>,
}

fn usage(message: &str) -> Box<dyn Error> {
    Exit::Usage(format!("{USAGE}\n{message}")).into()
}

fn positive(value: &str, flag: &str) -> Result<usize, Box<dyn Error>> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| usage(&format!("invalid {flag}")))?;
    if parsed == 0 {
        return Err(usage(&format!("{flag} must be positive")));
    }
    Ok(parsed)
}

fn parse(args: &[String]) -> Result<Args, Box<dyn Error>> {
    let dir = args
        .first()
        .filter(|arg| !arg.starts_with('-'))
        .ok_or_else(|| usage("missing spec directory"))?;
    let mut parsed = Args {
        dir: dir.into(),
        tests: PathBuf::new(),
        crate_dir: None,
        out: None,
        cargo: "cargo".into(),
        max_mutants: 8,
        jobs: 1,
        timeout: 180,
        only: BTreeSet::new(),
    };
    let mut seen = BTreeSet::new();
    let mut i = 1;
    while i < args.len() {
        let flag = args[i].as_str();
        let value = args
            .get(i + 1)
            .filter(|v| !v.starts_with('-'))
            .ok_or_else(|| usage(&format!("missing value for {flag}")))?;
        if flag == "--only" {
            if !parsed.only.insert(value.clone()) {
                return Err(usage("duplicate --only value"));
            }
        } else {
            if ![
                "--tests",
                "--crate",
                "--max-mutants",
                "--jobs",
                "--timeout-secs",
                "--out",
                "--cargo",
            ]
            .contains(&flag)
                || !seen.insert(flag)
            {
                return Err(usage(&format!("unknown or duplicate option {flag}")));
            }
            match flag {
                "--tests" => parsed.tests = value.into(),
                "--crate" => parsed.crate_dir = Some(value.into()),
                "--max-mutants" => parsed.max_mutants = positive(value, flag)?,
                "--jobs" => parsed.jobs = positive(value, flag)?,
                "--timeout-secs" => {
                    parsed.timeout = value
                        .parse::<u64>()
                        .map_err(|_| usage("invalid --timeout-secs"))?;
                    if parsed.timeout == 0 {
                        return Err(usage("--timeout-secs must be positive"));
                    }
                }
                "--out" => parsed.out = Some(value.into()),
                "--cargo" => parsed.cargo = value.into(),
                _ => unreachable!(),
            }
        }
        i += 2;
    }
    if parsed.tests.as_os_str().is_empty() {
        return Err(usage("--tests is required"));
    }
    Ok(parsed)
}

pub(super) fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let args = parse(args)?;
    let (spec, annotations) = load(&args.dir)?;
    let index = TestIndex::build(&args.tests)?;
    let lines: BTreeMap<_, _> = spec
        .lines
        .iter()
        .map(|line| (line.id.as_str(), line))
        .collect();
    let tests_root = identity_root(&args.tests);
    let requested_crate = args
        .crate_dir
        .as_ref()
        .map(|path| {
            let canonical = path.canonicalize()?;
            if !canonical.is_dir() {
                return Err(std::io::Error::other("--crate must be a directory"));
            }
            Ok(canonical)
        })
        .transpose()?;
    let mut selected: Vec<(String, String, String, String, TestEntry)> = vec![];
    for (requirement, annotation) in &annotations.requirements {
        if !args.only.is_empty() && !args.only.contains(requirement) {
            continue;
        }
        let Some(line) = lines.get(requirement.as_str()) else {
            for link in &annotation.links {
                println!(
                    "{requirement} {} skipped: requirement unresolved",
                    link.test_id
                );
            }
            continue;
        };
        for link in &annotation.links {
            if !link.is_accepted() {
                println!("{requirement} {} skipped: challenged link", link.test_id);
                continue;
            }
            if link.revision != line.revision || link.audit.revision != line.revision {
                println!(
                    "{requirement} {} skipped: stale requirement revision",
                    link.test_id
                );
                continue;
            }
            let Some(entry) = index.get(&link.test_id) else {
                println!("{requirement} {} skipped: unresolved test", link.test_id);
                continue;
            };
            if entry.fingerprint != link.fingerprint {
                println!(
                    "{requirement} {} skipped: stale test fingerprint",
                    link.test_id
                );
                continue;
            }
            if let Some(requested_crate) = &requested_crate {
                let package = test_package_dir(entry, tests_root);
                if package.as_ref() != Some(requested_crate) {
                    let reason = if package.is_some() {
                        "outside requested crate"
                    } else {
                        "test package unresolved"
                    };
                    println!("{requirement} {} skipped: {reason}", link.test_id);
                    continue;
                }
            }
            selected.push((
                requirement.clone(),
                link.test_id.clone(),
                link.fingerprint.clone(),
                line.revision.clone(),
                entry.clone(),
            ));
        }
    }
    selected.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    if !args.only.is_empty() {
        for id in &args.only {
            if !lines.contains_key(id.as_str()) {
                return Err(format!("unknown requirement id: {id}").into());
            }
        }
    }
    let options = Options {
        max_mutants: args.max_mutants,
        timeout: Duration::from_secs(args.timeout),
        cargo: args.cargo.clone(),
    };
    let cursor = Arc::new(Mutex::new(0usize));
    let results = Arc::new(Mutex::new(Vec::<(usize, LinkRecord)>::new()));
    thread::scope(|scope| {
        for _ in 0..args.jobs.min(selected.len().max(1)) {
            let cursor = Arc::clone(&cursor);
            let results = Arc::clone(&results);
            let selected = &selected;
            let options = &options;
            scope.spawn(move || loop {
                let i = {
                    let mut cursor = cursor.lock().expect("job index lock");
                    if *cursor >= selected.len() {
                        break;
                    }
                    let i = *cursor;
                    *cursor += 1;
                    i
                };
                let (req, _, _, revision, entry) = &selected[i];
                let record = run_link(options, entry, req, revision, tests_root);
                results.lock().expect("result lock").push((i, record));
            });
        }
    });
    let mut records = Arc::try_unwrap(results)
        .expect("workers complete")
        .into_inner()
        .expect("result lock");
    records.sort_by_key(|(i, _)| *i);
    let run_count = records.len();
    let checked = records
        .iter()
        .filter(|(_, link)| link.status == crate::core::mutation::LinkStatus::Checked)
        .count();
    let survived = records
        .iter()
        .filter(|(_, link)| link.status == crate::core::mutation::LinkStatus::Survived)
        .count();
    let unsupported = records
        .iter()
        .filter(|(_, link)| link.status == crate::core::mutation::LinkStatus::Unsupported)
        .count();
    for (_, record) in &records {
        print_record(record);
    }
    let engine_errors = records
        .iter()
        .filter(|(_, record)| record.status == crate::core::mutation::LinkStatus::Error)
        .count();
    let output = args.out.unwrap_or_else(|| args.dir.join("mutation.json"));
    let mut mutation = if output.exists() {
        MutationFile::from_bytes(&fs::read(&output)?)
            .map_err(|e| format!("invalid existing mutation file: {e}"))?
    } else {
        MutationFile {
            schema: "living-spec.mutation/v1".into(),
            engine: "rust-cargo-v1".into(),
            links: vec![],
            signoffs: vec![],
        }
    };
    for (_, record) in records {
        mutation
            .links
            .retain(|old| old.requirement != record.requirement || old.test_id != record.test_id);
        mutation.links.push(record);
    }
    mutation
        .links
        .sort_by(|a, b| (&a.requirement, &a.test_id).cmp(&(&b.requirement, &b.test_id)));
    write_atomic(
        &output,
        &mutation
            .to_pretty_bytes()
            .map_err(|e| format!("invalid mutation evidence: {e}"))?,
    )?;
    println!("mutation run complete: {run_count} links run, {checked} checked, {survived} survived, {unsupported} unsupported, {engine_errors} error");
    Ok(())
}

/// Resolve ownership from the test's path, even when its subject is unsupported.
fn test_package_dir(entry: &TestEntry, tests_root: &Path) -> Option<PathBuf> {
    let root = tests_root.canonicalize().ok()?;
    let file = root.join(&entry.file).canonicalize().ok()?;
    if !file.starts_with(&root) {
        return None;
    }
    let mut directory = file.parent()?;
    loop {
        if directory.join("Cargo.toml").is_file() {
            return Some(directory.to_path_buf());
        }
        if directory == root {
            return None;
        }
        directory = directory.parent()?;
        if !directory.starts_with(&root) {
            return None;
        }
    }
}

fn print_record(record: &LinkRecord) {
    let total = record.mutants.len();
    let killed = record
        .mutants
        .iter()
        .filter(|m| m.outcome == crate::core::mutation::MutantOutcome::Killed)
        .count();
    if let Some(survivor) = record
        .mutants
        .iter()
        .find(|m| m.outcome == crate::core::mutation::MutantOutcome::Survived)
    {
        println!(
            "{} survived {} {}:{} {}",
            record.requirement,
            survivor.id,
            survivor.file,
            survivor.line,
            operator_name(survivor.operator)
        );
    } else {
        match record.status {
            crate::core::mutation::LinkStatus::Checked => {
                println!("{} checked {killed}/{total} killed", record.requirement)
            }
            crate::core::mutation::LinkStatus::Unsupported => println!(
                "{} unsupported: {}",
                record.requirement,
                record.reason.as_deref().unwrap_or("unsupported")
            ),
            crate::core::mutation::LinkStatus::Error => println!(
                "{} error: {}",
                record.requirement,
                record.reason.as_deref().unwrap_or("engine error")
            ),
            crate::core::mutation::LinkStatus::Survived => println!(
                "{} survived (survivor details unavailable)",
                record.requirement
            ),
        }
    }
}

fn operator_name(operator: crate::core::mutation::Operator) -> &'static str {
    use crate::core::mutation::Operator::*;
    match operator {
        CmpFlip => "cmp_flip",
        ArithSwap => "arith_swap",
        BoolSwap => "bool_swap",
        IntBump => "int_bump",
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut file, bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::test_package_dir;
    use crate::agents::test_index::{Language, TestEntry};
    use std::fs;

    #[test]
    fn unsupported_test_outside_selected_crate_is_excluded_by_package_path() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let selected = root.join("selected");
        let other = root.join("other");
        fs::create_dir_all(selected.join("tests")).unwrap();
        fs::create_dir_all(other.join("tests")).unwrap();
        fs::write(selected.join("Cargo.toml"), "[package]\n").unwrap();
        fs::write(other.join("Cargo.toml"), "[package]\n").unwrap();
        fs::write(
            other.join("tests/unsupported.py"),
            "def test_case(): pass\n",
        )
        .unwrap();

        // Python tests have no Rust mutation subject, but --crate ownership is still knowable.
        let entry = TestEntry {
            id: "python:other/tests/unsupported.py:test_case".into(),
            name: "test_case".into(),
            file: "other/tests/unsupported.py".into(),
            line: 1,
            source: "def test_case(): pass\n".into(),
            body: "pass".into(),
            language: Language::Python,
            should_panic: false,
            attributes: vec![],
            unsupported_attributes: false,
            decorated: false,
            case_ids: vec![],
            unknown_cases: false,
            fingerprint: "sha256:test".into(),
            rename_hint: "test_case".into(),
        };

        let owner = test_package_dir(&entry, root).unwrap();
        assert_eq!(owner, other.canonicalize().unwrap());
        assert_ne!(owner, selected.canonicalize().unwrap());
    }
}
