use super::coverage::CaseLink;
use super::{gen::generate, model::*};
use crate::core::{annotations::RequirementAnnotations, spec::RequirementLine};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Provenance belongs to the whole bounds list, including an empty override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "origin", rename_all = "snake_case", deny_unknown_fields)]
pub enum BoundsSource {
    Authored,
    Inferred { revision: String },
}

pub struct ResolvedBounds {
    pub bounds: Vec<Bound>,
    pub links: Vec<CaseLink>,
    pub source: BoundsSource,
    /// Cached inferred cases and receipts predate this requirement or its bounds.
    pub stale: bool,
}

/// Shared CLI/provider policy. Inferred values are never an authored override.
pub fn resolve_requirement(
    line: &RequirementLine,
    annotation: Option<&RequirementAnnotations>,
) -> Result<ResolvedBounds, Box<dyn std::error::Error>> {
    let empty = RequirementAnnotations::default();
    let annotation = annotation.unwrap_or(&empty);
    let present = annotation.bounds_present || !annotation.bounds.is_empty();
    let stored: Vec<Bound> = annotation
        .bounds
        .iter()
        .cloned()
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()?;
    let source: Option<BoundsSource> = annotation
        .extra
        .get("bounds_source")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?;
    if source.is_some() && !present {
        return Err(BoundaryError::InvalidBound("bounds_source requires bounds".into()).into());
    }
    // Old generation did not record origin. Its output cannot safely be treated
    // as an override; ask for an explicit origin instead of guessing its intent.
    if source.is_none() && !stored.is_empty() && annotation.extra.contains_key("boundary_cases") {
        return Err(BoundaryError::NeedsAnnotation(
            "legacy generated bounds need an explicit bounds_source".into(),
        )
        .into());
    }
    let authored = present && matches!(source, None | Some(BoundsSource::Authored));
    let bounds = resolve_bounds(&line.text, authored.then_some(stored.as_slice()))?;
    let links: Vec<CaseLink> = annotation
        .extra
        .get("boundary_links")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    let stale = match &source {
        Some(BoundsSource::Inferred { revision }) => {
            if revision.is_empty() {
                return Err(BoundaryError::InvalidBound("empty inferred revision".into()).into());
            }
            revision != &line.revision || bounds != stored
        }
        _ => false,
    };
    Ok(ResolvedBounds {
        bounds,
        stale,
        // A revision change can alter acceptance without changing any value.
        // Invalidate all inferred links rather than comparing numbers alone.
        links: if stale { Vec::new() } else { links },
        source: if authored {
            BoundsSource::Authored
        } else {
            BoundsSource::Inferred {
                revision: line.revision.clone(),
            }
        },
    })
}

/// General scoring also visits requirements outside the literal bounds grammar.
/// Explicit boundary sections and boundary-language cues require resolution;
/// unsupported clauses with such cues must fail rather than disappear.
pub fn requires_resolution(
    line: &RequirementLine,
    annotation: Option<&RequirementAnnotations>,
) -> bool {
    annotation.is_some_and(|a| a.bounds_present || !a.bounds.is_empty()
        || ["bounds_source", "boundary_links", "boundary_cases"].iter().any(|key| a.extra.contains_key(*key)))
        || Regex::new(r"(?i)\b(at most|no more than|up to|at least|more than|fewer than|less than|under|over|exactly|within|between|one of|only|or fewer|fast|normally|percentile)\b")
            .unwrap().is_match(&line.text)
}

/// Authored entries win, including an explicitly authored empty list.
pub fn resolve_bounds(text: &str, explicit: Option<&[Bound]>) -> Result<Vec<Bound>, BoundaryError> {
    if let Some(bounds) = explicit {
        for bound in bounds {
            generate(bound)?;
        }
        return Ok(bounds.to_vec());
    }
    parse_bounds(text)
}

/// Deliberately closed grammar: a subject, `is`, and one literal clause.
/// Decimals require an authored step; ambiguous prose is never partially parsed.
pub fn parse_bounds(text: &str) -> Result<Vec<Bound>, BoundaryError> {
    let unknown = || BoundaryError::NeedsAnnotation(text.into());
    let text = text.trim().strip_suffix('.').unwrap_or(text.trim());
    let (quantity, clause) = text.split_once(" is ").ok_or_else(unknown)?;
    let subject = Regex::new(r"^[A-Za-z][A-Za-z _-]*$").unwrap();
    let unsafe_words =
        Regex::new(r"(?i)\b(fast|normally|not|never|percentile|unless|except|and|or)\b").unwrap();
    if !subject.is_match(quantity) || unsafe_words.is_match(quantity) {
        return Err(unknown());
    }
    if let Some(values) = clause
        .strip_prefix("one of ")
        .or_else(|| clause.strip_prefix("only "))
    {
        let values = values.replace(" or ", ", ");
        let token = Regex::new(r"^[A-Za-z][A-Za-z0-9_-]*$").unwrap();
        let allowed: Vec<String> = values.split(',').map(|s| s.trim().to_owned()).collect();
        if allowed.len() < 2 || allowed.iter().any(|s| !token.is_match(s)) {
            return Err(unknown());
        }
        let bound = Bound {
            quantity: quantity.into(),
            kind: BoundKind::Domain {
                allowed,
                closed: true,
            },
        };
        generate(&bound)?;
        return Ok(vec![bound]);
    }
    let range =
        Regex::new(r"^between (-?[0-9]+) and (-?[0-9]+) ([A-Za-z%]+) (inclusive|exclusive)$")
            .unwrap();
    let (value, unit, op): (Decimal, Unit, Op) = if let Some(c) = range.captures(clause) {
        let lo: Decimal = c[1].parse()?;
        let hi: Decimal = c[2].parse()?;
        (
            lo.clone(),
            c[3].parse()?,
            Op::Range {
                lo,
                hi,
                inclusive_lo: &c[4] == "inclusive",
                inclusive_hi: &c[4] == "inclusive",
            },
        )
    } else {
        let numeric = Regex::new(r"^(at most|no more than|up to|at least|more than|fewer than|less than|under|over|exactly|within) (-?[0-9]+) ([A-Za-z%]+)$").unwrap();
        let fewer = Regex::new(r"^(-?[0-9]+) or fewer ([A-Za-z%]+)$").unwrap();
        if let Some(c) = numeric.captures(clause) {
            let op = match &c[1] {
                "at most" | "no more than" | "up to" | "within" => Op::Le,
                "at least" => Op::Ge,
                "more than" | "over" => Op::Gt,
                "fewer than" | "less than" | "under" => Op::Lt,
                "exactly" => Op::Eq,
                _ => unreachable!(),
            };
            (c[2].parse()?, c[3].parse()?, op)
        } else if let Some(c) = fewer.captures(clause) {
            (c[1].parse()?, c[2].parse()?, Op::Le)
        } else {
            return Err(unknown());
        }
    };
    let integer = !matches!(unit.base(), Unit::Ms | Unit::B | Unit::Percent);
    let bound = Bound {
        quantity: quantity.into(),
        kind: BoundKind::Numeric {
            value,
            unit,
            op,
            step: Step {
                value: "1".parse()?,
                unit: unit.base(),
            },
            integer,
        },
    };
    generate(&bound)?;
    Ok(vec![bound])
}
