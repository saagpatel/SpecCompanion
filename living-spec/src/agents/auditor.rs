//! Conservative, offline assertion analysis. Acceptance is a syntactic relevance check,
//! not a claim of semantic equivalence or of execution.
use super::{
    linker::terms,
    syntax::{close, lex, lex_javascript, lex_rust, split_args, Kind, Token},
    test_index::{Language, TestEntry},
};
use crate::core::{
    annotations::{Audit, AuditVerdict, Link},
    spec::RequirementLine,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn audit(requirement: &RequirementLine, link: &Link, entry: &TestEntry) -> Audit {
    let mut notes = Vec::new();
    if link.test_id != entry.id || link.fingerprint != entry.fingerprint {
        notes.push("Test identity changed or does not resolve to the indexed source; re-link and re-audit it.".into());
    }
    let tokens = match entry.language {
        Language::Rust => lex_rust(&entry.body),
        Language::JavaScript => lex_javascript(&entry.body),
        Language::Python => lex(&entry.body, true),
    };
    let analysis = assertions(&tokens, &entry.body, entry.language, entry.should_panic);
    if entry.language == Language::Python && entry.decorated {
        notes.push("Assertion syntax is unsupported for decorated tests; fixture and parametrization effects are unknown.".into());
    }
    if analysis.unsupported {
        notes.push(
            "Assertion syntax is unsupported; relevance cannot be analysed conservatively.".into(),
        );
    }
    if analysis.expressions.is_empty() {
        notes.push("The test body has no supported assertion.".into());
    } else {
        let expected = terms(&requirement.text);
        let meaningful: Vec<_> = analysis
            .expressions
            .iter()
            .filter(|e| !placeholder(e))
            .collect();
        if meaningful.is_empty() {
            notes.push(
                "The test contains only placeholder assertions (literal or self-comparisons)."
                    .into(),
            );
        } else if !meaningful.iter().any(|e| relevant(e, &expected)) {
            notes.push("Assertions check unrelated values; no asserted expression derives from requirement terms.".into());
        }
        let numbers = numeric_tokens(&lex(&requirement.text, false));
        let used = numeric_tokens(&analysis.inputs);
        if numbers.iter().any(|n| {
            !used
                .iter()
                .any(|value| n == value || (n - value).abs() == 1.0)
        }) {
            notes.push("A required number is absent from assertions and call inputs, including adjacent values.".into());
        }
    }
    if notes.is_empty() {
        notes.push("A supported non-placeholder assertion touches requirement terms and required numeric inputs.".into());
        Audit {
            extra: BTreeMap::new(),
            revision: requirement.revision.clone(),
            verdict: AuditVerdict::Accepted,
            notes,
        }
    } else {
        Audit {
            extra: BTreeMap::new(),
            revision: requirement.revision.clone(),
            verdict: AuditVerdict::Challenged,
            notes,
        }
    }
}

#[derive(Default)]
struct Analysis {
    expressions: Vec<Vec<Token>>,
    inputs: Vec<Token>,
    unsupported: bool,
}

fn assertions(tokens: &[Token], source: &str, language: Language, should_panic: bool) -> Analysis {
    let mut result = Analysis::default();
    let mut bindings: BTreeMap<String, Vec<Token>> = BTreeMap::new();
    let mut i = 0;
    // Branch-sensitive data flow, interpolation, macros in expressions, and nested functions
    // are intentionally outside this small analyser.
    if tokens.iter().enumerate().any(|(i, t)| {
        t.kind == Kind::Unsupported
            || matches!(
                t.text.as_str(),
                "for"
                    | "while"
                    | "loop"
                    | "lambda"
                    | "yield"
                    | "def"
                    | "fn"
                    | "function"
                    | "try"
                    | "catch"
                    | "return"
                    | "else"
                    | "=>"
                    | "or"
                    | "and"
                    | "||"
                    | "&&"
            )
            || t.kind == Kind::String
                && (t.text.contains("${")
                    || t.text.starts_with("\"\"\"")
                    || t.text.starts_with("'''"))
            || t.text == "if" && (language != Language::Rust || !supported_panic_branch(tokens, i))
            || t.kind == Kind::Ident
                && matches!(t.text.as_str(), "f" | "r" | "b" | "fr" | "rf")
                && tokens
                    .get(i + 1)
                    .is_some_and(|next| next.kind == Kind::String)
    }) {
        result.unsupported = true;
    }
    if should_panic {
        // Only one direct call with inert arguments can supply the expected
        // panic. An assertion, setup call, index, or nested argument may panic
        // independently of the relevant subject.
        if let Some(end) = direct_call_end(tokens, 0, Language::Rust) {
            if end + 1 == tokens.len() || end + 2 == tokens.len() && tokens[end + 1].text == ";" {
                result.expressions.push(tokens[..=end].to_vec());
                result.inputs.extend_from_slice(&tokens[..=end]);
                return result;
            }
        }
        result.unsupported = true;
        return result;
    }
    while i < tokens.len() {
        // Track simple single-name assignments in source order; constants cannot become evidence
        // just by being assigned a requirement-shaped variable name.
        // Follow single-name Rust bindings, including mutability and type annotations.
        let binding = if language == Language::Rust && tokens[i].text == "let" {
            let name = i + 1 + usize::from(tokens.get(i + 1).is_some_and(|t| t.text == "mut"));
            let assignment = (name + 1..tokens.len())
                .take_while(|j| !matches!(tokens[*j].text.as_str(), ";" | "{"))
                .find(|j| tokens[*j].text == "=");
            assignment
                .filter(|eq| {
                    tokens.get(name).is_some_and(|t| t.kind == Kind::Ident)
                        && (*eq == name + 1 || tokens[name + 1].text == ":")
                })
                .map(|eq| (name, eq))
        } else if tokens[i].kind == Kind::Ident && tokens.get(i + 1).is_some_and(|t| t.text == "=")
        {
            Some((i, i + 1))
        } else {
            None
        };
        if let Some((name, assignment)) = binding {
            let end = statement_end(tokens, assignment + 1, source, language);
            let value = expand(&tokens[assignment + 1..end], &bindings);
            bindings.insert(tokens[name].text.clone(), value);
            i = end;
            continue;
        }
        let name = tokens[i].text.as_str();
        let mut expressions: Vec<Vec<Token>> = Vec::new();
        let mut end = i;
        let mut recognised = false;
        if language == Language::Python && name == "with" {
            recognised = true;
            if let Some((begin, last)) = raises_suite(tokens, i, source) {
                expressions.push(tokens[begin..=last].to_vec());
                end = last;
            } else {
                result.unsupported = true;
            }
        } else if language == Language::Rust
            && matches!(name, "assert" | "assert_eq" | "assert_ne")
            && tokens.get(i + 1).is_some_and(|t| t.text == "!")
        {
            recognised = true;
            if let Some(open) = tokens
                .get(i + 2)
                .filter(|t| matches!(t.text.as_str(), "(" | "[" | "{"))
            {
                let _ = open;
                if let Some(last) = close(tokens, i + 2) {
                    let args = split_args(&tokens[i + 3..last]);
                    let count = if name == "assert" { 1 } else { 2 };
                    if args.len() >= count {
                        expressions.push(join_operands(&args[..count]));
                    } else {
                        result.unsupported = true;
                    }
                    end = last;
                } else {
                    result.unsupported = true;
                }
            } else {
                result.unsupported = true;
            }
        } else if language == Language::Python && name == "assert" {
            recognised = true;
            end = statement_end(tokens, i + 1, source, language);
            let args = split_args(&tokens[i + 1..end]);
            if let Some(expr) = args.first() {
                expressions.push(expr.to_vec());
            } else {
                result.unsupported = true;
            }
            end = end.saturating_sub(1);
        } else if language == Language::JavaScript
            && name == "expect"
            && tokens.get(i + 1).is_some_and(|t| t.text == "(")
        {
            recognised = true;
            if let Some(actual_end) = close(tokens, i + 1) {
                let mut matcher = actual_end + 2;
                if tokens.get(matcher).is_some_and(|t| t.text == "not") {
                    matcher += 2;
                }
                if tokens.get(actual_end + 1).is_some_and(|t| t.text == ".")
                    && tokens.get(matcher).is_some_and(|t| {
                        matches!(
                            t.text.as_str(),
                            "toBe"
                                | "toEqual"
                                | "toStrictEqual"
                                | "toBeTruthy"
                                | "toBeFalsy"
                                | "toBeNull"
                                | "toBeDefined"
                                | "toBeUndefined"
                                | "toContain"
                                | "toHaveLength"
                                | "toBeGreaterThan"
                                | "toBeLessThan"
                                | "toBeGreaterThanOrEqual"
                                | "toBeLessThanOrEqual"
                                | "toThrow"
                        )
                    })
                    && tokens.get(matcher + 1).is_some_and(|t| t.text == "(")
                {
                    if let Some(last) = close(tokens, matcher + 1) {
                        let actual = &tokens[i + 2..actual_end];
                        let expected = &tokens[matcher + 2..last];
                        // Preserve operand boundaries for placeholder detection.
                        let mut expr = actual.to_vec();
                        expr.push(Token {
                            kind: Kind::Symbol,
                            text: "__expect_actual_end__".into(),
                            start: 0,
                            end: 0,
                        });
                        expr.push(separator());
                        expr.extend_from_slice(expected);
                        expressions.push(expr);
                        end = last;
                    } else {
                        result.unsupported = true;
                    }
                } else {
                    result.unsupported = true;
                }
            } else {
                result.unsupported = true;
            }
        } else if matches!(
            name,
            "assert"
                | "assertEqual"
                | "assertTrue"
                | "assertFalse"
                | "strictEqual"
                | "deepEqual"
                | "equal"
                | "ok"
        ) && tokens.get(i + 1).is_some_and(|t| t.text == "(")
            && (language != Language::Rust)
            && (name.starts_with("assert") || i >= 2 && tokens[i - 2].text == "assert")
        {
            recognised = true;
            if let Some(last) = close(tokens, i + 1) {
                let args = split_args(&tokens[i + 2..last]);
                let count = if matches!(name, "assert" | "assertTrue" | "assertFalse" | "ok") {
                    1
                } else {
                    2
                };
                if args.len() >= count {
                    expressions.push(join_operands(&args[..count]));
                } else {
                    result.unsupported = true;
                }
                end = last;
            } else {
                result.unsupported = true;
            }
        } else if language == Language::Python
            && name == "raises"
            && i >= 2
            && tokens[i - 2].text == "pytest"
            && tokens.get(i + 1).is_some_and(|t| t.text == "(")
        {
            recognised = true;
            if let Some(last) = close(tokens, i + 1) {
                let args = split_args(&tokens[i + 2..last]);
                if args.len() >= 2
                    && exception_type(args[0])
                    && simple_path(args[1], ".")
                    && args[2..].iter().all(|arg| inert_argument(arg))
                {
                    expressions.push(args[1..].concat());
                    end = last;
                } else {
                    result.unsupported = true;
                }
            } else {
                result.unsupported = true;
            }
        } else if language == Language::Rust
            && name == "panic"
            && tokens.get(i + 1).is_some_and(|t| t.text == "!")
        {
            recognised = true;
            // A conditional panic is an inverted assertion. An unconditional panic is no proof.
            if let Some(branch) = tokens[..i].iter().rposition(|t| t.text == "if") {
                if let Some(open) = (branch + 1..i).find(|j| tokens[*j].text == "{") {
                    if close(tokens, open).is_some_and(|j| j > i) {
                        expressions.push(tokens[branch + 1..open].to_vec());
                    }
                }
            }
            if expressions.is_empty() {
                result.unsupported = true;
            }
            if let Some(last) = close(tokens, i + 2) {
                end = last;
            }
        } else if tokens[i].kind == Kind::Ident
            && (name.starts_with("assert") || name.starts_with("debug_assert") || name == "expect")
            && !tokens.get(i + 1).is_some_and(|t| t.text == ".")
            && (language != Language::Rust || tokens.get(i + 1).is_some_and(|t| t.text == "!"))
        {
            result.unsupported = true;
        }
        for expr in expressions {
            let expanded = expand(&expr, &bindings);
            let expanded = if language == Language::Rust {
                rust_asserted_values(&expanded)
            } else {
                expanded
            };
            if !supported(&expanded, language) {
                result.unsupported = true;
            }
            result.inputs.extend(expanded.clone());
            result.expressions.push(expanded);
        }
        // Numeric test inputs must be arguments to calls, not comments, messages or unused locals.
        if !recognised
            && tokens[i].kind == Kind::Ident
            && tokens.get(i + 1).is_some_and(|t| t.text == "(")
        {
            if let Some(last) = close(tokens, i + 1) {
                result
                    .inputs
                    .extend(expand(&tokens[i + 2..last], &bindings));
            }
        }
        i = end + 1;
    }
    result
}
fn simple_path(tokens: &[Token], separator: &str) -> bool {
    !tokens.is_empty()
        && tokens.iter().enumerate().all(|(i, t)| {
            if i % 2 == 0 {
                t.kind == Kind::Ident
            } else {
                t.text == separator
            }
        })
        && tokens.len() % 2 == 1
}
fn inert_argument(tokens: &[Token]) -> bool {
    tokens.len() == 1 && matches!(tokens[0].kind, Kind::Ident | Kind::Number | Kind::String)
        || tokens.len() == 2 && tokens[0].text == "-" && tokens[1].kind == Kind::Number
}
// No argument calls, indexing, operators or macros: those can provide a panic
// or exception before the subject is invoked.
fn direct_call_end(tokens: &[Token], begin: usize, language: Language) -> Option<usize> {
    let open = (begin..tokens.len()).find(|i| tokens[*i].text == "(")?;
    let separator = if language == Language::Rust {
        "::"
    } else {
        "."
    };
    if !simple_path(&tokens[begin..open], separator) {
        return None;
    }
    let end = close(tokens, open)?;
    split_args(&tokens[open + 1..end])
        .iter()
        .all(|arg| inert_argument(arg))
        .then_some(end)
}
fn exception_type(tokens: &[Token]) -> bool {
    simple_path(tokens, ".")
        && tokens.last().is_some_and(|t| {
            t.text
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_uppercase)
        })
}
fn indentation(source: &str, token: &Token) -> usize {
    let line = source[..token.start].rfind('\n').map_or(0, |n| n + 1);
    source[line..token.start].len()
}
fn raises_suite(tokens: &[Token], start: usize, source: &str) -> Option<(usize, usize)> {
    let prefix = ["with", "pytest", ".", "raises", "("];
    if !tokens
        .get(start..start + prefix.len())?
        .iter()
        .zip(prefix)
        .all(|(token, expected)| token.text == expected)
    {
        return None;
    }
    let end = close(tokens, start + 4)?;
    let args = split_args(&tokens[start + 5..end]);
    if args.len() != 1 || !exception_type(args[0]) || tokens.get(end + 1)?.text != ":" {
        return None;
    }
    let begin = end + 2;
    let call_end = direct_call_end(tokens, begin, Language::Python)?;
    let depth = indentation(source, &tokens[start]);
    // Require exactly one complete statement in an indented suite. Reject
    // multiple context managers, aliases, inline suites and extra panic sources.
    if !source[tokens[end + 1].end..tokens[begin].start].contains('\n')
        || indentation(source, &tokens[begin]) <= depth
    {
        return None;
    }
    if let Some(next) = tokens.get(call_end + 1) {
        if !source[tokens[call_end].end..next.start].contains('\n')
            || indentation(source, next) > depth
        {
            return None;
        }
    }
    Some((begin, call_end))
}
// Without type/name resolution, identifiers and constructors may be bindings
// or irrefutable struct patterns. Only literal patterns establish a constraint
// in this bounded analyser; all other patterns retain the unsupported macro.
fn refutable_pattern(tokens: &[Token]) -> bool {
    tokens.len() == 1
        && (matches!(tokens[0].kind, Kind::Number | Kind::String)
            || matches!(tokens[0].text.as_str(), "true" | "false"))
        || tokens.len() == 2 && tokens[0].text == "-" && tokens[1].kind == Kind::Number
}
// Only the scrutinee of matches! supplies relevance, never a decoy pattern name.
fn rust_asserted_values(tokens: &[Token]) -> Vec<Token> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].text == "matches" && tokens.get(i + 1).is_some_and(|t| t.text == "!") {
            if let Some(end) = close(tokens, i + 2) {
                let args = split_args(&tokens[i + 3..end]);
                if args.len() == 2 && refutable_pattern(args[1]) {
                    result.extend(rust_asserted_values(args[0]));
                    i = end + 1;
                    continue;
                }
            }
        }
        result.push(tokens[i].clone());
        i += 1;
    }
    result
}
fn supported_panic_branch(tokens: &[Token], branch: usize) -> bool {
    let Some(open) = (branch + 1..tokens.len()).find(|i| tokens[*i].text == "{") else {
        return false;
    };
    let Some(end) = close(tokens, open) else {
        return false;
    };
    if !tokens.get(open + 1).is_some_and(|t| t.text == "panic")
        || !tokens.get(open + 2).is_some_and(|t| t.text == "!")
    {
        return false;
    }
    let Some(call_end) = close(tokens, open + 3) else {
        return false;
    };
    call_end + 1 == end || call_end + 2 == end && tokens[call_end + 1].text == ";"
}
fn separator() -> Token {
    Token {
        kind: Kind::Symbol,
        text: ",".into(),
        start: 0,
        end: 0,
    }
}
fn join_operands(args: &[&[Token]]) -> Vec<Token> {
    let mut result = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            result.push(separator());
        }
        result.extend_from_slice(arg);
    }
    result
}
fn statement_end(tokens: &[Token], begin: usize, source: &str, language: Language) -> usize {
    let mut i = begin;
    while i < tokens.len() {
        if tokens[i].text == ";"
            || language == Language::Python
                && i > begin
                && source[tokens[i - 1].end..tokens[i].start].contains('\n')
        {
            break;
        }
        if matches!(tokens[i].text.as_str(), "(" | "[" | "{") {
            if let Some(last) = close(tokens, i) {
                i = last;
            } else {
                return tokens.len();
            }
        }
        i += 1;
    }
    i
}
fn expand(tokens: &[Token], bindings: &BTreeMap<String, Vec<Token>>) -> Vec<Token> {
    tokens
        .iter()
        .enumerate()
        .flat_map(|(i, t)| {
            if t.kind == Kind::Ident
                && (i == 0 || !matches!(tokens[i - 1].text.as_str(), "." | "::"))
            {
                bindings
                    .get(&t.text)
                    .cloned()
                    .unwrap_or_else(|| vec![t.clone()])
            } else {
                vec![t.clone()]
            }
        })
        .collect()
}
fn supported(tokens: &[Token], language: Language) -> bool {
    !tokens.is_empty()
        && !tokens.iter().enumerate().any(|(i, t)| {
            matches!(
                t.text.as_str(),
                "=>" | "if" | "else" | "match" | "await" | "async"
            ) || t.text == "!"
                && i > 0
                && tokens[i - 1].kind == Kind::Ident
                && !(language == Language::Rust
                    && matches!(
                        tokens[i - 1].text.as_str(),
                        "vec" | "format" | "env" | "json"
                    ))
                || t.kind == Kind::Symbol
                    && !(language == Language::Rust
                        && matches!(t.text.as_str(), "&" | "?" | "{" | "}" | ":" | ";" | "|"))
                    && !matches!(
                        t.text.as_str(),
                        "(" | ")"
                            | "["
                            | "]"
                            | ","
                            | "."
                            | "::"
                            | "+"
                            | "-"
                            | "*"
                            | "/"
                            | "%"
                            | "<"
                            | ">"
                            | "<="
                            | ">="
                            | "=="
                            | "!="
                            | "==="
                            | "!=="
                            | "!"
                            | "__expect_actual_end__"
                    )
        })
}
fn compact(tokens: &[Token]) -> String {
    tokens.iter().map(|t| t.text.to_ascii_lowercase()).collect()
}
fn placeholder(tokens: &[Token]) -> bool {
    if let Some(end) = tokens
        .iter()
        .position(|t| t.text == "__expect_actual_end__")
    {
        if literal_actual(&tokens[..end]) {
            return true;
        }
        let without_marker: Vec<_> = tokens
            .iter()
            .filter(|t| t.text != "__expect_actual_end__")
            .cloned()
            .collect();
        return placeholder(&without_marker);
    }
    let s = compact(tokens);
    if matches!(
        s.as_str(),
        "true" | "false" | "notfalse" | "nottrue" | "none" | "null" | "undefined"
    ) {
        return true;
    }
    if !tokens.iter().any(|t| {
        t.kind == Kind::Ident
            && !matches!(
                t.text.as_str(),
                "True" | "False" | "true" | "false" | "not" | "None" | "null" | "undefined"
            )
    }) {
        return true;
    }
    for op in ["===", "!==", "==", "!=", ">=", "<=", ">", "<", ","] {
        if let Some((left, right)) = s.split_once(op) {
            if left == right {
                return true;
            }
        }
    }
    let args = split_args(tokens);
    if args.len() == 2 && compact(args[0]) == compact(args[1]) {
        return true;
    }
    false
}
fn literal_actual(mut tokens: &[Token]) -> bool {
    while tokens.first().is_some_and(|t| t.text == "(")
        && close(tokens, 0) == Some(tokens.len() - 1)
    {
        tokens = &tokens[1..tokens.len() - 1];
    }
    tokens.len() == 1
        && (matches!(tokens[0].kind, Kind::Number | Kind::String)
            || matches!(
                tokens[0].text.as_str(),
                "true" | "false" | "null" | "undefined"
            ))
}
fn relevant(tokens: &[Token], expected: &BTreeSet<String>) -> bool {
    let names = tokens
        .iter()
        .filter(|t| matches!(t.kind, Kind::Ident | Kind::String))
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    !terms(&names).is_disjoint(expected)
}
fn numeric_tokens(tokens: &[Token]) -> Vec<f64> {
    tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            if t.kind != Kind::Number {
                return None;
            }
            t.text.parse::<f64>().ok().map(|v| {
                if i > 0 && tokens[i - 1].text == "-" {
                    -v
                } else {
                    v
                }
            })
        })
        .collect()
}
