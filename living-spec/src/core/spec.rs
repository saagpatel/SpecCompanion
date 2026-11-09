use regex::Regex;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementLine {
    pub id: String,
    pub text: String,
    pub revision: String,
    pub heading_path: Vec<String>,
    pub line_no: usize,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spec {
    pub lines: Vec<RequirementLine>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecError {
    DuplicateId { id: String, line_no: usize },
    MalformedId { line_no: usize },
}
impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId { id, line_no } => write!(f, "line {line_no}: duplicate id {id}"),
            Self::MalformedId { line_no } => write!(f, "line {line_no}: malformed bracket id"),
        }
    }
}
impl std::error::Error for SpecError {}
impl Spec {
    /// Parse bracketed IDs at the start of Markdown list items.
    pub fn parse(source: &str) -> Result<Self, SpecError> {
        let ids = Regex::new(r"^[A-Z][A-Z0-9]*-[0-9]{3,}$").unwrap();
        let items = Regex::new(r"^\s*(?:[-+*]|[0-9]+[.)])\s+(.*)$").unwrap();
        let heading = Regex::new(r"^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$").unwrap();
        let mut result = Self::default();
        let mut seen = BTreeSet::new();
        let mut headings: Vec<(usize, String)> = Vec::new();
        let mut fence: Option<(char, usize)> = None;
        for (index, line) in source.lines().enumerate() {
            let line_no = index + 1;
            let trimmed = line.trim_start();
            if let Some(ch @ ('`' | '~')) = trimmed.chars().next() {
                let count = trimmed.chars().take_while(|c| *c == ch).count();
                if count >= 3 {
                    match fence {
                        None => fence = Some((ch, count)),
                        Some((open, n))
                            if open == ch && count >= n && trimmed[count..].trim().is_empty() =>
                        {
                            fence = None
                        }
                        _ => {}
                    }
                    continue;
                }
            }
            if fence.is_some() {
                continue;
            }
            if let Some(c) = heading.captures(line) {
                let level = c[1].len();
                while headings.last().is_some_and(|(n, _)| *n >= level) {
                    headings.pop();
                }
                headings.push((level, c[2].to_owned()));
                continue;
            }
            let Some(c) = items.captures(line) else {
                continue;
            };
            let item = &c[1];
            if !item.starts_with('[') {
                continue;
            }
            let Some(end) = item.find(']') else {
                return Err(SpecError::MalformedId { line_no });
            };
            let id = &item[1..end];
            if !ids.is_match(id) || !item[end + 1..].starts_with(char::is_whitespace) {
                return Err(SpecError::MalformedId { line_no });
            }
            if !seen.insert(id.to_owned()) {
                return Err(SpecError::DuplicateId {
                    id: id.to_owned(),
                    line_no,
                });
            }
            result.lines.push(RequirementLine {
                id: id.to_owned(),
                text: item[end + 1..]
                    .strip_prefix(' ')
                    .unwrap_or(&item[end + 1..])
                    .trim_end()
                    .to_owned(),
                revision: revision(
                    item[end + 1..]
                        .strip_prefix(' ')
                        .unwrap_or(&item[end + 1..]),
                ),
                heading_path: headings.iter().map(|(_, h)| h.clone()).collect(),
                line_no,
            });
        }
        Ok(result)
    }
}

/// Hash requirement text preserving punctuation and interior whitespace.
pub fn revision(text: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(text.trim_end().as_bytes()))
}
