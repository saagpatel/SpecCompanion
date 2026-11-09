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
use std::{collections::BTreeMap, error::Error};

pub(super) fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let (dir, root, _) = parse(args, false)?;
    let (spec, annotations) = load(dir)?;
    let index = TestIndex::build(root)?;
    let mut challenged = false;
    for (id, annotation) in &annotations.requirements {
        for link in &annotation.links {
            let line = spec.lines.iter().find(|line| line.id == *id);
            let resolved = index.resolve(link);
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
                    // Re-audit the current source binding. This command reports only; link --write
                    // is the explicit persistence operation, including refreshed identity metadata.
                    let mut current = link.clone();
                    current.test_id = entry.id.clone();
                    current.fingerprint = entry.fingerprint.clone();
                    current.rename_hint = Some(entry.rename_hint.clone());
                    let mut result = auditor::audit(line, &current, entry);
                    if let Some(note) = note {
                        result.notes.push(format!(
                            "{note} Current test: {} ({}); use link --write to persist proposals.",
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
            print_audit(id, &link.test_id, &result);
        }
    }
    if challenged {
        Err(Exit::Challenged.into())
    } else {
        Ok(())
    }
}
