//! Pure mutation evidence and tier evaluation.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationFile {
    pub schema: String,
    pub engine: String,
    pub links: Vec<LinkRecord>,
    pub signoffs: Vec<Signoff>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkRecord {
    pub requirement: String,
    pub test_id: String,
    pub test_fingerprint: String,
    pub requirement_revision: String,
    pub subject_digest: String,
    pub target: Target,
    pub baseline_passes: u32,
    pub status: LinkStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_string"
    )]
    pub infra_failure: Option<String>,
    pub mutants: Vec<Mutant>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub kind: TargetKind,
    pub name: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Lib,
    Test,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStatus {
    Checked,
    Survived,
    Unsupported,
    Error,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutantOutcome {
    Killed,
    Survived,
    Timeout,
    Unviable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    CmpFlip,
    ArithSwap,
    BoolSwap,
    IntBump,
}
impl Operator {
    fn name(self) -> &'static str {
        match self {
            Self::CmpFlip => "cmp_flip",
            Self::ArithSwap => "arith_swap",
            Self::BoolSwap => "bool_swap",
            Self::IntBump => "int_bump",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mutant {
    pub id: String,
    pub file: String,
    pub function: String,
    pub line: u64,
    pub ordinal: u64,
    pub operator: Operator,
    pub original: String,
    pub replacement: String,
    pub outcome: MutantOutcome,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Signoff {
    Link {
        requirement: String,
        test_id: String,
        test_fingerprint: String,
        requirement_revision: String,
        reason: String,
        signed_by: String,
    },
    Mutant {
        requirement: String,
        test_id: String,
        mutant_id: String,
        test_fingerprint: String,
        requirement_revision: String,
        subject_digest: String,
        reason: String,
        signed_by: String,
    },
}
fn nonempty(s: &str) -> bool {
    !s.trim().is_empty()
}
fn present_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}
// Components are framed by their byte length as a big-endian u64. IDs use
// UTF-8 fields and decimal ordinal. Subject tuples sort lexicographically by
// file, function and source bytes, and each of their components is framed.
fn framed<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_be_bytes());
        h.update(p);
    }
    format!("{:x}", h.finalize())
}
pub fn mutant_id(
    file: &str,
    function: &str,
    operator: Operator,
    original: &str,
    replacement: &str,
    ordinal: u64,
) -> String {
    let ordinal = ordinal.to_string();
    let h = framed([
        file.as_bytes(),
        function.as_bytes(),
        operator.name().as_bytes(),
        original.as_bytes(),
        replacement.as_bytes(),
        ordinal.as_bytes(),
    ]);
    format!("m-{}", &h[..12])
}
pub fn subject_digest(parts: &[(&str, &str, &[u8])]) -> String {
    let mut parts = parts.to_vec();
    parts.sort();
    format!(
        "sha256:{}",
        framed(
            parts
                .iter()
                .flat_map(|(f, n, s)| [f.as_bytes(), n.as_bytes(), *s])
        )
    )
}
impl MutationFile {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let value: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        value.validate()?;
        Ok(value)
    }
    pub fn to_pretty_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut b = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        b.push(b'\n');
        Ok(b)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != "living-spec.mutation/v1" || self.engine != "rust-cargo-v1" {
            return Err("invalid schema or engine".into());
        }
        let mut links = HashSet::new();
        for l in &self.links {
            if !links.insert((&l.requirement, &l.test_id)) {
                return Err("duplicate link".into());
            }
            if [
                &l.requirement,
                &l.test_id,
                &l.test_fingerprint,
                &l.requirement_revision,
                &l.subject_digest,
                &l.target.name,
            ]
            .iter()
            .any(|v| v.is_empty())
            {
                return Err("empty link field".into());
            }
            let mut ids = HashSet::new();
            for m in &l.mutants {
                if m.file.is_empty() || m.function.is_empty() {
                    return Err("empty mutant field".into());
                }
                if m.file.starts_with(['/', '\\'])
                    || m.file.split(['/', '\\']).any(|c| c == "..")
                    || m.file.as_bytes().get(1) == Some(&b':')
                {
                    return Err("invalid path".into());
                }
                if m.id
                    != mutant_id(
                        &m.file,
                        &m.function,
                        m.operator,
                        &m.original,
                        &m.replacement,
                        m.ordinal,
                    )
                    || !ids.insert(&m.id)
                {
                    return Err("invalid or duplicate mutant id".into());
                }
            }
            if l.infra_failure.as_deref().is_some_and(|s| !nonempty(s)) {
                return Err("empty infrastructure failure".into());
            }
            let survivor = l
                .mutants
                .iter()
                .any(|m| m.outcome == MutantOutcome::Survived);
            let timeout = l
                .mutants
                .iter()
                .any(|m| m.outcome == MutantOutcome::Timeout);
            let killed = l.mutants.iter().any(|m| m.outcome == MutantOutcome::Killed);
            let valid = if l.infra_failure.is_some() {
                l.status == LinkStatus::Error
            } else if survivor {
                l.status == LinkStatus::Survived
            } else if timeout {
                l.status == LinkStatus::Error
            } else {
                match l.status {
                    LinkStatus::Checked => l.baseline_passes == 2 && killed,
                    LinkStatus::Survived => false,
                    LinkStatus::Unsupported | LinkStatus::Error => {
                        !(l.baseline_passes == 2 && killed)
                    }
                }
            };
            if !valid {
                return Err("stored status disagrees with evidence".into());
            }
            if matches!(l.status, LinkStatus::Unsupported | LinkStatus::Error)
                && !l.reason.as_deref().is_some_and(nonempty)
            {
                return Err("missing reason".into());
            }
        }
        for s in &self.signoffs {
            let fields: Vec<&str> = match s {
                Signoff::Link {
                    requirement,
                    test_id,
                    test_fingerprint,
                    requirement_revision,
                    reason,
                    signed_by,
                } => vec![
                    requirement,
                    test_id,
                    test_fingerprint,
                    requirement_revision,
                    reason,
                    signed_by,
                ],
                Signoff::Mutant {
                    requirement,
                    test_id,
                    mutant_id,
                    test_fingerprint,
                    requirement_revision,
                    subject_digest,
                    reason,
                    signed_by,
                } => vec![
                    requirement,
                    test_id,
                    mutant_id,
                    test_fingerprint,
                    requirement_revision,
                    subject_digest,
                    reason,
                    signed_by,
                ],
            };
            if fields.iter().any(|s| !nonempty(s)) {
                return Err("missing signoff binding".into());
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct Bindings<'a> {
    pub test_fingerprint: &'a str,
    pub requirement_revision: &'a str,
    pub subject_digest: Option<&'a str>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkState {
    Checked,
    Waived { waived: usize },
    SignedOff,
    Survived { survivors: Vec<Mutant> },
    Stale,
    Error,
    Unsupported,
    NotRun,
}
pub fn evaluate_link(
    record: Option<&LinkRecord>,
    b: &Bindings<'_>,
    signoffs: &[Signoff],
) -> LinkState {
    let Some(r) = record else {
        return LinkState::NotRun;
    };
    let candidate = MutationFile {
        schema: "living-spec.mutation/v1".into(),
        engine: "rust-cargo-v1".into(),
        links: vec![r.clone()],
        signoffs: vec![],
    };
    if candidate.validate().is_err() {
        return LinkState::Error;
    }
    if r.test_fingerprint != b.test_fingerprint || r.requirement_revision != b.requirement_revision
    {
        return LinkState::Stale;
    }
    if matches!(r.status, LinkStatus::Checked | LinkStatus::Survived)
        && b.subject_digest != Some(r.subject_digest.as_str())
    {
        return LinkState::Stale;
    }
    match r.status {
  LinkStatus::Checked => LinkState::Checked,
  LinkStatus::Survived => { let survivors: Vec<_> = r.mutants.iter().filter(|m| m.outcome == MutantOutcome::Survived).collect(); let remaining: Vec<_> = survivors.iter().filter(|m| !signoffs.iter().any(|s| matches!(s, Signoff::Mutant { requirement, test_id, mutant_id, test_fingerprint, requirement_revision, subject_digest, reason, signed_by } if requirement == &r.requirement && test_id == &r.test_id && mutant_id == &m.id && test_fingerprint == b.test_fingerprint && requirement_revision == b.requirement_revision && Some(subject_digest.as_str()) == b.subject_digest && nonempty(reason) && nonempty(signed_by)))).map(|m| (*m).clone()).collect(); if remaining.is_empty() && !survivors.is_empty() { LinkState::Waived { waived: survivors.len() } } else { LinkState::Survived { survivors: survivors.into_iter().cloned().collect() } } },
  LinkStatus::Unsupported | LinkStatus::Error => if signoffs.iter().any(|s| matches!(s, Signoff::Link { requirement, test_id, test_fingerprint, requirement_revision, reason, signed_by } if requirement == &r.requirement && test_id == &r.test_id && test_fingerprint == b.test_fingerprint && requirement_revision == b.requirement_revision && nonempty(reason) && nonempty(signed_by))) { LinkState::SignedOff } else if r.status == LinkStatus::Error { LinkState::Error } else { LinkState::Unsupported },
 }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TierState {
    MutationChecked,
    SignedOff,
    Survived,
    Stale,
    Error,
    Unsupported,
    NotRun,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tier {
    pub state: TierState,
    pub waived: usize,
    pub signed_off: usize,
    pub survivors: Vec<Mutant>,
}
pub fn line_tier(states: &[LinkState], proven: bool) -> Option<Tier> {
    if !proven {
        return None;
    }
    let mut t = Tier {
        state: TierState::MutationChecked,
        waived: 0,
        signed_off: 0,
        survivors: vec![],
    };
    let mut worst = usize::from(states.is_empty());
    for s in states {
        match s {
            LinkState::Checked => (),
            LinkState::Waived { waived } => t.waived += waived,
            LinkState::SignedOff => t.signed_off += 1,
            LinkState::Survived { survivors } => {
                worst = worst.max(5);
                t.survivors.extend(survivors.clone());
            }
            LinkState::Stale => worst = worst.max(4),
            LinkState::Error => worst = worst.max(3),
            LinkState::Unsupported => worst = worst.max(2),
            LinkState::NotRun => worst = worst.max(1),
        }
    }
    t.state = match worst {
        5 => TierState::Survived,
        4 => TierState::Stale,
        3 => TierState::Error,
        2 => TierState::Unsupported,
        1 => TierState::NotRun,
        _ if t.waived > 0 || t.signed_off > 0 => TierState::SignedOff,
        _ => TierState::MutationChecked,
    };
    Some(t)
}
