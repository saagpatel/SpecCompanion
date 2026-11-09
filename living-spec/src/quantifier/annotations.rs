//! Strict typed view over the shared annotation section; core JSON is untouched.
use super::{catalog, Error, Result};
pub use crate::core::quantifier::{Base, Domain, Kind, Quantifier, Sampling, Value};
use std::collections::BTreeSet;
pub const FINITE_CAP: u64 = 1_000_000;
pub const SAMPLE_CAP: u32 = 10_000;
pub const WORK_CAP: u64 = 10_000_000;
impl Value {
    pub fn validate(&self, depth: usize) -> Result<()> {
        if depth > 8 {
            return Err(Error::new("value depth exceeds 8"));
        }
        match self {
            Self::String(s) if s.chars().count() > 256 => {
                Err(Error::new("string exceeds 256 characters"))
            }
            Self::Bytes(v) if v.len() > 64 => Err(Error::new("bytes exceed 64 elements")),
            Self::VecInt(v) if v.len() > 64 => Err(Error::new("vector exceeds 64 elements")),
            Self::Tuple(v) => {
                if v.is_empty() || v.len() > 8 {
                    return Err(Error::new("tuple needs 1..8 parts"));
                }
                for value in v {
                    value.validate(depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub fn cost(&self) -> u64 {
        1 + match self {
            Self::String(v) => v.len() as u64,
            Self::Bytes(v) => v.len() as u64,
            Self::VecInt(v) => v.len() as u64,
            Self::Tuple(v) => v.iter().map(Self::cost).sum(),
            _ => 0,
        }
    }
}
impl Domain {
    pub fn cardinality(&self) -> Option<u64> {
        match self {
            Self::IntRange { lo, hi } => u64::try_from(i128::from(*hi) - i128::from(*lo) + 1).ok(),
            Self::Enum { values } => Some(values.len() as u64),
            Self::Product { parts } => parts
                .iter()
                .try_fold(1u64, |n, p| n.checked_mul(p.cardinality()?)),
            Self::Unbounded { .. } => None,
        }
    }
    pub fn is_finite(&self) -> bool {
        match self {
            Self::Unbounded { .. } => false,
            Self::Product { parts } => parts.iter().all(Self::is_finite),
            _ => true,
        }
    }
    pub fn representative(&self) -> Result<Value> {
        match self {
            Self::Product { parts } => Ok(Value::Tuple(
                parts
                    .iter()
                    .map(Self::representative)
                    .collect::<Result<_>>()?,
            )),
            Self::Unbounded { base, .. } => Ok(match base {
                Base::Int => Value::Int(0),
                Base::String => Value::String(String::new()),
                Base::Bytes => Value::Bytes(vec![]),
                Base::VecInt => Value::VecInt(vec![]),
            }),
            _ => self.value_at(0),
        }
    }
    pub fn validate(&self, cap: u64) -> Result<()> {
        self.validate_at(cap, 0)
    }
    fn validate_at(&self, cap: u64, depth: usize) -> Result<()> {
        if depth > 8 {
            return Err(Error::new("domain depth exceeds 8"));
        }
        match self {
            Self::IntRange { lo, hi } if lo > hi => {
                return Err(Error::new("empty or invalid integer domain"))
            }
            Self::Enum { values } => {
                if values.is_empty() {
                    return Err(Error::new("empty enum domain"));
                }
                if values.iter().collect::<BTreeSet<_>>().len() != values.len() {
                    return Err(Error::new("duplicate enum values"));
                }
                for value in values {
                    value.validate(0)?;
                }
            }
            Self::Product { parts } => {
                if parts.is_empty() || parts.len() > 8 {
                    return Err(Error::new("product needs 1..8 axes"));
                }
                for part in parts {
                    part.validate_at(cap, depth + 1)?;
                }
            }
            Self::Unbounded { size_hint, .. } if !(1..=64).contains(size_hint) => {
                return Err(Error::new("size_hint must be 1..64"))
            }
            _ => {}
        }
        if self.is_finite() && self.cardinality().is_none_or(|n| n > cap) {
            return Err(Error::new(
                "declared finite domain exceeds finite cap; refusing sampling fallback",
            ));
        }
        Ok(())
    }
    pub fn value_at(&self, index: u64) -> Result<Value> {
        let count = self
            .cardinality()
            .ok_or_else(|| Error::new("cannot enumerate unbounded domain"))?;
        if index >= count {
            return Err(Error::new("domain index out of range"));
        }
        match self {
            Self::IntRange { lo, .. } => {
                Ok(Value::Int((i128::from(*lo) + i128::from(index)) as i64))
            }
            Self::Enum { values } => Ok(values[index as usize].clone()),
            Self::Product { parts } => {
                let mut remaining = index;
                let mut values = Vec::new();
                for part in parts.iter().rev() {
                    let n = part
                        .cardinality()
                        .ok_or_else(|| Error::new("unbounded axis"))?;
                    values.push(part.value_at(remaining % n)?);
                    remaining /= n;
                }
                values.reverse();
                Ok(Value::Tuple(values))
            }
            Self::Unbounded { .. } => Err(Error::new("cannot enumerate unbounded domain")),
        }
    }
    pub fn contains(&self, value: &Value) -> bool {
        match (self, value) {
            (Self::IntRange { lo, hi }, Value::Int(v)) => v >= lo && v <= hi,
            (Self::Enum { values }, v) => values.contains(v),
            (Self::Product { parts }, Value::Tuple(v)) => {
                parts.len() == v.len() && parts.iter().zip(v).all(|(p, v)| p.contains(v))
            }
            (
                Self::Unbounded {
                    base: Base::Int, ..
                },
                Value::Int(_),
            )
            | (
                Self::Unbounded {
                    base: Base::String, ..
                },
                Value::String(_),
            )
            | (
                Self::Unbounded {
                    base: Base::Bytes, ..
                },
                Value::Bytes(_),
            )
            | (
                Self::Unbounded {
                    base: Base::VecInt, ..
                },
                Value::VecInt(_),
            ) => true,
            _ => false,
        }
    }
}
impl Quantifier {
    pub fn from_section(value: &serde_json::Value) -> Result<Self> {
        let result: Self = serde_json::from_value(value.clone())?;
        result.validate(FINITE_CAP)?;
        Ok(result)
    }
    pub fn validate(&self, finite_cap: u64) -> Result<()> {
        self.domain.validate(finite_cap)?;
        if self.kind == Kind::FiniteUniversal && !self.domain.is_finite() {
            return Err(Error::new(
                "finite-universal requires a finite domain; use infinite-universal",
            ));
        }
        if self.kind == Kind::InfiniteUniversal && self.domain.is_finite() {
            return Err(Error::new(
                "infinite-universal requires an unbounded domain",
            ));
        }
        let sampled = self.kind == Kind::InfiniteUniversal || !self.domain.is_finite();
        if sampled && self.sampling.is_none() {
            return Err(Error::new(
                "sampled checking requires explicit seed and samples",
            ));
        }
        if !sampled && self.sampling.is_some() {
            return Err(Error::new(
                "sampling is not used for exhaustive finite checking",
            ));
        }
        if self
            .sampling
            .as_ref()
            .is_some_and(|s| s.samples == 0 || s.samples > SAMPLE_CAP)
        {
            return Err(Error::new("samples must be 1..10000"));
        }
        self.predicate.validate()?;
        catalog::validate_domain(&self.subject, &self.domain)?;
        let input = self.domain.representative()?;
        let output = catalog::output_shape(&self.subject)?;
        self.predicate.validate_types(&input, &output)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quantifier_annotations_kinds_and_rejections() {
        let base = serde_json::json!({"kind":"finite-universal","domain":{"type":"int-range","lo":-128,"hi":127},"subject":"abs_i8","predicate":{"version":1,"expression":{"op":"ge","left":{"kind":"field","source":"output"},"right":{"kind":"literal","value":{"type":"int","value":0}}}}});
        assert!(Quantifier::from_section(&base).is_ok());
        let mut v = base.clone();
        v["kind"] = "existential".into();
        assert!(Quantifier::from_section(&v).is_ok());
        v["kind"] = "infinite-universal".into();
        v["subject"] = "clamp".into();
        v["domain"] = serde_json::json!({"type":"unbounded","base":"int","size_hint":8});
        v["sampling"] = serde_json::json!({"seed":7,"samples":10});
        assert!(Quantifier::from_section(&v).is_ok());
        v["kind"] = "finite-universal".into();
        v["domain"] = serde_json::json!({"type":"unbounded","base":"int","size_hint":8});
        assert!(Quantifier::from_section(&v)
            .unwrap_err()
            .to_string()
            .contains("infinite-universal"));
        assert!(Domain::IntRange {
            lo: 0,
            hi: 1_000_000
        }
        .validate(FINITE_CAP)
        .is_err());
        assert!(Domain::IntRange { lo: 2, hi: 1 }
            .validate(FINITE_CAP)
            .is_err());
        assert!(Domain::Enum { values: vec![] }
            .validate(FINITE_CAP)
            .is_err());
        let mut v = base;
        v["subject"] = "unknown".into();
        assert!(Quantifier::from_section(&v).is_err());
    }
}
