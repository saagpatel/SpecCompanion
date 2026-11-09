//! Readable edition export for a set of quantified requirement reports.
use super::super::{catalog, view, Loaded, Report, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub catalog_version: String,
    pub fixture_hashes: BTreeMap<String, String>,
    pub seeds: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Edition {
    pub schema: String,
    pub manifest: Manifest,
    pub requirements: Vec<Report>,
}

pub fn build(loaded: &Loaded, reports: Vec<Report>) -> Edition {
    let seeds = reports
        .iter()
        .filter_map(|report| report.coverage.seed.map(|seed| (report.id.clone(), seed)))
        .collect();
    Edition {
        schema: "living-spec.quantifier-edition/v1".to_owned(),
        manifest: Manifest {
            catalog_version: catalog::VERSION.to_owned(),
            fixture_hashes: loaded.fixture_hashes.clone(),
            seeds,
        },
        requirements: reports,
    }
}

pub fn write(edition: &Edition, out_dir: &Path) -> Result<()> {
    let json = view::json(edition)?;
    let markdown = markdown(edition)?;
    fs::create_dir_all(out_dir)?;
    fs::write(out_dir.join("edition.json"), json)?;
    fs::write(out_dir.join("edition.md"), markdown)?;
    Ok(())
}

fn markdown(edition: &Edition) -> Result<String> {
    let mut out = format!(
        "# Quantifier verification edition\n\nCatalog version: `{}`.\n\n## Source manifest\n\nFixture hashes and sampled seeds are listed below.\n\n",
        md(&edition.manifest.catalog_version),
    );
    out.push_str("| Requirement | Fixture hash | Seed |\n| --- | --- | --- |\n");
    for (id, hash) in &edition.manifest.fixture_hashes {
        let seed = edition
            .manifest
            .seeds
            .get(id)
            .map_or_else(|| "not sampled".to_owned(), |seed| seed.to_string());
        out.push_str(&format!("| {} | `{}` | {} |\n", md(id), md(hash), seed));
    }
    out.push_str("\n## Requirements\n\n");
    out.push_str(&view::markdown(&edition.requirements)?);
    view::assert_proof_free(&out)?;
    Ok(out)
}

fn md(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\n' | '\r') {
            escaped.push(' ');
        } else {
            if matches!(
                character,
                '\\' | '|'
                    | '`'
                    | '#'
                    | '>'
                    | '-'
                    | '!'
                    | '['
                    | ']'
                    | '('
                    | ')'
                    | '*'
                    | '_'
                    | '{'
                    | '}'
            ) {
                escaped.push('\\');
            }
            escaped.push(character);
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantifier::{check_claim, load, reference_dir};

    #[test]
    fn edition_serializes_schema_and_manifest_for_reference_set() {
        let loaded = load(&reference_dir()).unwrap();
        let reports = loaded
            .claims
            .iter()
            .map(check_claim)
            .collect::<Result<Vec<_>>>()
            .unwrap();
        let edition = build(&loaded, reports);
        let json = view::json(&edition).unwrap();
        assert!(json.contains("living-spec.quantifier-edition/v1"));
        assert_eq!(edition.requirements.len(), loaded.claims.len());
        assert_eq!(edition.manifest.fixture_hashes.len(), loaded.claims.len());
        assert!(edition
            .requirements
            .iter()
            .any(|report| report.coverage.seed.is_some()));
        let tmp =
            tempfile::tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).unwrap();
        write(&edition, tmp.path()).unwrap();
        let parsed: Edition =
            serde_json::from_slice(&fs::read(tmp.path().join("edition.json")).unwrap()).unwrap();
        assert_eq!(parsed, edition);
        let md = fs::read_to_string(tmp.path().join("edition.md")).unwrap();
        view::assert_proof_free(&md).unwrap();
    }
}
