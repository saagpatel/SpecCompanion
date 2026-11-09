use super::{
    link::{load, parse, print_audit},
    Exit,
};
use crate::{
    agents::{
        auditor,
        test_index::{SourceResolution, TestIndex},
    },
    core::annotations::{Audit, AuditVerdict},
};
use std::{collections::BTreeMap, error::Error, fs};

pub(super) fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let (dir, root, write) = parse(args, true)?;
    let (spec, annotations) = load(dir)?;
    let index = TestIndex::build(root)?;
    let path = dir.join("annotations.json");
    let mut document: Option<serde_json::Value> = if write && path.exists() {
        Some(serde_json::from_slice(&fs::read(&path)?)?)
    } else {
        None
    };
    let original = document.clone();
    let mut challenged = false;
    for (id, annotation) in &annotations.requirements {
        for (position, link) in annotation.links.iter().enumerate() {
            let line = spec.lines.iter().find(|line| line.id == *id);
            let resolved = index.resolve(link);
            let mut current = link.clone();
            current.retire_stale_override();
            if !matches!(resolved, SourceResolution::Exact(_))
                || line.is_none_or(|line| line.revision != current.revision)
            {
                current.invalidate_override();
            }
            if let Some(line) = line {
                current.revision = line.revision.clone();
            }
            let result = match (line, resolved) {
                (
                    Some(line),
                    resolution @ (SourceResolution::Exact(_)
                    | SourceResolution::Changed(_)
                    | SourceResolution::Renamed(_)),
                ) => {
                    let (entry, note) = match resolution {
                        SourceResolution::Exact(entry) => (entry, None),
                        SourceResolution::Changed(entry) => (
                            entry,
                            Some(
                                "Test source changed; this audit analyses the current fingerprint.",
                            ),
                        ),
                        SourceResolution::Renamed(entry) => (
                            entry,
                            Some("Test renamed; resolved by a unique rename_hint."),
                        ),
                        SourceResolution::Unresolved => unreachable!(),
                    };
                    current.test_id = entry.id.clone();
                    current.fingerprint = entry.fingerprint.clone();
                    current.rename_hint = Some(entry.rename_hint.clone());
                    let mut result = auditor::audit(line, &current, entry);
                    if let Some(note) = note.filter(|_| !write) {
                        result.notes.push(format!(
                            "{note} Current test: {} ({}); use audit --write to persist this binding.",
                            entry.id, entry.fingerprint
                        ));
                    }
                    result
                }
                _ => Audit {
                    extra: BTreeMap::new(),
                    revision: line.map_or_else(String::new, |line| line.revision.clone()),
                    verdict: AuditVerdict::Challenged,
                    notes: vec![if line.is_none() {
                        "Requirement is absent from the spec (orphan link).".into()
                    } else {
                        "Test id is unresolved; no unique rename_hint match exists.".into()
                    }],
                },
            };
            challenged |= result.verdict == AuditVerdict::Challenged;
            print_audit(id, &current.test_id, &result);
            if let Some(document) = &mut document {
                let extra = current.audit.extra;
                current.audit = result;
                current.audit.extra = extra;
                current.retire_stale_override();
                let saved = &mut document["requirements"][id]["links"][position];
                saved["test_id"] = current.test_id.into();
                saved["fingerprint"] = current.fingerprint.into();
                saved["rename_hint"] = serde_json::to_value(current.rename_hint)?;
                if line.is_some() {
                    saved["revision"] = current.revision.into();
                }
                saved["audit"] = serde_json::to_value(current.audit)?;
            }
        }
    }
    if document != original {
        let mut bytes = serde_json::to_vec_pretty(&document.unwrap())?;
        bytes.push(b'\n');
        fs::write(path, bytes)?;
    }
    if challenged {
        Err(Exit::Challenged.into())
    } else {
        Ok(())
    }
}
