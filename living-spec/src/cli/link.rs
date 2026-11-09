use super::Exit;
use crate::{
    agents::{auditor, linker, test_index::TestIndex},
    core::{
        annotations::{Annotations, Audit, AuditVerdict, Link},
        spec::Spec,
    },
};
use std::{collections::BTreeMap, error::Error, fs, path::Path};

pub(super) fn parse(
    args: &[String],
    allow_write: bool,
) -> Result<(&Path, &Path, bool), Box<dyn Error>> {
    match args {
        [dir, flag, root] if flag == "--tests" && !dir.starts_with('-') && !root.starts_with('-') => Ok((Path::new(dir), Path::new(root), false)),
        [dir, flag, root, write] if allow_write && flag == "--tests" && write == "--write" && !dir.starts_with('-') && !root.starts_with('-') => Ok((Path::new(dir), Path::new(root), true)),
        _ => Err(Exit::Usage("usage: living-spec link <spec-dir> --tests <source-root> [--write]; living-spec audit <spec-dir> --tests <source-root>".into()).into()),
    }
}
pub(super) fn load(dir: &Path) -> Result<(Spec, Annotations), Box<dyn Error>> {
    let spec = Spec::parse(&fs::read_to_string(dir.join("spec.md"))?)?;
    let annotations = if dir.join("annotations.json").exists() {
        Annotations::load(dir.join("annotations.json"))?
    } else {
        Annotations {
            schema: "living-spec.annotations/v1".into(),
            requirements: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    };
    if annotations.schema != "living-spec.annotations/v1" {
        return Err("unsupported annotation schema".into());
    }
    Ok((spec, annotations))
}
pub(super) fn print_audit(requirement: &str, test_id: &str, audit: &Audit) {
    let verdict = match audit.verdict {
        AuditVerdict::Accepted => "accepted",
        AuditVerdict::Challenged => "challenged",
    };
    println!(
        "{requirement} -> {test_id}: {verdict} (revision {})",
        audit.revision
    );
    for note in &audit.notes {
        println!("  {note}");
    }
}
pub(super) fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let (dir, root, write) = parse(args, true)?;
    let (spec, mut annotations) = load(dir)?;
    let index = TestIndex::build(root)?;
    for id in annotations.orphans(&spec) {
        println!("orphan annotation: {id}");
    }
    for proposal in linker::propose(&spec, &index) {
        let line = spec
            .lines
            .iter()
            .find(|line| line.id == proposal.requirement)
            .unwrap();
        let entry = index.get(&proposal.test_id).unwrap();
        let mut link = Link {
            extra: BTreeMap::new(),
            revision: proposal.revision.clone(),
            rename_hint: Some(proposal.rename_hint),
            test_id: proposal.test_id,
            fingerprint: proposal.fingerprint,
            explanation: proposal.explanation,
            proposed_by: "linker".into(),
            audit: Audit {
                extra: BTreeMap::new(),
                revision: proposal.revision,
                verdict: AuditVerdict::Challenged,
                notes: Vec::new(),
            },
        };
        link.audit = auditor::audit(line, &link, entry);
        println!("score {}: {}", proposal.score, link.explanation);
        print_audit(&line.id, &link.test_id, &link.audit);
        if write {
            let links = &mut annotations
                .requirements
                .entry(line.id.clone())
                .or_default()
                .links;
            // Human links, including authored metadata, are never replaced.
            if links
                .iter()
                .any(|old| old.test_id == link.test_id && old.proposed_by == "human")
            {
                continue;
            }
            if let Some(old) = links
                .iter_mut()
                .find(|old| old.test_id == link.test_id && old.proposed_by == "linker")
            {
                link.extra = old.extra.clone();
                link.audit.extra = old.audit.extra.clone();
                *old = link;
            } else {
                links.push(link);
            }
        }
    }
    if write {
        annotations.save(dir.join("annotations.json"))?;
    }
    Ok(())
}
