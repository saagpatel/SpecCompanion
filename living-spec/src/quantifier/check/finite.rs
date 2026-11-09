use super::execute;
use crate::quantifier::{
    annotations::{Domain, Value},
    predicate::Budget,
    Claim, Coverage, Error, Mode, Report, Result,
};
use std::collections::{BTreeMap, BTreeSet};
pub fn check(claim: &Claim) -> Result<Report> {
    claim.validate()?;
    if !claim.quantifier.domain.is_finite() {
        return Err(Error::new("finite checking requires a finite domain"));
    }
    let n = claim
        .quantifier
        .domain
        .cardinality()
        .ok_or_else(|| Error::new("finite cardinality overflow"))?;
    // Validation above makes each index valid, without allocating the input domain.
    execute(
        claim,
        (0..n).map(|i| {
            claim
                .quantifier
                .domain
                .value_at(i)
                .expect("validated finite domain")
        }),
        Coverage {
            mode: Mode::Exhaustive,
            tested_cases: 0,
            distinct_tested: 0,
            finite_total: Some(n),
            seed: None,
            untested_remainder: String::new(),
        },
    )
}
/// Exact witnessed regions only. Product rectangles expand only when every
/// newly included Cartesian point was observed failing. Maximal rectangles may
/// overlap; an axis projection alone cannot justify a failing rectangle.
pub(crate) fn collapse(
    domain: &Domain,
    values: BTreeSet<Value>,
    budget: &mut Budget,
) -> Result<Vec<Domain>> {
    if values.is_empty() {
        return Ok(vec![]);
    }
    budget.spend(values.len() as u64)?;
    match domain {
        Domain::IntRange { .. }
        | Domain::Unbounded {
            base: crate::quantifier::annotations::Base::Int,
            ..
        } => {
            let mut ranges = Vec::new();
            let mut run: Option<(i64, i64)> = None;
            for value in values {
                let Value::Int(n) = value else {
                    return Err(Error::new("integer region type mismatch"));
                };
                match run {
                    Some((lo, hi)) if hi.checked_add(1) == Some(n) => run = Some((lo, n)),
                    Some((lo, hi)) => {
                        ranges.push(Domain::IntRange { lo, hi });
                        run = Some((n, n));
                    }
                    None => run = Some((n, n)),
                }
            }
            if let Some((lo, hi)) = run {
                ranges.push(Domain::IntRange { lo, hi });
            }
            Ok(ranges)
        }
        Domain::Product { parts } => {
            let failures = values.clone();
            let mut groups: BTreeMap<Vec<Value>, BTreeSet<Value>> = BTreeMap::new();
            for value in values {
                let Value::Tuple(mut tuple) = value else {
                    return Err(Error::new("product region type mismatch"));
                };
                let last = tuple
                    .pop()
                    .ok_or_else(|| Error::new("empty product observation"))?;
                groups.entry(tuple).or_default().insert(last);
            }
            let mut result = Vec::new();
            for (prefix, last_values) in groups {
                for last in collapse(
                    parts.last().ok_or_else(|| Error::new("empty product"))?,
                    last_values,
                    budget,
                )? {
                    let mut axes = Vec::new();
                    for (axis, value) in parts.iter().zip(&prefix) {
                        axes.extend(collapse(axis, BTreeSet::from([value.clone()]), budget)?);
                    }
                    axes.push(last);
                    result.push(Domain::Product { parts: axes });
                }
            }
            let regions = merge_product_axes(result, budget)?;
            expand_regions(parts, regions, &failures, budget)
        }
        _ => Ok(vec![Domain::Enum {
            values: values.into_iter().collect(),
        }]),
    }
}
fn expand_regions(
    original: &[Domain],
    regions: Vec<Domain>,
    failures: &BTreeSet<Value>,
    budget: &mut Budget,
) -> Result<Vec<Domain>> {
    let mut unique = BTreeMap::new();
    for region in regions {
        let Domain::Product { mut parts } = region else {
            unreachable!()
        };
        loop {
            let mut changed = false;
            for axis in 0..parts.len() {
                // Catalog product subjects currently take scalar integer axes.
                // Nested regions remain exact and are handled recursively by collapse.
                let candidates = match (&original[axis], &parts[axis]) {
                    (
                        Domain::IntRange { .. } | Domain::Unbounded { .. },
                        Domain::IntRange { lo, hi },
                    ) => lo
                        .checked_sub(1)
                        .into_iter()
                        .chain(hi.checked_add(1))
                        .map(Value::Int)
                        .filter(|v| original[axis].contains(v))
                        .collect::<Vec<_>>(),
                    (Domain::Enum { values }, current) => values
                        .iter()
                        .filter(|v| !current.contains(v))
                        .cloned()
                        .collect(),
                    _ => vec![],
                };
                for candidate in candidates {
                    let mut slab = parts.clone();
                    slab[axis] = Domain::Enum {
                        values: vec![candidate.clone()],
                    };
                    let slab = Domain::Product { parts: slab };
                    let n = slab
                        .cardinality()
                        .ok_or_else(|| Error::new("region cardinality overflow"))?;
                    budget.spend(n + 1)?;
                    let mut all_failed = true;
                    for i in 0..n {
                        if !failures.contains(&slab.value_at(i)?) {
                            all_failed = false;
                            break;
                        }
                    }
                    if all_failed {
                        match (&mut parts[axis], candidate) {
                            (Domain::IntRange { lo, hi }, Value::Int(n)) => {
                                *lo = (*lo).min(n);
                                *hi = (*hi).max(n);
                            }
                            (Domain::Enum { values }, v) => {
                                values.push(v);
                                values.sort();
                            }
                            _ => continue,
                        }
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let region = Domain::Product { parts };
        unique.insert(serde_json::to_string(&region)?, region);
    }
    Ok(unique.into_values().collect())
}
fn merge_product_axes(mut regions: Vec<Domain>, budget: &mut Budget) -> Result<Vec<Domain>> {
    // Group identical cross-sections; unite only one axis at a time. This avoids
    // an all-pairs merge and remains bounded for checkerboard failure patterns.
    loop {
        let before = regions.len();
        let axes = match regions.first() {
            Some(Domain::Product { parts }) => parts.len(),
            _ => 0,
        };
        for axis in (0..axes).rev() {
            let mut groups: BTreeMap<String, (Vec<Domain>, Vec<Domain>)> = BTreeMap::new();
            for region in regions {
                budget.spend(1)?;
                let Domain::Product { mut parts } = region else {
                    unreachable!()
                };
                let selected = parts.remove(axis);
                let key = serde_json::to_string(&parts)?;
                let group = groups.entry(key).or_insert_with(|| (parts, vec![]));
                group.1.push(selected);
            }
            regions = Vec::new();
            for (_, (other, selections)) in groups {
                for selected in union_axis(selections, budget)? {
                    let mut parts = other.clone();
                    parts.insert(axis, selected);
                    regions.push(Domain::Product { parts });
                }
            }
        }
        if regions.len() == before {
            return Ok(regions);
        }
    }
}
fn union_axis(domains: Vec<Domain>, budget: &mut Budget) -> Result<Vec<Domain>> {
    let mut ranges = Vec::new();
    let mut enums = BTreeSet::new();
    let mut rest = Vec::new();
    for domain in domains {
        budget.spend(1)?;
        match domain {
            Domain::IntRange { lo, hi } => ranges.push((lo, hi)),
            Domain::Enum { values } => enums.extend(values),
            _ => rest.push(domain),
        }
    }
    ranges.sort();
    let mut run: Option<(i64, i64)> = None;
    for (lo, hi) in ranges {
        match run {
            Some((a, b)) if i128::from(lo) <= i128::from(b) + 1 => run = Some((a, b.max(hi))),
            Some((a, b)) => {
                rest.push(Domain::IntRange { lo: a, hi: b });
                run = Some((lo, hi));
            }
            None => run = Some((lo, hi)),
        }
    }
    if let Some((lo, hi)) = run {
        rest.push(Domain::IntRange { lo, hi });
    }
    if !enums.is_empty() {
        rest.push(Domain::Enum {
            values: enums.into_iter().collect(),
        });
    }
    Ok(rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantifier::{
        annotations::{Kind, Quantifier},
        predicate::{Expr, Operand, PredicateRef, Source},
    };
    #[test]
    fn finite_exhaustive_maximal_product_regions_do_not_fill_holes() {
        let domain = Domain::Product {
            parts: vec![
                Domain::IntRange { lo: 0, hi: 1 },
                Domain::IntRange { lo: 0, hi: 1 },
            ],
        };
        let failures = BTreeSet::from([
            Value::Tuple(vec![Value::Int(0), Value::Int(0)]),
            Value::Tuple(vec![Value::Int(0), Value::Int(1)]),
            Value::Tuple(vec![Value::Int(1), Value::Int(0)]),
        ]);
        let regions = collapse(&domain, failures.clone(), &mut Budget::new(1000)).unwrap();
        assert_eq!(regions.len(), 2);
        let mut represented = BTreeSet::new();
        for region in &regions {
            assert_eq!(region.cardinality(), Some(2));
            for i in 0..region.cardinality().unwrap() {
                represented.insert(region.value_at(i).unwrap());
            }
        }
        assert_eq!(represented, failures);
    }
    #[test]
    fn finite_exhaustive_over_work_cap_is_an_error_without_sampling() {
        let claim = Claim {
            id: "W-001".into(),
            text: "Synthetic work bound".into(),
            expected: None,
            quantifier: Quantifier {
                kind: Kind::FiniteUniversal,
                domain: Domain::Enum {
                    values: (0..400)
                        .map(|n| Value::String(format!("{n:03}{}", "x".repeat(253))))
                        .collect(),
                },
                subject: "utf8_roundtrip".into(),
                sampling: None,
                predicate: PredicateRef {
                    version: 1,
                    expression: Expr::In {
                        value: Operand::Field {
                            source: Source::Output,
                            path: vec![],
                        },
                        set: (0..64)
                            .map(|n| Value::String(format!("{n:03}{}", "y".repeat(253))))
                            .collect(),
                    },
                },
            },
        };
        assert!(check(&claim).unwrap_err().to_string().contains("work cap"));
    }
}
