//! Mutation engine building blocks.
pub mod generate;
pub mod runner;
pub mod subject;

use crate::{
    agents::test_index::TestEntry,
    core::mutation::{
        subject_digest, LinkRecord, LinkStatus, Mutant, MutantOutcome, Target, TargetKind,
    },
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone)]
pub struct Options {
    pub max_mutants: usize,
    pub timeout: Duration,
    pub cargo: PathBuf,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_mutants: 8,
            timeout: Duration::from_secs(180),
            cargo: "cargo".into(),
        }
    }
}
#[derive(Default)]
struct RecordState {
    baseline_passes: u32,
    mutants: Vec<Mutant>,
    infra_failure: Option<String>,
}
impl RecordState {
    fn failure(&mut self, reason: &str) {
        self.infra_failure = Some(reason.into());
    }
}

pub fn run_link(
    options: &Options,
    entry: &TestEntry,
    requirement: &str,
    requirement_revision: &str,
    tests_root: &Path,
) -> LinkRecord {
    let mut record = LinkRecord {
        requirement: requirement.into(),
        test_id: entry.id.clone(),
        test_fingerprint: entry.fingerprint.clone(),
        requirement_revision: requirement_revision.into(),
        subject_digest: subject_digest(&[]),
        target: Target {
            kind: TargetKind::Lib,
            name: "unknown".into(),
        },
        baseline_passes: 0,
        status: LinkStatus::Unsupported,
        reason: None,
        infra_failure: None,
        mutants: vec![],
    };
    let subject = match subject::resolve_subject(entry, tests_root) {
        Ok(subject) => subject,
        Err(reason) => {
            record.reason = Some(reason.0);
            return record;
        }
    };
    record.target = subject.target.clone();
    record.subject_digest = subject_digest(
        &subject
            .functions
            .iter()
            .map(|f| {
                (
                    f.file.as_str(),
                    f.name.as_str(),
                    f.source[f.span.clone()].as_bytes(),
                )
            })
            .collect::<Vec<_>>(),
    );
    let mut state = RecordState::default();
    match runner::Runner::new(&subject) {
        Ok(runner) => runner.execute(options, &subject, &entry.id, &mut state),
        Err(_) => state.failure("crate copy failure"),
    }
    record.baseline_passes = state.baseline_passes;
    record.mutants = state.mutants;
    record.infra_failure = state.infra_failure;
    if let Some(reason) = &record.infra_failure {
        record.status = LinkStatus::Error;
        record.reason = Some(reason.clone());
    } else if record
        .mutants
        .iter()
        .any(|m| m.outcome == MutantOutcome::Survived)
    {
        record.status = LinkStatus::Survived;
    } else if record
        .mutants
        .iter()
        .any(|m| m.outcome == MutantOutcome::Timeout)
    {
        record.status = LinkStatus::Error;
        record.reason = Some("timeout".into());
    } else if record
        .mutants
        .iter()
        .any(|m| m.outcome == MutantOutcome::Killed)
    {
        record.status = LinkStatus::Checked;
    } else {
        record.reason = Some("no viable mutants".into());
    }
    record
}
