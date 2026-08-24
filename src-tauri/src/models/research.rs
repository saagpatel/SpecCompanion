use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::models::report::AlignmentClassification;

pub const CONTRACT_SCHEMA_VERSION: &str = "evidence-centered.research-package.v1";
pub const CONTRACT_SCHEMA_SHA256: &str =
    "sha256:ab1702392cdd3c3b0d465f52de5114d5f4aad8e1e47730c10fca53fc7622360c";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchClaimState {
    Supported,
    Weakened,
    Contested,
    Contradicted,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ClaimQualification {
    pub claim_id: String,
    pub state: ResearchClaimState,
    pub excluded_evidence: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ConclusionQualification {
    pub conclusion_id: String,
    pub state: ResearchClaimState,
    pub referenced_claim_states: BTreeMap<String, ResearchClaimState>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AdapterLoss {
    pub path: String,
    pub reason: String,
    pub retained_in_canonical_package: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResearchPackageImport {
    pub package_id: String,
    pub revision_id: String,
    pub schema_digest: String,
    pub canonical_package: Value,
    pub qualification: Vec<ClaimQualification>,
    pub conclusion_qualification: Vec<ConclusionQualification>,
    pub alignment_projection: BTreeMap<String, AlignmentClassification>,
    pub losses: Vec<AdapterLoss>,
}

pub fn import_research_package(raw: &str) -> Result<ResearchPackageImport, String> {
    let package: Value = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    validate_package(&package)?;
    let package_id = text(&package, "package_id")?.to_string();
    let revision_id = text(&package, "revision_id")?.to_string();
    let qualification = qualify_claims(&package)?;
    let conclusion_qualification = qualify_conclusions(&package, &qualification)?;
    let alignment_projection = qualification
        .iter()
        .map(|claim| {
            (
                claim.claim_id.clone(),
                map_claim_state_to_alignment(&claim.state),
            )
        })
        .collect();
    let losses = qualification
        .iter()
        .map(|claim| AdapterLoss {
            path: format!("claims/{}", claim.claim_id),
            reason: "research qualification is not native requirement execution proof; VERIFIED remains gated by SpecCompanion evidence".into(),
            retained_in_canonical_package: true,
        })
        .chain(conclusion_qualification.iter().map(|conclusion| AdapterLoss {
            path: format!("conclusions/{}", conclusion.conclusion_id),
            reason: "research conclusion qualification is retained separately from native requirement execution proof".into(),
            retained_in_canonical_package: true,
        }))
        .collect();
    Ok(ResearchPackageImport {
        package_id,
        revision_id,
        schema_digest: CONTRACT_SCHEMA_SHA256.into(),
        canonical_package: package,
        qualification,
        conclusion_qualification,
        alignment_projection,
        losses,
    })
}

pub fn export_research_package(imported: &ResearchPackageImport) -> Result<String, String> {
    serde_json::to_string(&canonicalize(&imported.canonical_package))
        .map_err(|error| error.to_string())
}

pub fn research_package_digest(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(&canonicalize(value)).map_err(|error| error.to_string())?;
    let digest = Sha256::digest(bytes);
    let encoded = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("sha256:{encoded}"))
}

pub fn map_claim_state_to_alignment(state: &ResearchClaimState) -> AlignmentClassification {
    match state {
        ResearchClaimState::Contradicted => AlignmentClassification::Failed,
        ResearchClaimState::Unknown => AlignmentClassification::Unknown,
        ResearchClaimState::Supported
        | ResearchClaimState::Weakened
        | ResearchClaimState::Contested => AlignmentClassification::Partial,
    }
}

fn validate_package(package: &Value) -> Result<(), String> {
    if text(package, "schema_version")? != CONTRACT_SCHEMA_VERSION {
        return Err("unsupported evidence-centered research schema".into());
    }
    if !matches!(text(package, "privacy_tier")?, "P0" | "P1") {
        return Err("research package privacy tier must be P0 or P1".into());
    }
    if text(package, "privacy_tier")? == "P1"
        && package.get("reviewed").and_then(Value::as_bool) != Some(true)
    {
        return Err("P1 research packages require reviewed=true".into());
    }
    let sources = index(array(package, "sources")?, "source_id")?;
    let methods = index(array(package, "methods")?, "method_id")?;
    let evidence = index(array(package, "evidence")?, "evidence_id")?;
    let claims = index(array(package, "claims")?, "claim_id")?;
    if sources.is_empty() || methods.is_empty() || claims.is_empty() {
        return Err("sources, methods, and claims must be non-empty".into());
    }
    for item in evidence.values() {
        if !methods.contains_key(text(item, "method_ref")?) {
            return Err(format!(
                "evidence {} has unknown method",
                text(item, "evidence_id")?
            ));
        }
        for source_ref in strings(array(item, "source_refs")?)? {
            if !sources.contains_key(source_ref.as_str()) {
                return Err(format!(
                    "evidence {} has unknown source",
                    text(item, "evidence_id")?
                ));
            }
        }
        if text(item, "status")? == "available"
            && item.get("result_binding").is_none_or(Value::is_null)
        {
            return Err(format!(
                "available evidence {} lacks result binding",
                text(item, "evidence_id")?
            ));
        }
    }
    for claim in claims.values() {
        for link in array(claim, "evidence_links")? {
            if !evidence.contains_key(text(link, "evidence_ref")?) {
                return Err(format!(
                    "claim {} has unknown evidence",
                    text(claim, "claim_id")?
                ));
            }
        }
    }
    Ok(())
}

fn qualify_claims(package: &Value) -> Result<Vec<ClaimQualification>, String> {
    let sources = index(array(package, "sources")?, "source_id")?;
    let methods = index(array(package, "methods")?, "method_id")?;
    let evidence = index(array(package, "evidence")?, "evidence_id")?;
    let mut results = Vec::new();
    for claim in array(package, "claims")? {
        let mut support = 0;
        let mut weakening = 0;
        let mut contradiction = 0;
        let mut excluded = BTreeMap::new();
        for link in array(claim, "evidence_links")? {
            let evidence_id = text(link, "evidence_ref")?;
            let item = evidence[evidence_id];
            let method = methods[text(item, "method_ref")?];
            let reasons = exclusion_reasons(claim, link, item, method, &sources)?;
            let relationship = text(link, "relationship")?;
            if !reasons.is_empty() {
                if text(method, "power_status")? == "underpowered" && relationship == "supports" {
                    weakening += 1;
                }
                excluded.insert(evidence_id.to_string(), reasons);
                continue;
            }
            match relationship {
                "supports" => support += 1,
                "weakens" => weakening += 1,
                "contradicts" => contradiction += 1,
                _ => return Err("unknown claim-evidence relationship".into()),
            }
        }
        let state = match (support > 0, weakening > 0, contradiction > 0) {
            (true, _, true) => ResearchClaimState::Contested,
            (false, _, true) => ResearchClaimState::Contradicted,
            (true, true, false) | (false, true, false) => ResearchClaimState::Weakened,
            (true, false, false) => ResearchClaimState::Supported,
            (false, false, false) => ResearchClaimState::Unknown,
        };
        results.push(ClaimQualification {
            claim_id: text(claim, "claim_id")?.into(),
            state,
            excluded_evidence: excluded,
        });
    }
    Ok(results)
}

fn qualify_conclusions(
    package: &Value,
    claim_results: &[ClaimQualification],
) -> Result<Vec<ConclusionQualification>, String> {
    let claims = index(array(package, "claims")?, "claim_id")?;
    let states: BTreeMap<_, _> = claim_results
        .iter()
        .map(|result| (result.claim_id.clone(), result.state.clone()))
        .collect();
    let mut results = Vec::new();

    for conclusion in array(package, "conclusions")? {
        let conclusion_id = text(conclusion, "conclusion_id")?;
        let mut referenced_claim_states = BTreeMap::new();
        let mut has_unknown = false;
        let mut has_contradicted = false;
        let mut has_contested = false;
        let mut has_weakened = false;
        let mut strongest_supported: Option<&str> = None;

        for claim_ref in strings(array(conclusion, "claim_refs")?)? {
            let state = states
                .get(&claim_ref)
                .ok_or_else(|| format!("conclusion {conclusion_id} has unknown claim"))?
                .clone();
            match state {
                ResearchClaimState::Unknown => has_unknown = true,
                ResearchClaimState::Contradicted => has_contradicted = true,
                ResearchClaimState::Contested => has_contested = true,
                ResearchClaimState::Weakened => has_weakened = true,
                ResearchClaimState::Supported => {
                    let claim_type = text(claims[claim_ref.as_str()], "claim_type")?;
                    if strongest_supported.is_none_or(|current| {
                        claim_type_rank(claim_type) > claim_type_rank(current)
                    }) {
                        strongest_supported = Some(claim_type);
                    }
                }
            }
            referenced_claim_states.insert(claim_ref, state);
        }

        let mut reasons = Vec::new();
        let mut state = if has_unknown {
            reasons.push("referenced_claim:unknown".into());
            ResearchClaimState::Unknown
        } else if has_contradicted {
            reasons.push("referenced_claim:contradicted".into());
            ResearchClaimState::Contradicted
        } else if has_contested {
            reasons.push("referenced_claim:contested".into());
            ResearchClaimState::Contested
        } else if has_weakened {
            reasons.push("referenced_claim:weakened".into());
            ResearchClaimState::Weakened
        } else {
            ResearchClaimState::Supported
        };

        match strongest_supported {
            None => {
                reasons.push("conclusion:no_supported_claim".into());
                state = ResearchClaimState::Unknown;
            }
            Some(supported) => {
                let declared = text(conclusion, "strongest_claim_type")?;
                if claim_type_rank(declared) > claim_type_rank(supported) {
                    reasons.push(format!(
                        "conclusion:claim_type_overreach:declared={declared}:supported={supported}"
                    ));
                    state = ResearchClaimState::Unknown;
                }
            }
        }

        reasons.sort();
        reasons.dedup();
        results.push(ConclusionQualification {
            conclusion_id: conclusion_id.into(),
            state,
            referenced_claim_states,
            reasons,
        });
    }
    Ok(results)
}

fn claim_type_rank(value: &str) -> u8 {
    match value {
        "observation" => 0,
        "inference" => 1,
        "hypothesis" => 2,
        "causal" => 3,
        "decision" => 4,
        _ => 255,
    }
}

fn exclusion_reasons(
    claim: &Value,
    link: &Value,
    evidence: &Value,
    method: &Value,
    sources: &BTreeMap<String, &Value>,
) -> Result<Vec<String>, String> {
    let mut reasons = BTreeSet::new();
    if text(evidence, "status")? != "available" {
        reasons.insert(format!("evidence_status:{}", text(evidence, "status")?));
    }
    if text(evidence, "freshness")? != "current" {
        reasons.insert(format!(
            "evidence_freshness:{}",
            text(evidence, "freshness")?
        ));
    }
    for source_ref in strings(array(evidence, "source_refs")?)? {
        let source = sources[source_ref.as_str()];
        if text(source, "state")? != "active" {
            reasons.insert(format!(
                "source:{source_ref}:state:{}",
                text(source, "state")?
            ));
        }
        if text(source, "freshness")? != "current" {
            reasons.insert(format!(
                "source:{source_ref}:freshness:{}",
                text(source, "freshness")?
            ));
        }
    }
    for transform in array(method, "transformations")? {
        if text(transform, "validity")? != "valid" {
            reasons.insert(format!(
                "transformation:{}:validity:{}",
                text(transform, "transformation_id")?,
                text(transform, "validity")?
            ));
        }
    }
    if link.get("requires_denominator").and_then(Value::as_bool) == Some(true)
        && method
            .get("denominator")
            .and_then(|value| value.get("value"))
            .is_none_or(Value::is_null)
    {
        reasons.insert("denominator:missing".into());
    }
    let power = text(method, "power_status")?;
    let relationship = text(link, "relationship")?;
    if matches!(power, "underpowered" | "unknown")
        && matches!(relationship, "supports" | "contradicts")
    {
        reasons.insert(format!("power:{power}:cannot_{relationship}"));
    }
    if text(claim, "claim_type")? == "causal"
        && matches!(relationship, "supports" | "contradicts")
        && method
            .get("causal_identification")
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            != Some("identified")
    {
        reasons.insert("causal_design:not_identified".into());
    }
    Ok(reasons.into_iter().collect())
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string field {key}"))
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value], String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("missing array field {key}"))
}

fn strings(values: &[Value]) -> Result<Vec<String>, String> {
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "expected string array".into())
        })
        .collect()
}

fn index<'a>(values: &'a [Value], id_field: &str) -> Result<BTreeMap<String, &'a Value>, String> {
    let mut result = BTreeMap::new();
    for value in values {
        let id = text(value, id_field)?.to_string();
        if result.insert(id, value).is_some() {
            return Err(format!("duplicate {id_field}"));
        }
    }
    Ok(result)
}

fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonicalize).collect()),
        Value::Object(values) => {
            let ordered: BTreeMap<_, _> = values
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize(value)))
                .collect();
            serde_json::to_value(ordered).expect("BTreeMap serialization is infallible")
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str =
        include_str!("../../../fixtures/evidence-centered-research/qualified-package-v2.json");

    #[test]
    fn imports_requalifies_and_round_trips_the_shared_contract() {
        let imported = import_research_package(FIXTURE).expect("import fixture");
        let original: Value = serde_json::from_str(FIXTURE).expect("parse fixture");
        let exported: Value =
            serde_json::from_str(&export_research_package(&imported).expect("export fixture"))
                .expect("parse export");
        assert_eq!(
            research_package_digest(&original).unwrap(),
            research_package_digest(&exported).unwrap()
        );

        let states: BTreeMap<_, _> = imported
            .qualification
            .iter()
            .map(|claim| (claim.claim_id.as_str(), &claim.state))
            .collect();
        assert_eq!(states["claim-supported"], &ResearchClaimState::Supported);
        assert_eq!(states["claim-contested"], &ResearchClaimState::Contested);
        assert_eq!(states["claim-retracted"], &ResearchClaimState::Unknown);
        assert_eq!(states["claim-underpowered"], &ResearchClaimState::Weakened);
        assert_eq!(
            states["claim-causal-overreach"],
            &ResearchClaimState::Unknown
        );

        let unsupported = imported
            .conclusion_qualification
            .iter()
            .find(|item| item.conclusion_id == "conclusion-unsupported")
            .expect("unsupported conclusion qualification");
        assert_eq!(unsupported.state, ResearchClaimState::Unknown);
        assert!(unsupported
            .reasons
            .iter()
            .any(|reason| reason == "conclusion:no_supported_claim"));
    }

    #[test]
    fn supported_research_never_becomes_native_verified() {
        assert_eq!(
            map_claim_state_to_alignment(&ResearchClaimState::Supported),
            AlignmentClassification::Partial
        );
        let imported = import_research_package(FIXTURE).expect("import fixture");
        assert!(imported
            .alignment_projection
            .values()
            .all(|state| state != &AlignmentClassification::Verified));
        assert!(imported
            .losses
            .iter()
            .all(|loss| loss.retained_in_canonical_package));
    }
}
