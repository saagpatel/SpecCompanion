//! Allowlisted compiled fixture subjects. Claims live separately in the JSON AST.
use super::{
    annotations::{Base, Domain, Value},
    Error, Result,
};
pub const VERSION: &str = "fixture-subjects/v1";
/// A real project can implement a reviewed, compiled observation adapter.
/// The shipped CLI exclusively uses `FixtureCatalog`; it never loads plugins.
pub trait ClaimChecker {
    fn observe(&self, subject: &str, input: &Value) -> Result<Value>;
}
pub struct FixtureCatalog;
impl ClaimChecker for FixtureCatalog {
    fn observe(&self, subject: &str, input: &Value) -> Result<Value> {
        match (subject, input) {
            ("abs_i8", Value::Int(n)) if (-128..=127).contains(n) => Ok(Value::Int(n.abs())),
            ("abs_wrapping_i8", Value::Int(n)) if (-128..=127).contains(n) => {
                Ok(Value::Int((*n as i8).wrapping_abs() as i64))
            }
            ("clamp" | "percent_of", Value::Int(n)) => Ok(Value::Int((*n).clamp(0, 100))),
            ("clamp_buggy" | "percent_of_buggy", Value::Int(n)) => Ok(Value::Int((*n).max(0))),
            ("saturating_add", Value::Tuple(parts)) if parts.len() == 2 => {
                match (&parts[0], &parts[1]) {
                    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a.saturating_add(*b))),
                    _ => Err(Error::new("saturating_add needs two integers")),
                }
            }
            ("utf8_roundtrip", Value::String(s)) => Ok(Value::String(s.clone())),
            ("utf8_roundtrip_buggy", Value::String(_)) => Ok(Value::String(String::new())),
            ("bytes_roundtrip", Value::Bytes(v)) => Ok(Value::Bytes(v.clone())),
            ("sort_vec", Value::VecInt(v)) => {
                let mut v = v.clone();
                v.sort();
                Ok(Value::VecInt(v))
            }
            ("sort_vec_buggy", Value::VecInt(v)) => Ok(Value::VecInt(v.clone())),
            ("dedup_vec", Value::VecInt(v)) => {
                let mut v = v.clone();
                v.dedup();
                Ok(Value::VecInt(v))
            }
            _ => Err(Error::new(
                "subject is unknown or input has an unsupported type/range",
            )),
        }
    }
}
#[derive(Clone, Copy)]
enum Shape {
    Int,
    I8,
    String,
    Bytes,
    VecInt,
    Pair,
}
fn shape(subject: &str) -> Result<Shape> {
    Ok(match subject {
        "abs_i8" | "abs_wrapping_i8" => Shape::I8,
        "clamp" | "clamp_buggy" | "percent_of" | "percent_of_buggy" => Shape::Int,
        "utf8_roundtrip" | "utf8_roundtrip_buggy" => Shape::String,
        "bytes_roundtrip" => Shape::Bytes,
        "sort_vec" | "sort_vec_buggy" | "dedup_vec" => Shape::VecInt,
        "saturating_add" => Shape::Pair,
        _ => return Err(Error::new("unknown fixture subject")),
    })
}
/// A type representative, not a subject execution or tested observation.
pub fn output_shape(subject: &str) -> Result<Value> {
    Ok(match shape(subject)? {
        Shape::Int | Shape::I8 | Shape::Pair => Value::Int(0),
        Shape::String => Value::String(String::new()),
        Shape::Bytes => Value::Bytes(vec![]),
        Shape::VecInt => Value::VecInt(vec![]),
    })
}
fn matches_value(shape: Shape, value: &Value) -> bool {
    match (shape, value) {
        (Shape::Int, Value::Int(_))
        | (Shape::String, Value::String(_))
        | (Shape::Bytes, Value::Bytes(_))
        | (Shape::VecInt, Value::VecInt(_)) => true,
        (Shape::I8, Value::Int(v)) => (-128..=127).contains(v),
        (Shape::Pair, Value::Tuple(v)) => {
            v.len() == 2 && v.iter().all(|v| matches_value(Shape::Int, v))
        }
        _ => false,
    }
}
fn matches_domain(shape: Shape, domain: &Domain) -> bool {
    match domain {
        Domain::Enum { values } => values.iter().all(|v| matches_value(shape, v)),
        Domain::IntRange { lo, hi } => {
            matches!(shape, Shape::Int) || matches!(shape, Shape::I8) && *lo >= -128 && *hi <= 127
        }
        Domain::Unbounded { base, .. } => matches!(
            (shape, base),
            (Shape::Int, Base::Int)
                | (Shape::String, Base::String)
                | (Shape::Bytes, Base::Bytes)
                | (Shape::VecInt, Base::VecInt)
        ),
        Domain::Product { parts } => {
            matches!(shape, Shape::Pair)
                && parts.len() == 2
                && parts.iter().all(|p| matches_domain(Shape::Int, p))
        }
    }
}
pub fn validate_domain(subject: &str, domain: &Domain) -> Result<()> {
    if !matches_domain(shape(subject)?, domain) {
        return Err(Error::new(
            "subject input type/range does not cover declared domain",
        ));
    }
    Ok(())
}
