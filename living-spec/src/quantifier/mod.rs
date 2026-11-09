//! Quantified fixture observations, with bounded execution and explicit coverage.
pub mod annotations;
pub mod catalog;
pub mod check;
pub mod export;
use crate::core::json_input;
pub mod predicate;
pub mod view;
pub use crate::core::quantifier::*;
/// Validate original JSON bytes before any map deserialization discards keys.
pub fn validate_json_input(bytes: &[u8]) -> Result<()> {
    json_input::reject_duplicate_keys(bytes)
        .map_err(|_| Error::new("invalid quantifier JSON; object keys must be unique"))
}
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Path, PathBuf},
};
#[derive(Debug)]
pub struct Error(String);
impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::new("invalid quantifier JSON or unsupported fields")
    }
}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::new(format!(
            "quantifier file operation failed: {}",
            error.kind()
        ))
    }
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone)]
pub struct Loaded {
    pub claims: Vec<Claim>,
    pub fixture_hashes: BTreeMap<String, String>,
}
pub fn hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
pub fn reference_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/quantifier/specs")
}
impl Claim {
    pub fn validate(&self) -> Result<()> {
        if !regex::Regex::new(r"^[A-Z][A-Z0-9]*-[0-9]{3,}$")
            .expect("constant id pattern")
            .is_match(&self.id)
        {
            return Err(Error::new("invalid quantified requirement id"));
        }
        self.quantifier.validate(annotations::FINITE_CAP)
    }
}
pub fn load(path: &Path) -> Result<Loaded> {
    let mut loaded = Loaded {
        claims: vec![],
        fixture_hashes: BTreeMap::new(),
    };
    if path.is_dir() && path.join("spec.md").is_file() {
        let spec_bytes = fs::read(path.join("spec.md"))?;
        let annotation_bytes = fs::read(path.join("annotations.json"))?;
        validate_json_input(&annotation_bytes)?;
        let source =
            String::from_utf8(spec_bytes.clone()).map_err(|_| Error::new("spec must be UTF-8"))?;
        let spec = crate::core::spec::Spec::parse(&source)
            .map_err(|_| Error::new("invalid requirement lines"))?;
        let annotations: crate::core::annotations::Annotations =
            serde_json::from_slice(&annotation_bytes)?;
        if annotations.schema != "living-spec.annotations/v1" {
            return Err(Error::new("unsupported annotations schema"));
        }
        if !annotations.orphans(&spec).is_empty() {
            return Err(Error::new("orphan annotations found"));
        }
        let mut combined = spec_bytes;
        combined.push(0);
        combined.extend(annotation_bytes);
        for line in spec.lines {
            if let Some(section) = annotations
                .requirements
                .get(&line.id)
                .and_then(|a| a.quantifier.as_ref())
            {
                let q = Quantifier::from_section(section)?;
                loaded
                    .fixture_hashes
                    .insert(line.id.clone(), hash(&combined));
                loaded.claims.push(Claim {
                    id: line.id,
                    text: line.text,
                    quantifier: q,
                    expected: None,
                });
            }
        }
    } else {
        let paths = if path.is_dir() {
            let mut files = Vec::new();
            for entry in fs::read_dir(path)? {
                let p = entry?.path();
                if p.extension().is_some_and(|e| e == "json") {
                    files.push(p);
                }
            }
            files.sort();
            files
        } else {
            vec![path.to_owned()]
        };
        for path in paths {
            let bytes = fs::read(path)?;
            validate_json_input(&bytes)?;
            let claim: Claim = serde_json::from_slice(&bytes)?;
            claim.validate()?;
            if loaded
                .fixture_hashes
                .insert(claim.id.clone(), hash(&bytes))
                .is_some()
            {
                return Err(Error::new("duplicate fixture requirement id"));
            }
            loaded.claims.push(claim);
        }
    }
    if loaded.claims.is_empty() {
        return Err(Error::new("no quantified requirements loaded"));
    }
    Ok(loaded)
}
pub fn check_claim(claim: &Claim) -> Result<Report> {
    claim.validate()?;
    if claim.quantifier.domain.is_finite() && claim.quantifier.kind != Kind::InfiniteUniversal {
        check::finite::check(claim)
    } else {
        check::sampled::check(claim)
    }
}
