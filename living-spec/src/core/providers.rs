//! Provider contracts shared by the independent scoring layers.
use super::{
    annotations::{Annotations, Link},
    badge::LineStatus,
    spec::{RequirementLine, Spec},
    test_identity::TestOutcome,
};
use std::{collections::BTreeSet, error::Error};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub trait BoundaryGapProvider {
    fn gaps(
        &self,
        spec: &Spec,
        annotations: &Annotations,
        outcomes: &[TestOutcome],
    ) -> Result<BTreeSet<String>>;
    fn reports(
        &self,
        _spec: &Spec,
        _annotations: &Annotations,
        _outcomes: &[TestOutcome],
    ) -> Result<Vec<super::views::BoundaryReport>> {
        Ok(Vec::new())
    }
}
/// No boundary execution evidence is available until a provider supplies it.
pub struct NoBoundaryEvidence;
impl BoundaryGapProvider for NoBoundaryEvidence {
    fn gaps(
        &self,
        _spec: &Spec,
        annotations: &Annotations,
        _outcomes: &[TestOutcome],
    ) -> Result<BTreeSet<String>> {
        Ok(annotations
            .requirements
            .iter()
            .filter(|(_, a)| !a.bounds.is_empty())
            .map(|(id, _)| id.clone())
            .collect())
    }
}
pub trait QualifiedProvider {
    fn qualify(
        &self,
        spec: &Spec,
        annotations: &Annotations,
        statuses: &mut [LineStatus],
    ) -> Result<()>;
}
pub struct NoQualification;
impl QualifiedProvider for NoQualification {
    fn qualify(
        &self,
        _spec: &Spec,
        _annotations: &Annotations,
        _statuses: &mut [LineStatus],
    ) -> Result<()> {
        Ok(())
    }
}

/// Refresh a resolved link using current source evidence supplied by the pipeline.
pub trait Reauditor {
    fn reaudit(&self, line: &RequirementLine, link: &mut Link, changed_source: bool);
}
