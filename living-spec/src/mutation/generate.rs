//! Deterministic token-level candidates for a verified free-function extent.
use crate::agents::syntax::{close, lex_rust, Kind};
use crate::core::mutation::{mutant_id, Mutant, MutantOutcome, Operator};
use std::ops::Range;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub mutant: Mutant,
    pub span: Range<usize>,
}

/// The caller must supply a verified, non-test free-function extent. Reject
/// attributes, nested items and macros rather than guessing their semantics.
pub fn generate(
    source: &str,
    file: &str,
    function: &str,
    extent: Range<usize>,
    cap: usize,
) -> Vec<Candidate> {
    let Some(text) = source.get(extent.clone()) else {
        return Vec::new();
    };
    let tokens = lex_rust(text);
    if tokens.iter().any(|t| {
        matches!(t.text.as_str(), "#" | "!" | "const" | "static") || t.kind == Kind::Unsupported
    }) || tokens.iter().filter(|t| t.text == "fn").count() != 1
    {
        return Vec::new();
    }
    let Some(open) = tokens.iter().position(|t| t.text == "{") else {
        return Vec::new();
    };
    let Some(end) = close(&tokens, open) else {
        return Vec::new();
    };
    if end + 1 != tokens.len() {
        return Vec::new();
    }
    let mut numeric_run = vec![false; tokens.len()];
    for i in 0..tokens.len().saturating_sub(1) {
        if tokens[i].kind == Kind::Number
            && tokens[i + 1].kind == Kind::Ident
            && tokens[i].end == tokens[i + 1].start
        {
            numeric_run[i] = true;
            numeric_run[i + 1] = true;
            if matches!(tokens[i + 1].text.as_str(), "e" | "E")
                && i + 3 < tokens.len()
                && matches!(tokens[i + 2].text.as_str(), "+" | "-")
                && tokens[i + 1].end == tokens[i + 2].start
                && tokens[i + 2].end == tokens[i + 3].start
                && tokens[i + 3].kind == Kind::Number
            {
                numeric_run[i + 2] = true;
                numeric_run[i + 3] = true;
            }
        }
    }
    let mut groups: [Vec<Candidate>; 4] = Default::default();
    for (index, token) in tokens.iter().enumerate().take(end).skip(open + 1) {
        if numeric_run[index] {
            continue;
        }
        let prev = index.checked_sub(1).and_then(|i| tokens.get(i));
        let next = tokens.get(index + 1);
        // Suffixes, prefixes, floats and compound assignments can be split
        // into adjacent tokens by the lexer.
        if next.is_some_and(|n| {
            n.start == token.end
                && (n.text == "="
                    || (token.kind == Kind::Number && (n.kind == Kind::Ident || n.text == ".")))
        }) || (token.kind == Kind::Number
            && prev
                .is_some_and(|p| p.end == token.start && (p.kind == Kind::Ident || p.text == ".")))
        {
            continue;
        }
        let (group, operator, replacements): (usize, Operator, Vec<String>) =
            match token.text.as_str() {
                "==" => (0, Operator::CmpFlip, vec!["!=".into()]),
                "!=" => (0, Operator::CmpFlip, vec!["==".into()]),
                "<" => (0, Operator::CmpFlip, vec![">=".into()]),
                ">=" => (0, Operator::CmpFlip, vec!["<".into()]),
                "<=" => (0, Operator::CmpFlip, vec![">".into()]),
                ">" => (0, Operator::CmpFlip, vec!["<=".into()]),
                "+" => (1, Operator::ArithSwap, vec!["-".into()]),
                "-" => (1, Operator::ArithSwap, vec!["+".into()]),
                "*" => (1, Operator::ArithSwap, vec!["/".into()]),
                "/" => (1, Operator::ArithSwap, vec!["*".into()]),
                "&&" => (2, Operator::BoolSwap, vec!["||".into()]),
                "||" => (2, Operator::BoolSwap, vec!["&&".into()]),
                "true" => (2, Operator::BoolSwap, vec!["false".into()]),
                "false" => (2, Operator::BoolSwap, vec!["true".into()]),
                s if token.kind == Kind::Number && s.bytes().all(|b| b.is_ascii_digit()) => {
                    let Ok(n) = s.parse::<u128>() else { continue };
                    let Some(bumped) = n.checked_add(1) else {
                        continue;
                    };
                    let mut replacements = vec![bumped.to_string()];
                    if n != 0 {
                        replacements.push("0".into());
                    }
                    (3, Operator::IntBump, replacements)
                }
                _ => continue,
            };
        for replacement in replacements {
            let ordinal = index as u64;
            groups[group].push(Candidate {
                mutant: Mutant {
                    id: mutant_id(file, function, operator, &token.text, &replacement, ordinal),
                    file: file.into(),
                    function: function.into(),
                    line: (source[..extent.start + token.start]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                        + 1) as u64,
                    ordinal,
                    operator,
                    original: token.text.clone(),
                    replacement,
                    outcome: MutantOutcome::Unviable,
                },
                span: extent.start + token.start..extent.start + token.end,
            });
        }
    }
    let mut result = Vec::new();
    let mut iterators = groups.into_iter().map(Vec::into_iter).collect::<Vec<_>>();
    while result.len() < cap {
        let mut advanced = false;
        for iter in &mut iterators {
            if result.len() == cap {
                break;
            }
            if let Some(candidate) = iter.next() {
                result.push(candidate);
                advanced = true;
            }
        }
        if !advanced {
            break;
        }
    }
    result
}

/// Select candidates across all functions, cycling through operators in order.
pub fn candidates_for(subject: &super::subject::Subject, max_mutants: usize) -> Vec<Candidate> {
    let mut functions = subject.functions.iter().collect::<Vec<_>>();
    functions
        .sort_by(|a, b| (&a.file, a.span.start, &a.name).cmp(&(&b.file, b.span.start, &b.name)));
    let mut groups: [Vec<Candidate>; 4] = Default::default();
    for function in functions {
        for candidate in generate(
            &function.source,
            &function.file,
            &function.name,
            function.span.clone(),
            usize::MAX,
        ) {
            let group = match candidate.mutant.operator {
                Operator::CmpFlip => 0,
                Operator::ArithSwap => 1,
                Operator::BoolSwap => 2,
                Operator::IntBump => 3,
            };
            groups[group].push(candidate);
        }
    }
    let mut groups = groups.into_iter().map(Vec::into_iter).collect::<Vec<_>>();
    let mut result = Vec::new();
    while result.len() < max_mutants {
        let mut advanced = false;
        for group in &mut groups {
            if result.len() == max_mutants {
                break;
            }
            if let Some(candidate) = group.next() {
                result.push(candidate);
                advanced = true;
            }
        }
        if !advanced {
            break;
        }
    }
    result
}

/// Replace one UTF-8 byte span, returning None for an invalid range.
pub fn apply(source: &str, span: Range<usize>, replacement: &str) -> Option<String> {
    if span.start > span.end {
        return None;
    }
    Some(format!(
        "{}{replacement}{}",
        source.get(..span.start)?,
        source.get(span.end..)?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn exponent_is_not_mutated(literal: &str, operator: &str) {
        let source = format!("fn value() -> f64 {{ {literal} {operator} 2.0 }}");
        let start = source.find(literal).unwrap();
        let candidates = generate(&source, "src/lib.rs", "value", 0..source.len(), 50);
        assert!(candidates
            .iter()
            .all(|c| c.span.end <= start || c.span.start >= start + literal.len()));
        assert!(candidates.iter().any(|c| c.mutant.original == operator));
    }
    #[test]
    fn positive_exponent_literal_not_mutated() {
        exponent_is_not_mutated("1e+3", "+");
    }
    #[test]
    fn negative_exponent_literal_not_mutated() {
        exponent_is_not_mutated("1e-3", "-");
    }
    #[test]
    fn apply_then_revert_round_trips() {
        let source = "fn check(x: i32) -> bool { x <= 4 && true }";
        for candidate in generate(source, "src/lib.rs", "check", 0..source.len(), 50) {
            let changed = apply(
                source,
                candidate.span.clone(),
                &candidate.mutant.replacement,
            )
            .unwrap();
            let restored = apply(
                &changed,
                candidate.span.start..candidate.span.start + candidate.mutant.replacement.len(),
                &candidate.mutant.original,
            )
            .unwrap();
            assert_eq!(restored, source);
        }
    }
    #[test]
    fn string_and_comment_operators_not_mutated() {
        let source = "fn check() -> bool { let _s = \"== 22\"; /* <= 3 */ true }";
        let candidates = generate(source, "src/lib.rs", "check", 0..source.len(), 50);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].mutant.original, "true");
    }
    #[test]
    fn mutant_cap_deterministic() {
        let source = "fn check(x: i32) -> bool { x < 3 && x + 1 > 0 }";
        let a = generate(source, "src/lib.rs", "check", 0..source.len(), 4);
        let b = generate(source, "src/lib.rs", "check", 0..source.len(), 4);
        assert_eq!(
            a.iter().map(|c| &c.mutant.id).collect::<Vec<_>>(),
            b.iter().map(|c| &c.mutant.id).collect::<Vec<_>>()
        );
        assert_eq!(a.len(), 4);
    }
}
