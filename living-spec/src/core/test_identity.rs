use super::annotations::Link;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    Failed,
    Errored,
    Skipped,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestOutcome {
    pub test_id: String,
    pub status: TestStatus,
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_message: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestIdentity {
    pub fingerprint: String,
    #[serde(default)]
    pub rename_hint: Option<String>,
}
pub type Fingerprints = BTreeMap<String, TestIdentity>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Exact,
    Changed,
    Renamed { new_id: String },
    Unresolved,
}
/// Hash source bytes, trimming only trailing whitespace on each line.
pub fn fingerprint(source: &str) -> String {
    let normalized = source
        .split('\n')
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    format!("sha256:{:x}", Sha256::digest(normalized.as_bytes()))
}
/// Normalize only a recognized test name, preserving the rest of the source.
/// Unsupported signatures have no hint: guessing a body boundary can hide edits.
pub fn rename_hint(source: &str) -> Option<String> {
    use regex::Regex;
    use std::sync::LazyLock;

    // Deliberately bounded headers: complex attributes, defaults, return types,
    // destructuring, and expression callbacks need a source-aware indexer.
    static HEADER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(concat!(
            r#"(?x)\A\s*(?:"#,
            r#"(?:\#\[[^\[\]"'\r\n]*\]\s*)*(?:async\s+)?fn\s+(?P<rust>[A-Za-z_]\w*)\s*\(\s*\)\s*\{"#,
            r#"|(?:@[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*\s*\n\s*)*(?:async\s+)?def\s+(?P<python>[A-Za-z_]\w*)\s*\(\s*(?:[A-Za-z_]\w*\s*(?:,\s*[A-Za-z_]\w*\s*)*,?\s*)?\)\s*:"#,
            r#"|(?:it|test)(?:\.(?:only|skip))?\s*\(\s*(?P<js>"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')\s*,\s*(?:async\s+)?(?:\(\s*(?:[A-Za-z_]\w*\s*)?\)\s*=>|function(?:\s+[A-Za-z_]\w*)?\s*\(\s*\))\s*\{"#,
            r#")"#,
        ))
        .expect("bounded declaration regex")
    });
    let captures = HEADER.captures(source)?;
    let header = captures.get(0)?;
    if source[header.end()..].trim().is_empty() {
        return None;
    }
    let name = captures
        .name("rust")
        .or_else(|| captures.name("python"))
        .or_else(|| captures.name("js"))?;
    let mut normalized = source.to_owned();
    normalized.replace_range(name.range(), "<test-name>");
    Some(fingerprint(&normalized))
}
/// Compatibility entry point for digest-only callers. Renames require hints.
pub fn resolve_link(
    link: &Link,
    outcomes: &[TestOutcome],
    fingerprints_by_id: &BTreeMap<String, String>,
) -> Resolution {
    let identities = fingerprints_by_id
        .iter()
        .map(|(id, digest)| {
            (
                id.clone(),
                TestIdentity {
                    fingerprint: digest.clone(),
                    rename_hint: None,
                },
            )
        })
        .collect();
    resolve_link_with_identities(link, outcomes, &identities)
}
/// An index entry can resolve a test even when that case was not executed.
/// Missing current digests never establish unchanged source identity.
pub fn resolve_link_with_identities(
    link: &Link,
    outcomes: &[TestOutcome],
    identities: &Fingerprints,
) -> Resolution {
    if let Some(current) = identities.get(&link.test_id) {
        return if current.fingerprint == link.fingerprint {
            Resolution::Exact
        } else {
            Resolution::Changed
        };
    }
    if outcomes.iter().any(|o| o.test_id == link.test_id) {
        return Resolution::Changed;
    }
    let Some(hint) = &link.rename_hint else {
        return Resolution::Unresolved;
    };
    // A declaration-only snippet has no body evidence for a safe rename.
    if hint == &fingerprint("") {
        return Resolution::Unresolved;
    }
    let case = |id: &str| id.rsplit_once('#').map(|(_, case)| case.to_owned());
    let matches = identities
        .iter()
        .filter(|(id, current)| {
            current.rename_hint.as_ref() == Some(hint) && case(id) == case(&link.test_id)
        })
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [id] => Resolution::Renamed {
            new_id: (*id).clone(),
        },
        _ => Resolution::Unresolved,
    }
}

/// Read either legacy fingerprint digests or complete source identities.
pub fn load_fingerprints(source: &str) -> Result<Fingerprints, Box<dyn std::error::Error>> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Digest(String),
        Identity(TestIdentity),
    }
    let entries: std::collections::BTreeMap<String, Entry> = serde_json::from_str(source)?;
    let identities: Fingerprints = entries
        .into_iter()
        .map(|(id, entry)| {
            (
                id,
                match entry {
                    Entry::Digest(fingerprint) => TestIdentity {
                        fingerprint,
                        rename_hint: None,
                    },
                    Entry::Identity(identity) => identity,
                },
            )
        })
        .collect();
    if identities.iter().any(|(id, entry)| {
        id.is_empty()
            || entry.fingerprint.is_empty()
            || entry.rename_hint.as_ref().is_some_and(|s| s.is_empty())
    }) {
        return Err("empty fingerprint identity".into());
    }
    Ok(identities)
}
