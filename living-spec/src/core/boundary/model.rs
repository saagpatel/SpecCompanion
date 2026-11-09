use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

const SCALE: i128 = 1_000_000_000;
/// Exact decimal literal with at most nine fractional digits; never a float.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Decimal(pub(crate) String);
impl Decimal {
    pub fn scaled(&self) -> Result<i128, BoundaryError> {
        let s = self.0.as_str();
        let negative = s.starts_with('-');
        let unsigned = s.strip_prefix('-').unwrap_or(s);
        let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || fraction.len() > 9
            || !fraction.bytes().all(|b| b.is_ascii_digit())
            || s.ends_with('.')
        {
            return Err(BoundaryError::InvalidBound(format!("invalid decimal {s}")));
        }
        let whole: i128 = whole.parse().map_err(|_| BoundaryError::Overflow)?;
        let frac: i128 = if fraction.is_empty() {
            0
        } else {
            fraction.parse().map_err(|_| BoundaryError::Overflow)?
        };
        let n = whole
            .checked_mul(SCALE)
            .and_then(|v| v.checked_add(frac * 10_i128.pow(9 - fraction.len() as u32)))
            .ok_or(BoundaryError::Overflow)?;
        Ok(if negative { -n } else { n })
    }
    pub fn from_scaled(n: i128) -> Self {
        let magnitude = n.unsigned_abs();
        let whole = magnitude / SCALE as u128;
        let frac = magnitude % SCALE as u128;
        let sign = if n < 0 { "-" } else { "" };
        Self(if frac == 0 {
            format!("{sign}{whole}")
        } else {
            format!("{sign}{whole}.{frac:09}")
                .trim_end_matches('0')
                .to_owned()
        })
    }
}
impl FromStr for Decimal {
    type Err = BoundaryError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let d = Self(s.into());
        d.scaled()?;
        Ok(d)
    }
}
impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    #[serde(rename = "ms")]
    Ms,
    #[serde(rename = "s")]
    S,
    #[serde(rename = "min")]
    Min,
    #[serde(rename = "h")]
    H,
    #[serde(rename = "d")]
    D,
    B,
    KB,
    MB,
    GB,
    KiB,
    MiB,
    #[serde(rename = "items")]
    Items,
    #[serde(rename = "chars")]
    Chars,
    #[serde(rename = "lines")]
    Lines,
    #[serde(rename = "retries")]
    Retries,
    #[serde(rename = "files")]
    Files,
    #[serde(rename = "percent")]
    Percent,
}
impl Unit {
    pub fn base(self) -> Self {
        match self {
            Self::Ms | Self::S | Self::Min | Self::H | Self::D => Self::Ms,
            Self::B | Self::KB | Self::MB | Self::GB | Self::KiB | Self::MiB => Self::B,
            u => u,
        }
    }
    pub fn factor(self) -> i128 {
        match self {
            Self::S => 1000,
            Self::Min => 60_000,
            Self::H => 3_600_000,
            Self::D => 86_400_000,
            Self::KB => 1000,
            Self::MB => 1_000_000,
            Self::GB => 1_000_000_000,
            Self::KiB => 1024,
            Self::MiB => 1_048_576,
            _ => 1,
        }
    }
    pub fn normalize(self, value: &Decimal) -> Result<i128, BoundaryError> {
        value
            .scaled()?
            .checked_mul(self.factor())
            .ok_or(BoundaryError::Overflow)
    }
}
impl FromStr for Unit {
    type Err = BoundaryError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_value(serde_json::Value::String(
            if s == "%" { "percent" } else { s }.into(),
        ))
        .map_err(|_| BoundaryError::NeedsAnnotation(format!("unsupported unit {s}")))
    }
}
impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            serde_json::to_value(self).unwrap().as_str().unwrap()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Range {
        lo: Decimal,
        hi: Decimal,
        inclusive_lo: bool,
        inclusive_hi: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub value: Decimal,
    pub unit: Unit,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BoundKind {
    Numeric {
        value: Decimal,
        unit: Unit,
        op: Op,
        step: Step,
        integer: bool,
    },
    Domain {
        allowed: Vec<String>,
        closed: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bound {
    pub quantity: String,
    pub kind: BoundKind,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryError {
    NeedsAnnotation(String),
    InvalidBound(String),
    UnitContradiction {
        requirement_unit: Unit,
        test_unit: Unit,
        detail: String,
    },
    Overflow,
}
impl fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnitContradiction { requirement_unit, test_unit, detail } => write!(f, "UnitContradiction: requirement uses {requirement_unit}, test uses {test_unit}: {detail}"),
            _ => write!(f, "{self:?}"),
        }
    }
}
impl std::error::Error for BoundaryError {}
