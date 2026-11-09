//! Closed v1 claim interpreter. Expressions cannot call subjects or execute code.
use super::{annotations::Value, Error, Result};
pub use crate::core::quantifier::{Expr, Operand, PredicateRef, Source};
use std::cmp::Ordering;
pub struct Budget {
    remaining: u64,
}
impl Budget {
    pub fn new(remaining: u64) -> Self {
        Self { remaining }
    }
    pub fn spend(&mut self, cost: u64) -> Result<()> {
        self.remaining = self
            .remaining
            .checked_sub(cost)
            .ok_or_else(|| Error::new("predicate/check work cap exceeded"))?;
        Ok(())
    }
}
impl Operand {
    fn reads_output(&self) -> bool {
        matches!(
            self,
            Self::Field {
                source: Source::Output,
                ..
            }
        )
    }
    fn validate(&self) -> Result<()> {
        match self {
            Self::Literal { value } => value.validate(0),
            Self::Field { path, .. } if path.len() > 8 || path.iter().any(|n| *n >= 8) => {
                Err(Error::new("field path exceeds tuple bounds"))
            }
            _ => Ok(()),
        }
    }
    fn get<'a>(&'a self, input: &'a Value, output: &'a Value) -> Result<&'a Value> {
        match self {
            Self::Literal { value } => Ok(value),
            Self::Field { source, path } => {
                let mut v = if *source == Source::Input {
                    input
                } else {
                    output
                };
                for i in path {
                    let Value::Tuple(parts) = v else {
                        return Err(Error::new("field path requires tuple"));
                    };
                    v = parts
                        .get(*i)
                        .ok_or_else(|| Error::new("field path missing"))?;
                }
                Ok(v)
            }
        }
    }
}
impl PredicateRef {
    pub fn validate_types(&self, input: &Value, output: &Value) -> Result<()> {
        self.expression.validate_types(input, output)
    }
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err(Error::new("unsupported predicate AST version"));
        }
        let mut nodes = 0;
        self.expression.validate_at(1, &mut nodes)?;
        if !self.expression.reads_output() {
            return Err(Error::new(
                "claim predicate must reference an observed output",
            ));
        }
        Ok(())
    }
    pub fn evaluate(&self, input: &Value, output: &Value, budget: &mut Budget) -> Result<bool> {
        self.expression.evaluate(input, output, budget)
    }
}
impl Expr {
    fn validate_types(&self, input: &Value, output: &Value) -> Result<()> {
        match self {
            Self::And { args } | Self::Or { args } => {
                for arg in args {
                    arg.validate_types(input, output)?;
                }
            }
            Self::Not { arg } => arg.validate_types(input, output)?,
            Self::In { value, set } => {
                let value = value.get(input, output)?;
                for member in set {
                    comparable(value, member)?;
                }
            }
            Self::Eq { left, right }
            | Self::Ne { left, right }
            | Self::Lt { left, right }
            | Self::Le { left, right }
            | Self::Gt { left, right }
            | Self::Ge { left, right } => {
                comparable(left.get(input, output)?, right.get(input, output)?)?;
            }
        }
        Ok(())
    }
    fn reads_output(&self) -> bool {
        match self {
            Self::And { args } | Self::Or { args } => args.iter().any(Self::reads_output),
            Self::Not { arg } => arg.reads_output(),
            Self::In { value, .. } => value.reads_output(),
            Self::Eq { left, right }
            | Self::Ne { left, right }
            | Self::Lt { left, right }
            | Self::Le { left, right }
            | Self::Gt { left, right }
            | Self::Ge { left, right } => left.reads_output() || right.reads_output(),
        }
    }
    fn validate_at(&self, depth: usize, nodes: &mut usize) -> Result<()> {
        *nodes += 1;
        if depth > 16 || *nodes > 1024 {
            return Err(Error::new("predicate depth/node cap exceeded"));
        }
        match self {
            Self::And { args } | Self::Or { args } => {
                if args.is_empty() || args.len() > 64 {
                    return Err(Error::new("boolean node needs 1..64 arguments"));
                }
                for arg in args {
                    arg.validate_at(depth + 1, nodes)?;
                }
            }
            Self::Not { arg } => arg.validate_at(depth + 1, nodes)?,
            Self::In { value, set } => {
                value.validate()?;
                if set.is_empty() || set.len() > 64 {
                    return Err(Error::new("membership needs 1..64 values"));
                }
                for v in set {
                    v.validate(0)?;
                }
            }
            Self::Eq { left, right }
            | Self::Ne { left, right }
            | Self::Lt { left, right }
            | Self::Le { left, right }
            | Self::Gt { left, right }
            | Self::Ge { left, right } => {
                left.validate()?;
                right.validate()?;
            }
        }
        Ok(())
    }
    fn evaluate(&self, input: &Value, output: &Value, budget: &mut Budget) -> Result<bool> {
        budget.spend(1)?;
        match self {
            Self::And { args } => {
                for arg in args {
                    if !arg.evaluate(input, output, budget)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::Or { args } => {
                for arg in args {
                    if arg.evaluate(input, output, budget)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Self::Not { arg } => Ok(!arg.evaluate(input, output, budget)?),
            Self::In { value, set } => {
                let v = value.get(input, output)?;
                for candidate in set {
                    budget.spend(v.cost() + candidate.cost())?;
                    if comparable(v, candidate)? == Ordering::Equal {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Self::Eq { left, right }
            | Self::Ne { left, right }
            | Self::Lt { left, right }
            | Self::Le { left, right }
            | Self::Gt { left, right }
            | Self::Ge { left, right } => {
                let (a, b) = (left.get(input, output)?, right.get(input, output)?);
                budget.spend(a.cost() + b.cost())?;
                let order = comparable(a, b)?;
                Ok(match self {
                    Self::Eq { .. } => order == Ordering::Equal,
                    Self::Ne { .. } => order != Ordering::Equal,
                    Self::Lt { .. } => order == Ordering::Less,
                    Self::Le { .. } => order != Ordering::Greater,
                    Self::Gt { .. } => order == Ordering::Greater,
                    Self::Ge { .. } => order != Ordering::Less,
                    _ => unreachable!(),
                })
            }
        }
    }
}
fn comparable(a: &Value, b: &Value) -> Result<Ordering> {
    if std::mem::discriminant(a) != std::mem::discriminant(b) {
        return Err(Error::new("predicate operand type mismatch"));
    }
    if let (Value::Tuple(a), Value::Tuple(b)) = (a, b) {
        if a.len() != b.len() {
            return Err(Error::new("tuple operand shape mismatch"));
        }
        for (a, b) in a.iter().zip(b) {
            comparable(a, b)?;
        }
    }
    Ok(a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(source: Source) -> Operand {
        Operand::Field {
            source,
            path: vec![],
        }
    }
    fn literal(n: i64) -> Operand {
        Operand::Literal {
            value: Value::Int(n),
        }
    }
    fn comparison() -> Expr {
        Expr::Ge {
            left: field(Source::Output),
            right: literal(0),
        }
    }
    #[test]
    fn closed_predicate_ast_caps_and_types() {
        let mut p = PredicateRef {
            version: 1,
            expression: comparison(),
        };
        p.validate().unwrap();
        for _ in 0..16 {
            p.expression = Expr::Not {
                arg: Box::new(p.expression),
            };
        }
        assert!(p.validate().is_err());
        p = PredicateRef {
            version: 1,
            expression: Expr::And {
                args: (0..64)
                    .map(|_| Expr::And {
                        args: (0..64).map(|_| comparison()).collect(),
                    })
                    .collect(),
            },
        };
        assert!(p.validate().is_err());
        p = PredicateRef {
            version: 2,
            expression: comparison(),
        };
        assert!(p.validate().is_err());
        p = PredicateRef {
            version: 1,
            expression: comparison(),
        };
        assert!(p
            .evaluate(&Value::Int(0), &Value::Int(0), &mut Budget::new(1))
            .is_err());
        assert!(serde_json::from_str::<PredicateRef>(
            r#"{"version":1,"expression":{"op":"eval","source":"code"}}"#
        )
        .is_err());
        let malformed = Expr::In {
            value: field(Source::Output),
            set: vec![Value::Int(0), Value::String("zero".into())],
        };
        p.expression = Expr::Or {
            args: vec![comparison(), malformed],
        };
        // First branch would short-circuit true; preflight still rejects the second.
        assert!(p.validate_types(&Value::Int(0), &Value::Int(0)).is_err());
        p.expression = Expr::Eq {
            left: Operand::Field {
                source: Source::Output,
                path: vec![0],
            },
            right: literal(0),
        };
        assert!(p.validate_types(&Value::Int(0), &Value::Int(0)).is_err());
        p.expression = Expr::Eq {
            left: field(Source::Input),
            right: literal(0),
        };
        assert!(p.validate().is_err());
    }
    #[test]
    fn closed_predicate_boolean_membership_and_tuple_fields() {
        let p = PredicateRef {
            version: 1,
            expression: Expr::And {
                args: vec![
                    Expr::Ne {
                        left: field(Source::Output),
                        right: literal(-1),
                    },
                    Expr::Not {
                        arg: Box::new(Expr::Lt {
                            left: field(Source::Output),
                            right: literal(0),
                        }),
                    },
                    Expr::Or {
                        args: vec![
                            Expr::Gt {
                                left: field(Source::Output),
                                right: literal(100),
                            },
                            Expr::Le {
                                left: field(Source::Output),
                                right: literal(100),
                            },
                        ],
                    },
                    Expr::In {
                        value: Operand::Field {
                            source: Source::Input,
                            path: vec![1],
                        },
                        set: vec![Value::Int(2), Value::Int(3)],
                    },
                ],
            },
        };
        p.validate().unwrap();
        let input = Value::Tuple(vec![Value::Int(1), Value::Int(2)]);
        p.validate_types(&input, &Value::Int(3)).unwrap();
        assert!(p
            .evaluate(&input, &Value::Int(3), &mut Budget::new(100))
            .unwrap());
    }
}
