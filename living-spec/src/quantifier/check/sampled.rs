use super::execute;
use crate::quantifier::{
    annotations::{Base, Domain, Value},
    Claim, Coverage, Error, Mode, Report, Result,
};
use std::collections::BTreeSet;
/// SplitMix64, fixed constants and wrapping arithmetic for reproducible draws.
pub struct SplitMix64(u64);
impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
}
pub fn check(claim: &Claim) -> Result<Report> {
    claim.validate()?;
    let sampling = claim
        .quantifier
        .sampling
        .as_ref()
        .ok_or_else(|| Error::new("sampled checking requires seed and samples"))?;
    let mut values = edges(&claim.quantifier.domain)?;
    let mut seen = BTreeSet::new();
    values.retain(|v| seen.insert(v.clone()));
    if values.len() > sampling.samples as usize {
        return Err(Error::new(
            "sample count is too small to include required edge values",
        ));
    }
    let mut rng = SplitMix64::new(sampling.seed);
    while values.len() < sampling.samples as usize {
        values.push(draw(&claim.quantifier.domain, &mut rng)?);
    }
    execute(
        claim,
        values.into_iter(),
        Coverage {
            mode: Mode::Sampled,
            tested_cases: 0,
            distinct_tested: 0,
            finite_total: claim.quantifier.domain.cardinality(),
            seed: Some(sampling.seed),
            untested_remainder: String::new(),
        },
    )
}
fn edges(domain: &Domain) -> Result<Vec<Value>> {
    Ok(match domain {
        Domain::IntRange { lo, hi } => {
            let mut v = vec![Value::Int(*lo), Value::Int(*hi)];
            if *lo <= 0 && *hi >= 0 {
                v.push(Value::Int(0));
            }
            v
        }
        Domain::Enum { values } => {
            let mut sorted = values.clone();
            sorted.sort();
            let mut v = vec![sorted[0].clone(), sorted[sorted.len() - 1].clone()];
            v.extend(
                values
                    .iter()
                    .filter(|v| {
                        matches!(v, Value::Int(0))
                            || matches!(v,Value::String(s) if s.is_empty())
                            || matches!(v,Value::VecInt(s) if s.is_empty())
                            || matches!(v,Value::Bytes(s) if s.is_empty())
                    })
                    .cloned(),
            );
            v
        }
        Domain::Product { parts } => {
            let axes: Vec<Vec<Value>> = parts.iter().map(edges).collect::<Result<_>>()?;
            let baseline: Vec<Value> = axes.iter().map(|a| a[0].clone()).collect();
            let mut values = vec![Value::Tuple(baseline.clone())];
            for (i, axis) in axes.iter().enumerate() {
                for value in axis {
                    let mut tuple = baseline.clone();
                    tuple[i] = value.clone();
                    values.push(Value::Tuple(tuple));
                }
            }
            values
        }
        Domain::Unbounded { base, .. } => match base {
            Base::Int => vec![Value::Int(i64::MIN), Value::Int(i64::MAX), Value::Int(0)],
            Base::String => vec![
                Value::String(String::new()),
                Value::String("a".into()),
                Value::String("é".into()),
            ],
            Base::Bytes => vec![
                Value::Bytes(vec![]),
                Value::Bytes(vec![0]),
                Value::Bytes(vec![255]),
            ],
            Base::VecInt => vec![
                Value::VecInt(vec![]),
                Value::VecInt(vec![0]),
                Value::VecInt(vec![i64::MAX, i64::MIN, i64::MIN]),
            ],
        },
    })
}
fn draw(domain: &Domain, rng: &mut SplitMix64) -> Result<Value> {
    Ok(match domain {
        Domain::IntRange { lo, hi } => Value::Int(
            (i128::from(*lo) + i128::from(rng.next_u64()) % (i128::from(*hi) - i128::from(*lo) + 1))
                as i64,
        ),
        Domain::Enum { values } => values[rng.next_u64() as usize % values.len()].clone(),
        Domain::Product { parts } => {
            Value::Tuple(parts.iter().map(|p| draw(p, rng)).collect::<Result<_>>()?)
        }
        Domain::Unbounded { base, size_hint } => match base {
            Base::Int => Value::Int(rng.next_u64() as i64),
            Base::String => {
                let len = rng.next_u64() % (*size_hint as u64 + 1);
                let mut s = String::new();
                for _ in 0..len {
                    let code = (rng.next_u64() % 0x110000) as u32;
                    s.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                }
                Value::String(s)
            }
            Base::Bytes => {
                let len = rng.next_u64() % (*size_hint as u64 + 1);
                Value::Bytes((0..len).map(|_| rng.next_u64() as u8).collect())
            }
            Base::VecInt => {
                let len = rng.next_u64() % (*size_hint as u64 + 1);
                Value::VecInt((0..len).map(|_| rng.next_u64() as i64).collect())
            }
        },
    })
}
