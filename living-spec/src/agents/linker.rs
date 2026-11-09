use super::{
    syntax::{lex, lex_rust, Kind},
    test_index::{Language, TestEntry, TestIndex},
};
use crate::core::spec::Spec;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub requirement: String,
    pub revision: String,
    pub test_id: String,
    pub fingerprint: String,
    pub rename_hint: String,
    pub score: usize,
    pub explanation: String,
}
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub threshold: usize,
    pub top_k: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            threshold: 2,
            top_k: 2,
        }
    }
}

pub(crate) fn terms(text: &str) -> BTreeSet<String> {
    const STOP: &[&str] = &[
        "the", "a", "an", "and", "or", "to", "of", "in", "is", "are", "with", "for", "on", "it",
        "when", "must", "should", "shall", "this", "that", "be", "as", "at", "by", "from", "test",
        "assert", "expect", "true", "false", "none", "null", "not", "self",
    ];
    let mut words = String::new();
    let mut previous_lower = false;
    for c in text.chars() {
        if c.is_ascii_uppercase() && previous_lower {
            words.push(' ');
        }
        previous_lower = c.is_ascii_lowercase();
        words.push(c);
    }
    words
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase)
        .filter(|s| !STOP.contains(&s.as_str()))
        .map(|s| {
            if s.len() > 4 && s.ends_with("ies") {
                format!("{}y", &s[..s.len() - 3])
            } else if s.len() > 5 && s.ends_with("ing") {
                s[..s.len() - 3].to_owned()
            } else if s.len() > 4 && s.ends_with("ed") {
                s[..s.len() - 2].to_owned()
            } else if s.len() > 3 && s.ends_with('s') && !s.ends_with("ss") {
                s[..s.len() - 1].to_owned()
            } else {
                s
            }
        })
        .collect()
}

/// Only leaf test names, called identifiers and literals participate, never paths/comments/locals.
fn features(entry: &TestEntry) -> [BTreeSet<String>; 3] {
    let name = entry
        .name
        .rsplit("::")
        .next()
        .unwrap()
        .rsplit(" > ")
        .next()
        .unwrap();
    let tokens = if entry.language == Language::Rust {
        lex_rust(&entry.body)
    } else {
        lex(&entry.body, entry.language == Language::Python)
    };
    let mut calls = String::new();
    let mut literals = String::new();
    for (i, token) in tokens.iter().enumerate() {
        if token.kind == Kind::Ident
            && tokens
                .get(i + 1)
                .is_some_and(|t| t.text == "(" || t.text == "!")
        {
            calls.push_str(&token.text);
            calls.push(' ');
        }
        if matches!(token.kind, Kind::String | Kind::Number) {
            literals.push_str(&token.text);
            literals.push(' ');
        }
    }
    [terms(name), terms(&calls), terms(&literals)]
}

pub fn propose(spec: &Spec, index: &TestIndex) -> Vec<Proposal> {
    propose_with_options(spec, index, Options::default())
}
pub fn propose_with_options(spec: &Spec, index: &TestIndex, options: Options) -> Vec<Proposal> {
    let mut result = Vec::new();
    for requirement in &spec.lines {
        let expected = terms(&requirement.text);
        let mut candidates = Vec::new();
        for entry in &index.entries {
            let features = features(entry);
            let mut shared = BTreeSet::new();
            let mut reasons = Vec::new();
            for (found, label) in
                features
                    .iter()
                    .zip(["test name", "called identifiers", "body literals"])
            {
                let matches: Vec<_> = expected.intersection(found).cloned().collect();
                if !matches.is_empty() {
                    reasons.push(format!(
                        "matched {} in {label}",
                        matches
                            .iter()
                            .map(|s| format!("{s:?}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                shared.extend(matches);
            }
            let score = shared.len() + shared.iter().filter(|s| s.parse::<f64>().is_ok()).count();
            if score < options.threshold || shared.is_empty() {
                continue;
            }
            candidates.push(Proposal {
                requirement: requirement.id.clone(),
                revision: requirement.revision.clone(),
                test_id: entry.id.clone(),
                fingerprint: entry.fingerprint.clone(),
                rename_hint: entry.rename_hint.clone(),
                score,
                explanation: reasons.join("; "),
            });
        }
        candidates.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.test_id.cmp(&b.test_id))
        });
        result.extend(candidates.into_iter().take(options.top_k));
    }
    result
}
