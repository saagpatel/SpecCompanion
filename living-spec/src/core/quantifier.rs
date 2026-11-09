//! Serde models shared by the quantifier checker and status renderer.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Value {
    Int(i64),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
    VecInt(Vec<i64>),
    Tuple(Vec<Value>),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Base {
    Int,
    String,
    Bytes,
    VecInt,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Domain {
    IntRange { lo: i64, hi: i64 },
    Enum { values: Vec<Value> },
    Product { parts: Vec<Domain> },
    Unbounded { base: Base, size_hint: u32 },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Existential,
    FiniteUniversal,
    InfiniteUniversal,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sampling {
    pub seed: u64,
    pub samples: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quantifier {
    pub kind: Kind,
    pub domain: Domain,
    pub subject: String,
    pub predicate: PredicateRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampling: Option<Sampling>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredicateRef {
    pub version: u32,
    pub expression: Expr,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Expr {
    Eq { left: Operand, right: Operand },
    Ne { left: Operand, right: Operand },
    Lt { left: Operand, right: Operand },
    Le { left: Operand, right: Operand },
    Gt { left: Operand, right: Operand },
    Ge { left: Operand, right: Operand },
    In { value: Operand, set: Vec<Value> },
    And { args: Vec<Expr> },
    Or { args: Vec<Expr> },
    Not { arg: Box<Expr> },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Input,
    Output,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Operand {
    Literal {
        value: Value,
    },
    Field {
        source: Source,
        #[serde(default)]
        path: Vec<usize>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub id: String,
    pub text: String,
    pub quantifier: Quantifier,
    #[serde(default)]
    pub expected: Option<Expected>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    pub result: Outcome,
    pub min_failures: u64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    NoCounterexampleInTestedCases,
    CounterexampleFound,
    WitnessFound,
    NoWitnessAmongTestedCases,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Exhaustive,
    Sampled,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    pub mode: Mode,
    pub tested_cases: u64,
    pub distinct_tested: u64,
    pub finite_total: Option<u64>,
    pub seed: Option<u64>,
    pub untested_remainder: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub input: Value,
    pub output: Value,
    pub satisfied: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub id: String,
    pub text: String,
    pub quantifier: Quantifier,
    pub coverage: Coverage,
    pub result: Outcome,
    pub observations: Vec<Observation>,
    pub failing_subdomains: Vec<Domain>,
    pub witness: Option<Observation>,
}
