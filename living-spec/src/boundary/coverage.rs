pub use crate::core::boundary::coverage::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn boundary_gaps(maps: &[CoverageMap]) -> BTreeSet<String> {
    maps.iter()
        .filter(|map| {
            map.bounds.iter().any(|bound| {
                bound
                    .cases
                    .iter()
                    .any(|case| case.status != CaseStatus::TestedPass)
            })
        })
        .map(|map| map.requirement.clone())
        .collect()
}

pub fn gaps_for_annotations(
    annotations: &BTreeMap<String, crate::core::annotations::RequirementAnnotations>,
    outcomes: &[crate::core::test_identity::TestOutcome],
) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let maps = annotations
        .iter()
        .map(|(id, annotation)| {
            // This compatibility API has no current spec/revision. It must not
            // accept cached inference; use BoundaryEvidence for current scoring.
            let source: Option<super::parser::BoundsSource> = annotation
                .extra
                .get("bounds_source")
                .cloned()
                .map(serde_json::from_value)
                .transpose()?;
            if matches!(source, Some(super::parser::BoundsSource::Inferred { .. }))
                || source.is_none()
                    && annotation.extra.contains_key("boundary_cases")
                    && !annotation.bounds.is_empty()
            {
                return Err("inferred/ambiguous bounds require current spec resolution".into());
            }
            let bounds: Vec<crate::core::boundary::Bound> = annotation
                .bounds
                .iter()
                .cloned()
                .map(serde_json::from_value)
                .collect::<Result<_, _>>()?;
            let links: Vec<CaseLink> = annotation
                .extra
                .get("boundary_links")
                .cloned()
                .map(serde_json::from_value)
                .transpose()?
                .unwrap_or_default();
            Ok::<_, Box<dyn std::error::Error>>(coverage(id, &bounds, &links, outcomes)?)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(boundary_gaps(&maps))
}

pub fn reports_for_annotations(
    spec: &crate::core::spec::Spec,
    annotations: &BTreeMap<String, crate::core::annotations::RequirementAnnotations>,
    outcomes: &[crate::core::test_identity::TestOutcome],
) -> Result<Vec<CoverageMap>, Box<dyn std::error::Error>> {
    spec.lines
        .iter()
        .filter(|line| super::parser::requires_resolution(line, annotations.get(&line.id)))
        .map(|line| {
            let resolved = super::parser::resolve_requirement(line, annotations.get(&line.id))?;
            Ok(coverage(
                &line.id,
                &resolved.bounds,
                &resolved.links,
                outcomes,
            )?)
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()
        .map(|maps| {
            maps.into_iter()
                .filter(|map| !map.bounds.is_empty())
                .collect()
        })
}

/// Boundary evidence supplied by the pipeline to the runner.
pub struct BoundaryEvidence;
impl crate::core::providers::BoundaryGapProvider for BoundaryEvidence {
    fn gaps(
        &self,
        spec: &crate::core::spec::Spec,
        annotations: &crate::core::annotations::Annotations,
        outcomes: &[crate::core::test_identity::TestOutcome],
    ) -> crate::core::providers::Result<BTreeSet<String>> {
        Ok(boundary_gaps(&reports_for_annotations(
            spec,
            &annotations.requirements,
            outcomes,
        )?))
    }
    fn reports(
        &self,
        spec: &crate::core::spec::Spec,
        annotations: &crate::core::annotations::Annotations,
        outcomes: &[crate::core::test_identity::TestOutcome],
    ) -> crate::core::providers::Result<Vec<crate::core::views::BoundaryReport>> {
        reports_for_annotations(spec, &annotations.requirements, outcomes)
    }
}
