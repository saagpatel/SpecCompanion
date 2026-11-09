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
    if let Some(shape) = early_exit(&tokens, entry.language) {
        notes.push(format!("unsupported test shape: {shape}"));
    }
    if entry.unknown_cases {
        notes.push("unsupported test shape: parameterized case inventory is unknown".into());
    }
    if entry.language == Language::Python && entry.decorated && !parametrize_only(&entry.source) {
        notes.push("unsupported test shape: Python decorator".into());
    }
    if entry.language == Language::Rust && entry.unsupported_attributes {
        notes.push("unsupported test shape: Rust attribute".into());
    }
    for shape in &analysis.unsupported {
        notes.push(format!("unsupported test shape: {shape}"));
    }
    if analysis.expressions.is_empty() {
        notes.push("unsupported test shape: no supported assertion connects a computed value to an independent expected value (placeholder assertions are not evidence).".into());
    } else {
        let expected = terms(&requirement.text);
        if !analysis.expressions.iter().any(|e| relevant(e, &expected)) {
            notes.push("unsupported test shape: Assertions check unrelated values; no asserted expression derives from requirement terms.".into());
        }
        let numbers = numeric_tokens(&lex(&requirement.text, false));
        if numbers.iter().any(|n| {
            !analysis.expressions.iter().any(|expression| {
                relevant(expression, &expected)
                    && numeric_tokens(expression)
                        .iter()
                        .any(|value| n == value || (n - value).abs() == 1.0)
            })
        }) {
            notes.push("unsupported test shape: A required number is absent from assertions and call inputs, including adjacent values.".into());
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
    unsupported: Vec<String>,
}

fn early_exit(tokens: &[Token], language: Language) -> Option<&'static str> {
    for (i, token) in tokens.iter().enumerate() {
        if token.kind == Kind::Ident && matches!(token.text.as_str(), "return" | "throw" | "raise")
        {
            return Some("early return or exception can skip assertions");
        }
        if token.text == "panic" && tokens.get(i + 1).is_some_and(|t| t.text == "!") {
            return Some("panic! outside the supported expected-panic call shape");
        }
        let is_process_exit = language == Language::JavaScript
            && tokens.get(i..i + 3).is_some_and(|path| {
                path[0].text == "process" && path[1].text == "." && path[2].text == "exit"
            });
        let is_rust_process_exit = language == Language::Rust
            && tokens.get(i..i + 5).is_some_and(|path| {
                path[0].text == "std"
                    && path[1].text == "::"
                    && path[2].text == "process"
                    && path[3].text == "::"
                    && path[4].text == "exit"
            });
        if is_process_exit || is_rust_process_exit {
            return Some("process exit can skip assertions");
        }
    }
    None
}

// Consume whole statements, never search an unknown token tree for assertions.
fn assertions(tokens: &[Token], source: &str, language: Language, should_panic: bool) -> Analysis {
    let mut result = Analysis::default();
    let mut bindings = BTreeMap::new();
    let mut i = 0;
    let mut panic_calls = 0;
    while i < tokens.len() {
        if tokens[i].text == ";" {
            i += 1;
            continue;
        }
        if language == Language::Python && tokens[i].text == "with" && !should_panic {
            if let Some((begin, end)) = raises_suite(tokens, i, source) {
                let value = expand(&tokens[begin..=end], &bindings);
                if valid_expr(&value, language)
                    && computed(&value, language)
                    && direct_call_end(&value, 0, language) == Some(value.len() - 1)
                {
                    result.expressions.push(value);
                } else {
                    result.unsupported.push("pytest.raises subject".into());
                }
                i = end + 1;
                continue;
            }
            result
                .unsupported
                .push("Python context manager suite".into());
            break;
        }
        let end = statement_end(tokens, i, source, language);
        let statement = &tokens[i..end];
        if statement.is_empty() {
            break;
        }
        let binding_prefix = match language {
            Language::Rust if statement[0].text == "let" => {
                Some(1 + usize::from(statement.get(1).is_some_and(|t| t.text == "mut")))
            }
            Language::JavaScript if matches!(statement[0].text.as_str(), "let" | "const") => {
                Some(1)
            }
            Language::Python if statement.get(1).is_some_and(|t| t.text == "=") => Some(0),
            _ => None,
        };
        if let Some(name) = binding_prefix {
            if statement.get(name).is_some_and(|t| t.kind == Kind::Ident)
                && !(language == Language::Rust && statement[name].text == "_")
                && statement.get(name + 1).is_some_and(|t| t.text == "=")
                && valid_expr(&statement[name + 2..], language)
                && (!should_panic
                    || panic_calls == 0
                        && inert_argument(&expand(&statement[name + 2..], &bindings)))
            {
                let value = expand(&statement[name + 2..], &bindings);
                // A bound callee can replace the code under test with a constant or closure.
                bindings.insert(statement[name].text.clone(), value);
            } else {
                result.unsupported.push(
                    "binding (requires one unannotated identifier and a supported expression)"
                        .into(),
                );
            }
        } else if should_panic {
            panic_calls += 1;
            if direct_call_end(statement, 0, language) == Some(statement.len() - 1) {
                let value = expand(statement, &bindings);
                if valid_expr(&value, language)
                    && computed(&value, language)
                    && direct_call_end(&value, 0, language) == Some(value.len() - 1)
                {
                    result.expressions.push(value);
                } else {
                    result.unsupported.push("should_panic subject".into());
                }
            } else {
                result
                    .unsupported
                    .push("should_panic requires exactly one direct call".into());
            }
        } else {
            match assertion(statement, &bindings, language) {
                Ok(Some(value)) => {
                    result.expressions.push(value);
                }
                Ok(None) => (), // Supported syntax, but no computed/independent assertion.
                Err(what) => result.unsupported.push(what.into()),
            }
        }
        i = end + usize::from(tokens.get(end).is_some_and(|t| t.text == ";"));
    }
    if should_panic && panic_calls != 1 {
        result
            .unsupported
            .push("should_panic requires exactly one direct call".into());
    }
    result
}

fn assertion(
    tokens: &[Token],
    bindings: &BTreeMap<String, Vec<Token>>,
    language: Language,
) -> Result<Option<Vec<Token>>, &'static str> {
    let name = tokens[0].text.as_str();
    let operands;
    let mut actual_only = false;
    match language {
        Language::Rust
            if matches!(name, "assert" | "assert_eq" | "assert_ne")
                && tokens.get(1).is_some_and(|t| t.text == "!") =>
        {
            if close(tokens, 2) != Some(tokens.len() - 1) {
                return Err("Rust assertion macro");
            }
            let args = split_args(&tokens[3..tokens.len() - 1]);
            let operand_count = if name == "assert" { 1 } else { 2 };
            if args
                .iter()
                .skip(operand_count)
                .any(|arg| !valid_expr(&expand(arg, bindings), language))
            {
                return Err("unsupported assertion message expression");
            }
            if name == "assert" {
                let value = args.first().ok_or("empty assert!")?;
                if let Some(pair) = comparison(value) {
                    operands = pair;
                } else if value.first().is_some_and(|t| t.text == "matches")
                    && value.get(1).is_some_and(|t| t.text == "!")
                    && close(value, 2) == Some(value.len() - 1)
                {
                    let parts = split_args(&value[3..value.len() - 1]);
                    if parts.len() != 2 || !refutable_pattern(parts[1]) {
                        return Err("irrefutable or unsupported matches! pattern");
                    }
                    let actual = expand(parts[0], bindings);
                    return proof_unary(actual, language);
                } else if value.len() >= 4
                    && value[value.len() - 4].text == "."
                    && matches!(
                        value[value.len() - 3].text.as_str(),
                        "is_ok" | "is_err" | "is_some" | "is_none"
                    )
                    && value[value.len() - 2].text == "("
                    && value[value.len() - 1].text == ")"
                {
                    return proof_unary(expand(&value[..value.len() - 4], bindings), language);
                } else {
                    return Err(
                        "assert! requires comparison, result predicate, or refutable matches!",
                    );
                }
            } else {
                if args.len() < 2 {
                    return Err("Rust assertion operands");
                }
                operands = (args[0], args[1]);
            }
        }
        Language::Python if name == "assert" => {
            let args = split_args(&tokens[1..]);
            if args.len() > 2
                || args
                    .iter()
                    .skip(1)
                    .any(|arg| !valid_expr(&expand(arg, bindings), language))
            {
                return Err("unsupported Python assertion message");
            }
            operands = comparison(args.first().ok_or("empty Python assert")?)
                .ok_or("Python assert requires comparison")?;
        }
        Language::JavaScript
            if name == "expect" && tokens.get(1).is_some_and(|t| t.text == "(") =>
        {
            let actual_end = close(tokens, 1).ok_or("expect operand")?;
            let matcher = actual_end + 2;
            if tokens.get(actual_end + 1).is_none_or(|t| t.text != ".")
                || close(tokens, matcher + 1) != Some(tokens.len() - 1)
            {
                return Err("expect matcher");
            }
            let actual = &tokens[2..actual_end];
            if split_args(actual).len() != 1 {
                return Err("expect requires exactly one actual expression");
            }
            let expected = split_args(&tokens[matcher + 2..tokens.len() - 1]);
            let matcher_name = tokens.get(matcher).ok_or("expect matcher")?.text.as_str();
            if matches!(matcher_name, "toBeTruthy" | "toBeFalsy") {
                if !expected.is_empty()
                    || !(actual.len() == 1 && actual[0].kind == Kind::Ident || whole_call(actual))
                {
                    return Err("truthiness requires a call or bound identifier");
                }
                return proof_unary(expand(actual, bindings), language);
            }
            if !matches!(
                matcher_name,
                "toBe"
                    | "toEqual"
                    | "toStrictEqual"
                    | "toThrow"
                    | "toBeGreaterThan"
                    | "toBeGreaterThanOrEqual"
                    | "toBeLessThan"
                    | "toBeLessThanOrEqual"
            ) || expected.len() != 1
            {
                return Err("unsupported expect matcher or arity");
            }
            operands = (actual, expected[0]);
            actual_only = true;
        }
        _ => {
            if whole_call(tokens) && valid_expr(tokens, language) {
                return Ok(None);
            }
            return Err("statement is not a supported binding, call, or assertion");
        }
    }
    let left = expand(operands.0, bindings);
    let right = expand(operands.1, bindings);
    if !valid_expr(&left, language) || !valid_expr(&right, language) {
        return Err("assertion expression");
    }
    if language == Language::Python && (container(&left) || container(&right))
        || language == Language::JavaScript && container(&left)
    {
        return Err("literal container in assertion");
    }
    if compact(&left) == compact(&right)
        || shared_value(&left, &right)
        || !(computed(&left, language) || !actual_only && computed(&right, language))
    {
        return Ok(None);
    }
    Ok(Some(join_operands(&[&left, &right])))
}

fn proof_unary(value: Vec<Token>, language: Language) -> Result<Option<Vec<Token>>, &'static str> {
    if !valid_expr(&value, language) || container(&value) {
        return Err("assertion expression or truthy container");
    }
    Ok(computed(&value, language).then_some(value))
}
fn unparen(mut tokens: &[Token]) -> &[Token] {
    while tokens.first().is_some_and(|t| t.text == "(")
        && close(tokens, 0) == Some(tokens.len() - 1)
        && split_args(&tokens[1..tokens.len() - 1]).len() == 1
        && !tokens.get(tokens.len() - 2).is_some_and(|t| t.text == ",")
    {
        tokens = &tokens[1..tokens.len() - 1];
    }
    tokens
}
fn comparison(tokens: &[Token]) -> Option<(&[Token], &[Token])> {
    let tokens = unparen(tokens);
    let mut i = 0;
    let mut found = None;
    while i < tokens.len() {
        if tokens[i].kind == Kind::Ident {
            if let Some(open) = call_open(tokens, i, Language::Rust).filter(|open| *open > i + 1) {
                i = open;
            }
        }
        if matches!(tokens[i].text.as_str(), "(" | "[" | "{") {
            i = close(tokens, i)?;
        } else if matches!(
            tokens[i].text.as_str(),
            "==" | "!=" | "<" | ">" | "<=" | ">=" | "is" | "in"
        ) {
            if found.is_some() {
                return None;
            }
            found = Some(i);
        }
        i += 1;
    }
    let op = found?;
    (op > 0 && op + 1 < tokens.len()).then_some((&tokens[..op], &tokens[op + 1..]))
}
fn container(tokens: &[Token]) -> bool {
    let tokens = unparen(tokens);
    matches!(tokens.first().map(|t| t.text.as_str()), Some("[" | "{"))
        || tokens.first().is_some_and(|t| t.text == "(")
            && close(tokens, 0) == Some(tokens.len() - 1)
}
fn whole_call(tokens: &[Token]) -> bool {
    let tokens = unparen(tokens);
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].kind == Kind::Ident {
            if let Some(open) = call_open(tokens, i, Language::Rust).filter(|open| *open > i + 1) {
                i = open;
            }
        }
        let token = &tokens[i];
        if matches!(token.text.as_str(), "(" | "[" | "{") {
            let Some(end) = close(tokens, i) else {
                return false;
            };
            if end == tokens.len() - 1 {
                return token.text == "("
                    && i > 0
                    && (tokens[i - 1].kind == Kind::Ident || tokens[i - 1].text == ">");
            }
            i = end;
        } else if token.kind == Kind::Symbol && !matches!(token.text.as_str(), "." | "::") {
            return false;
        }
        i += 1;
    }
    false
}
// Provenance uses the original source offsets. Expanding two aliases of the same
// value preserves their shared call tokens, even when their projections differ.
fn shared_value(left: &[Token], right: &[Token]) -> bool {
    let left_calls = value_calls(left);
    let right_calls = value_calls(right);
    let left_sites = call_sites(left);
    let right_sites = call_sites(right);
    left_calls.iter().any(|a| right_sites.contains(a))
        || right_calls.iter().any(|a| left_sites.contains(a))
        || contains_expression(left, right)
        || contains_expression(right, left)
}
fn call_sites(tokens: &[Token]) -> BTreeSet<usize> {
    tokens
        .iter()
        .enumerate()
        .filter(|(i, t)| t.kind == Kind::Ident && call_open(tokens, *i, Language::Rust).is_some())
        .map(|(_, t)| t.start)
        .collect()
}
fn contains_expression(haystack: &[Token], needle: &[Token]) -> bool {
    needle.len() > 1
        && haystack
            .windows(needle.len())
            .any(|part| compact(part) == compact(needle))
}
fn value_calls(tokens: &[Token]) -> Vec<usize> {
    let mut calls = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].kind == Kind::Ident {
            if let Some(open) = call_open(tokens, i, Language::Rust).filter(|open| *open > i + 1) {
                if let Some(end) = close(tokens, open) {
                    calls.push(tokens[i].start);
                    i = end + 1;
                    continue;
                }
            }
        }
        if tokens[i].text == "(" {
            if let Some(end) = close(tokens, i) {
                if i > 0 && tokens[i - 1].kind == Kind::Ident {
                    calls.push(tokens[i - 1].start);
                } else {
                    calls.extend(value_calls(&tokens[i + 1..end]));
                }
                i = end;
            }
        }
        i += 1;
    }
    calls
}
fn computed(tokens: &[Token], language: Language) -> bool {
    let tokens = unparen(tokens);
    if container(tokens) {
        return false;
    }
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].kind == Kind::Ident {
            let name = tokens[i].text.as_str();
            if tokens.get(i + 1).is_some_and(|t| t.text == "!") {
                // Literal construction macros supply no computed provenance, even
                // when their token trees contain calls.
                let Some(end) = close(tokens, i + 2) else {
                    return false;
                };
                i = end + 1;
                continue;
            }
            if let Some(open) = call_open(tokens, i, language) {
                let Some(end) = close(tokens, open) else {
                    return false;
                };
                let constructor = matches!(
                    name,
                    "Some"
                        | "Ok"
                        | "Err"
                        | "None"
                        | "String"
                        | "Vec"
                        | "bool"
                        | "int"
                        | "float"
                        | "complex"
                        | "str"
                        | "bytes"
                        | "bytearray"
                        | "list"
                        | "tuple"
                        | "dict"
                        | "set"
                        | "frozenset"
                        | "object"
                        | "Boolean"
                        | "Number"
                        | "Array"
                        | "Object"
                        | "Symbol"
                        | "BigInt"
                        | "RegExp"
                ) || language == Language::Rust
                    && i >= 2
                    && tokens[i - 1].text == "::"
                    && matches!(
                        name,
                        "new"
                            | "from"
                            | "from_iter"
                            | "from_ref"
                            | "default"
                            | "with_capacity"
                            | "from_secs"
                            | "from_millis"
                    )
                    && matches!(
                        tokens[i - 2].text.as_str(),
                        "String"
                            | "Vec"
                            | "BTreeMap"
                            | "BTreeSet"
                            | "HashMap"
                            | "HashSet"
                            | "Default"
                            | "Duration"
                            | "Path"
                            | "PathBuf"
                            | "Box"
                            | "Rc"
                            | "Arc"
                    )
                    || language == Language::Rust
                        && name.as_bytes().first().is_some_and(u8::is_ascii_uppercase);
                let literal_length =
                    matches!(name, "len" | "is_empty") && container(&tokens[open + 1..end]);
                if constructor || literal_length {
                    i = end + 1;
                    continue;
                }
                if i > 0 && tokens[i - 1].text == "." {
                    let receiver = &tokens[..i - 1];
                    if computed(receiver, language) {
                        return true;
                    }
                    // A literal or literal-constructor receiver cannot become
                    // code under test merely by adding a method call.
                    if receiver
                        .iter()
                        .any(|t| matches!(t.text.as_str(), "(" | "!" | "[" | "{"))
                        || !receiver.iter().any(|t| {
                            t.kind == Kind::Ident
                                && !matches!(
                                    t.text.as_str(),
                                    "true"
                                        | "false"
                                        | "True"
                                        | "False"
                                        | "None"
                                        | "null"
                                        | "undefined"
                                )
                        })
                    {
                        i = end + 1;
                        continue;
                    }
                }
                return true;
            }
        }
        i += 1;
    }
    false
}
fn call_open(tokens: &[Token], name: usize, language: Language) -> Option<usize> {
    if tokens.get(name + 1)?.text == "(" {
        return Some(name + 1);
    }
    if language != Language::Rust
        || tokens.get(name + 1)?.text != "::"
        || tokens.get(name + 2)?.text != "<"
    {
        return None;
    }
    let mut depth = 1;
    let mut i = name + 3;
    while i < tokens.len() {
        match tokens[i].text.as_str() {
            "<" => depth += 1,
            ">" => depth -= 1,
            _ => (),
        }
        if depth == 0 {
            return tokens.get(i + 1).filter(|t| t.text == "(").map(|_| i + 1);
        }
        i += 1;
    }
    None
}
// Expressions are inert syntax plus ordinary calls. Control flow, assignment,
// closures and unknown macros never become supported by nesting inside a let.
fn valid_expr(tokens: &[Token], language: Language) -> bool {
    !tokens.is_empty()
        && !tokens.iter().enumerate().any(|(i, t)| {
            t.kind == Kind::Unsupported
                || language != Language::Rust && t.text == "?"
                || t.kind == Kind::Ident
                    && matches!(
                        t.text.as_str(),
                        "if" | "else"
                            | "match"
                            | "for"
                            | "while"
                            | "loop"
                            | "fn"
                            | "def"
                            | "lambda"
                            | "function"
                            | "return"
                            | "yield"
                            | "await"
                            | "async"
                            | "and"
                            | "or"
                            | "not"
                            | "try"
                            | "catch"
                            | "unsafe"
                            | "move"
                            | "break"
                            | "continue"
                            | "throw"
                            | "delete"
                            | "typeof"
                            | "void"
                            | "class"
                            | "with"
                            | "raise"
                            | "pass"
                    )
                || t.kind == Kind::Symbol
                    && !matches!(
                        t.text.as_str(),
                        "(" | ")"
                            | "["
                            | "]"
                            | "{"
                            | "}"
                            | ","
                            | "."
                            | "::"
                            | ":"
                            | "&"
                            | "?"
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
                    )
                || t.text == "!"
                    && i > 0
                    && tokens[i - 1].kind == Kind::Ident
                    && !(language == Language::Rust
                        && matches!(
                            tokens[i - 1].text.as_str(),
                            "vec" | "format" | "json" | "env"
                        ))
                || t.kind == Kind::String
                    && (t.text.contains("${")
                        || t.text.starts_with("\"\"\"")
                        || t.text.starts_with("'''"))
                || t.kind == Kind::Ident
                    && matches!(t.text.as_str(), "f" | "r" | "b" | "fr" | "rf")
                    && tokens.get(i + 1).is_some_and(|t| t.kind == Kind::String)
        })
}
fn parametrize_only(source: &str) -> bool {
    let tokens = lex(source, true);
    let mut i = 0;
    while tokens.get(i).is_some_and(|t| t.text == "@") {
        let prefix = ["@", "pytest", ".", "mark", ".", "parametrize", "("];
        if tokens
            .get(i..i + prefix.len())
            .is_none_or(|ts| !ts.iter().zip(prefix).all(|(t, s)| t.text == s))
        {
            return false;
        }
        let Some(end) = close(&tokens, i + 6) else {
            return false;
        };
        i = end + 1;
    }
    tokens.get(i).is_some_and(|t| t.text == "def")
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
// No bindings, guards or alternatives: a literal, range, or qualified enum
// variant (plus the standard Option/Result variants) constrains the scrutinee.
fn refutable_pattern(tokens: &[Token]) -> bool {
    let literal = |tokens: &[Token]| {
        tokens.len() == 1
            && (matches!(tokens[0].kind, Kind::Number | Kind::String)
                || matches!(tokens[0].text.as_str(), "true" | "false"))
            || tokens.len() == 2 && tokens[0].text == "-" && tokens[1].kind == Kind::Number
    };
    if literal(tokens) {
        return true;
    }
    if let Some(dots) = tokens
        .windows(2)
        .position(|ts| ts[0].text == "." && ts[1].text == ".")
    {
        let end = dots + 2 + usize::from(tokens.get(dots + 2).is_some_and(|t| t.text == "="));
        return (dots == 0 || literal(&tokens[..dots]))
            && (end == tokens.len() || literal(&tokens[end..]))
            && (dots > 0 || end < tokens.len());
    }
    let open = tokens
        .iter()
        .position(|t| t.text == "(")
        .unwrap_or(tokens.len());
    let path = &tokens[..open];
    let variant = simple_path(path, "::")
        && (path.len() >= 3
            && path.last().is_some_and(|t| {
                t.text
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_uppercase)
            })
            || path.len() == 1 && matches!(path[0].text.as_str(), "Some" | "None" | "Ok" | "Err"));
    variant
        && (open == tokens.len()
            || close(tokens, open) == Some(tokens.len() - 1)
                && split_args(&tokens[open + 1..tokens.len() - 1])
                    .iter()
                    .all(|arg| arg.len() == 1 && arg[0].text == "_" || literal(arg)))
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
fn compact(tokens: &[Token]) -> String {
    tokens.iter().map(|t| t.text.to_ascii_lowercase()).collect()
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
