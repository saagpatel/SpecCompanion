use super::model::*;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expected {
    Accepted,
    Rejected,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Example {
    pub position: String,
    pub value: String,
    pub unit: Option<Unit>,
    pub expected: Expected,
}

pub fn generate(bound: &Bound) -> Result<Vec<Example>, BoundaryError> {
    if bound.quantity.trim().is_empty() {
        return Err(BoundaryError::InvalidBound("empty quantity".into()));
    }
    match &bound.kind {
        BoundKind::Domain { allowed, closed } => {
            if !closed {
                return Err(BoundaryError::NeedsAnnotation(
                    "an open domain does not define rejection of unlisted values".into(),
                ));
            }
            let unique: std::collections::BTreeSet<_> = allowed.iter().collect();
            if allowed.is_empty()
                || unique.len() != allowed.len()
                || allowed.iter().any(|v| v.trim().is_empty())
            {
                return Err(BoundaryError::InvalidBound("empty/duplicate domain".into()));
            }
            let mut cases: Vec<_> = allowed
                .iter()
                .enumerate()
                .map(|(i, v)| Example {
                    position: format!("allowed-{i}"),
                    value: v.clone(),
                    unit: None,
                    expected: Expected::Accepted,
                })
                .collect();
            let mut variant = format!("{} ", allowed[0]);
            while allowed.contains(&variant) {
                variant.push(' ');
            }
            let mut unlisted = "__unlisted__".to_owned();
            while allowed.contains(&unlisted) {
                unlisted.push('_');
            }
            for (position, value) in [("invalid-variant", variant), ("invalid-unlisted", unlisted)]
            {
                cases.push(Example {
                    position: position.into(),
                    value,
                    unit: None,
                    expected: if *closed {
                        Expected::Rejected
                    } else {
                        Expected::Accepted
                    },
                });
            }
            Ok(cases)
        }
        BoundKind::Numeric {
            value,
            unit,
            op,
            step,
            integer,
        } => {
            if unit.base() != step.unit.base() {
                return Err(BoundaryError::InvalidBound("step dimension differs".into()));
            }
            let delta = step.unit.normalize(&step.value)?;
            let center = unit.normalize(value)?;
            if delta <= 0
                || (*integer
                    && (value.scaled()? % 1_000_000_000 != 0
                        || delta != unit.factor() * 1_000_000_000))
            {
                return Err(BoundaryError::InvalidBound(
                    "invalid step/integer value".into(),
                ));
            }
            let edges = if let Op::Range { lo, hi, .. } = op {
                let low = unit.normalize(lo)?;
                let high = unit.normalize(hi)?;
                if low >= high {
                    return Err(BoundaryError::InvalidBound(
                        "range is reversed/empty".into(),
                    ));
                }
                if *integer
                    && (lo.scaled()? % 1_000_000_000 != 0 || hi.scaled()? % 1_000_000_000 != 0)
                {
                    return Err(BoundaryError::InvalidBound(
                        "fractional integer range".into(),
                    ));
                }
                vec![("edge-lo-", low), ("edge-hi-", high)]
            } else {
                vec![("", center)]
            };
            let mut cases = Vec::new();
            for (prefix, edge) in edges {
                for (position, offset) in [("below", -delta), ("at", 0), ("above", delta)] {
                    let n = edge.checked_add(offset).ok_or(BoundaryError::Overflow)?;
                    let accepted = match op {
                        Op::Lt => n < center,
                        Op::Le => n <= center,
                        Op::Gt => n > center,
                        Op::Ge => n >= center,
                        Op::Eq => n == center,
                        Op::Range {
                            lo,
                            hi,
                            inclusive_lo,
                            inclusive_hi,
                        } => {
                            let lo = unit.normalize(lo)?;
                            let hi = unit.normalize(hi)?;
                            (n > lo || (*inclusive_lo && n == lo))
                                && (n < hi || (*inclusive_hi && n == hi))
                        }
                    };
                    cases.push(Example {
                        position: format!("{prefix}{position}"),
                        value: Decimal::from_scaled(n).to_string(),
                        unit: Some(unit.base()),
                        expected: if accepted {
                            Expected::Accepted
                        } else {
                            Expected::Rejected
                        },
                    });
                }
            }
            Ok(cases)
        }
    }
}
