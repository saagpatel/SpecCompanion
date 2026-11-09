//! Small source lexer, not a language parser. Comments never become evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Ident,
    String,
    Number,
    Symbol,
    Unsupported,
}
#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub kind: Kind,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

pub(crate) fn lex(source: &str, python: bool) -> Vec<Token> {
    scan(source, python, false, false)
}
pub(crate) fn lex_rust(source: &str) -> Vec<Token> {
    scan(source, false, true, false)
}
pub(crate) fn lex_javascript(source: &str) -> Vec<Token> {
    scan(source, false, false, true)
}
fn scan(source: &str, python: bool, rust: bool, javascript: bool) -> Vec<Token> {
    let b = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if (python && b[i] == b'#') || (!python && b[i..].starts_with(b"//")) {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if !python && b[i..].starts_with(b"/*") {
            i += 2;
            if rust {
                let mut depth = 1;
                while i < b.len() && depth > 0 {
                    if b[i..].starts_with(b"/*") {
                        depth += 1;
                        i += 2;
                    } else if b[i..].starts_with(b"*/") {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            } else {
                // JavaScript and TypeScript block comments end at the first */.
                while i < b.len() && !b[i..].starts_with(b"*/") {
                    i += 1;
                }
                if i < b.len() {
                    i += 2;
                }
            }
            continue;
        }
        if javascript && b[i] == b'/' {
            // Slash needs a full JS grammar to distinguish division from a regex in
            // every position. Keep a possible regex opaque and challenge either
            // interpretation, rather than treating its contents as executable code.
            let mut end = i + 1;
            let mut class = false;
            while end < b.len() && !matches!(b[end], b'\n' | b'\r') {
                match b[end] {
                    b'\\' => end = (end + 2).min(b.len()),
                    b'[' => {
                        class = true;
                        end += 1;
                    }
                    b']' => {
                        class = false;
                        end += 1;
                    }
                    b'/' if !class => {
                        end += 1;
                        break;
                    }
                    _ => end += 1,
                }
            }
            // Without a closing delimiter this may be ordinary division. The
            // unsupported slash still prevents acceptance of the surrounding test.
            i = if b.get(end.saturating_sub(1)) == Some(&b'/') {
                end
            } else {
                i + 1
            };
            tokens.push(Token {
                kind: Kind::Unsupported,
                text: source[start..i].into(),
                start,
                end: i,
            });
            continue;
        }
        // Rust raw strings can contain quotes, braces, and fake test declarations.
        if rust && b[i] == b'r' {
            let mut quote = i + 1;
            while b.get(quote) == Some(&b'#') {
                quote += 1;
            }
            if b.get(quote) == Some(&b'"') {
                let suffix = format!("\"{}", "#".repeat(quote - i - 1));
                i = source[quote + 1..]
                    .find(&suffix)
                    .map_or(b.len(), |end| quote + 1 + end + suffix.len());
                tokens.push(Token {
                    kind: Kind::String,
                    text: source[start..i].into(),
                    start,
                    end: i,
                });
                continue;
            }
        }
        let kind = if matches!(b[i], b'\'' | b'"' | b'`') {
            let quote = b[i];
            // A Rust lifetime is not a character literal.
            if rust
                && quote == b'\''
                && b.get(i + 2) != Some(&b'\'')
                && b.get(i + 1)
                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
            {
                i += 1;
                tokens.push(Token {
                    kind: Kind::Symbol,
                    text: "'".into(),
                    start,
                    end: i,
                });
                continue;
            }
            let triple = python && b[i..].starts_with(&[quote, quote, quote]);
            let width = if triple { 3 } else { 1 };
            i += width;
            while i < b.len() {
                if b[i] == b'\\' {
                    i = (i + 2).min(b.len());
                } else if b[i..].starts_with(&vec![quote; width]) {
                    i += width;
                    break;
                } else {
                    i += 1;
                }
            }
            Kind::String
        } else if b[i].is_ascii_alphabetic() || matches!(b[i], b'_' | b'$') {
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'$')) {
                i += 1;
            }
            Kind::Ident
        } else if b[i].is_ascii_digit() {
            i += 1;
            while i < b.len()
                && (b[i].is_ascii_digit()
                    || b[i] == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit))
            {
                i += 1;
            }
            Kind::Number
        } else {
            i += source[i..].chars().next().unwrap().len_utf8();
            if ["==", "!=", ">=", "<=", "=>", "::", "&&", "||"]
                .iter()
                .any(|op| source[start..].starts_with(op))
            {
                i = start
                    + if source[start..].starts_with("===") || source[start..].starts_with("!==") {
                        3
                    } else {
                        2
                    };
            }
            Kind::Symbol
        };
        tokens.push(Token {
            kind,
            text: source[start..i].to_owned(),
            start,
            end: i,
        });
    }
    tokens
}

pub(crate) fn close(tokens: &[Token], open: usize) -> Option<usize> {
    let expected = match tokens.get(open)?.text.as_str() {
        "(" => ")",
        "[" => "]",
        "{" => "}",
        _ => return None,
    };
    let mut i = open + 1;
    while i < tokens.len() {
        if tokens[i].text == expected {
            return Some(i);
        }
        if matches!(tokens[i].text.as_str(), "(" | "[" | "{") {
            i = close(tokens, i)?;
        } else if matches!(tokens[i].text.as_str(), ")" | "]" | "}") {
            return None;
        }
        i += 1;
    }
    None
}

pub(crate) fn split_args(tokens: &[Token]) -> Vec<&[Token]> {
    let mut args = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < tokens.len() {
        if matches!(tokens[i].text.as_str(), "(" | "[" | "{") {
            if let Some(end) = close(tokens, i) {
                i = end;
            }
        } else if tokens[i].text == "," {
            args.push(&tokens[start..i]);
            start = i + 1;
        }
        i += 1;
    }
    if start < tokens.len() {
        args.push(&tokens[start..]);
    }
    args
}
