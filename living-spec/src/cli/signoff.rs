use super::{link::load, Exit};
use crate::{
    agents::test_index::TestIndex,
    core::mutation::{LinkStatus, MutationFile, Signoff},
};
use std::{error::Error, fs, path::Path};

const USAGE: &str = "usage: living-spec signoff <spec dir> link <REQ> <test_id> --reason <text> --signed-by <handle>\n       living-spec signoff <spec dir> mutant <REQ> <test_id> <mutant_id> --reason <text> --signed-by <handle>";

pub(super) fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let usage = || Exit::Usage(USAGE.into());
    if args.len() < 2 {
        return Err(usage().into());
    }
    let dir = Path::new(&args[0]);
    let kind = args[1].as_str();
    let (requirement, test_id, mutant_id, mut i) = match kind {
        "link" if args.len() >= 4 => (&args[2], &args[3], None, 4),
        "mutant" if args.len() >= 5 => (&args[2], &args[3], Some(&args[4]), 5),
        _ => return Err(usage().into()),
    };
    let mut reason = None;
    let mut signed_by = None;
    while i < args.len() {
        let (slot, flag) = match args[i].as_str() {
            "--reason" if reason.is_none() => (&mut reason, "--reason"),
            "--signed-by" if signed_by.is_none() => (&mut signed_by, "--signed-by"),
            _ => return Err(usage().into()),
        };
        let value = args
            .get(i + 1)
            .filter(|value| !value.starts_with('-'))
            .ok_or_else(usage)?;
        if value.trim().is_empty() {
            return Err(format!("{flag} must be non-empty").into());
        }
        *slot = Some(value.as_str());
        i += 2;
    }
    let reason = reason.ok_or_else(usage)?;
    let signed_by = signed_by.ok_or_else(usage)?;
    let (spec, annotations) = load(dir)?;
    let line = spec
        .lines
        .iter()
        .find(|line| line.id == *requirement)
        .ok_or_else(|| format!("unknown requirement {requirement}"))?;
    let link = annotations
        .requirements
        .get(requirement)
        .and_then(|annotation| {
            annotation
                .links
                .iter()
                .find(|link| link.test_id == *test_id)
        })
        .ok_or_else(|| format!("link not found: {requirement} {test_id}"))?;
    if !link.is_accepted() || link.revision != line.revision || link.audit.revision != line.revision
    {
        return Err("link is challenged or stale; re-audit it before signing off".into());
    }
    // Resolve from the project directory containing this spec so callers do
    // not need to run the command from a particular cwd.
    let canonical_dir = dir.canonicalize()?;
    let tests_root = canonical_dir.parent().unwrap_or(Path::new("."));
    let tests_root = if tests_root.file_name().is_some_and(|name| name == "tests")
        && tests_root
            .parent()
            .is_some_and(|parent| parent.join("Cargo.toml").is_file())
    {
        tests_root.parent().unwrap()
    } else {
        tests_root
    };
    let index = TestIndex::build(tests_root)?;
    let entry = index
        .get(test_id)
        .ok_or_else(|| format!("test not found: {test_id}"))?;
    if entry.fingerprint != link.fingerprint {
        return Err("test fingerprint is stale".into());
    }
    let file_path = dir.join("mutation.json");
    let bytes = fs::read(&file_path)?;
    let mut file =
        MutationFile::from_bytes(&bytes).map_err(|e| format!("invalid mutation file: {e}"))?;
    let record = file
        .links
        .iter()
        .find(|record| record.requirement == *requirement && record.test_id == *test_id)
        .ok_or_else(|| "mutation record not found".to_owned())?;
    let signoff = if let Some(mutant_id) = mutant_id {
        let Some(mutant) = record.mutants.iter().find(|mutant| mutant.id == *mutant_id) else {
            return Err(format!("mutant does not exist: {mutant_id}").into());
        };
        if mutant.outcome != crate::core::mutation::MutantOutcome::Survived {
            return Err("only a survived mutant can be waived".into());
        }
        let current_subject = crate::mutation::subject::subject_digest_for(entry, tests_root)
            .map_err(|error| format!("subject cannot be resolved: {}", error.0))?
            .ok_or_else(|| "subject digest unavailable".to_owned())?;
        if current_subject != record.subject_digest {
            return Err("mutation record subject digest is stale".into());
        }
        Signoff::Mutant {
            requirement: requirement.clone(),
            test_id: test_id.clone(),
            mutant_id: mutant_id.clone(),
            test_fingerprint: entry.fingerprint.clone(),
            requirement_revision: line.revision.clone(),
            subject_digest: record.subject_digest.clone(),
            reason: reason.into(),
            signed_by: signed_by.into(),
        }
    } else {
        if record.status == LinkStatus::Survived {
            return Err("cannot sign off a link with a survived mutant".into());
        }
        Signoff::Link {
            requirement: requirement.clone(),
            test_id: test_id.clone(),
            test_fingerprint: entry.fingerprint.clone(),
            requirement_revision: line.revision.clone(),
            reason: reason.into(),
            signed_by: signed_by.into(),
        }
    };
    // Preserve the existing order when this exact decision is repeated.
    if file.signoffs.contains(&signoff) {
        return Ok(());
    }
    // Replace a signoff for the same logical target so rebinding is explicit.
    file.signoffs.retain(|old| match (&signoff, old) {
        (
            Signoff::Link {
                requirement: r,
                test_id: t,
                ..
            },
            Signoff::Link {
                requirement,
                test_id,
                ..
            },
        ) => r != requirement || t != test_id,
        (
            Signoff::Mutant {
                requirement: r,
                test_id: t,
                mutant_id: m,
                ..
            },
            Signoff::Mutant {
                requirement,
                test_id,
                mutant_id,
                ..
            },
        ) => r != requirement || t != test_id || m != mutant_id,
        _ => true,
    });
    file.signoffs.push(signoff);
    write_atomic(
        &file_path,
        &file
            .to_pretty_bytes()
            .map_err(|e| format!("invalid mutation file: {e}"))?,
    )?;
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut tmp, bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    Ok(())
}
