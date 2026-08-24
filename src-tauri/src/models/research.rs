use std::collections::{BTreeMap, BTreeSet};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::models::report::AlignmentClassification;

pub const CONTRACT_SCHEMA_VERSION_V1: &str = "evidence-centered.research-package.v1";
pub const CONTRACT_SCHEMA_SHA256_V1: &str =
    "sha256:ab1702392cdd3c3b0d465f52de5114d5f4aad8e1e47730c10fca53fc7622360c";
pub const CONTRACT_SCHEMA_VERSION_V2: &str = "evidence-centered.research-package.v2";
pub const CONTRACT_SCHEMA_SHA256_V2: &str =
    "sha256:4cff2030f2ccfb64937d8db5453f16510b30bc1db48a882d161a8b6944ae3ceb";

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
#[serde(rename_all = "snake_case")]
pub enum SourceLifecycleState {
    Authenticated,
    RevokedAuthority,
    UnknownAuthority,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SourceLifecycleQualification {
    pub source_id: String,
    pub authority_id: String,
    pub state: SourceLifecycleState,
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
    pub schema_version: String,
    pub package_id: String,
    pub revision_id: String,
    pub schema_digest: String,
    pub package_digest: String,
    pub canonical_package: Value,
    pub qualification: Vec<ClaimQualification>,
    pub conclusion_qualification: Vec<ConclusionQualification>,
    pub source_lifecycle: Vec<SourceLifecycleQualification>,
    pub alignment_projection: BTreeMap<String, AlignmentClassification>,
    pub losses: Vec<AdapterLoss>,
}

pub fn import_research_package(raw: &str) -> Result<ResearchPackageImport, String> {
    let package: Value = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    validate_package(&package)?;
    let schema_version = text(&package, "schema_version")?.to_string();
    let package_id = text(&package, "package_id")?.to_string();
    let revision_id = text(&package, "revision_id")?.to_string();
    let package_digest = research_package_digest(&package)?;
    let qualification = qualify_claims(&package)?;
    let conclusion_qualification = qualify_conclusions(&package, &qualification)?;
    let source_lifecycle = qualify_source_lifecycle(&package)?;
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
        schema_version: schema_version.clone(),
        package_id,
        revision_id,
        schema_digest: match schema_version.as_str() {
            CONTRACT_SCHEMA_VERSION_V1 => CONTRACT_SCHEMA_SHA256_V1.into(),
            CONTRACT_SCHEMA_VERSION_V2 => CONTRACT_SCHEMA_SHA256_V2.into(),
            _ => return Err("unsupported evidence-centered research schema".into()),
        },
        package_digest,
        canonical_package: package,
        qualification,
        conclusion_qualification,
        source_lifecycle,
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
    let schema_version = text(package, "schema_version")?;
    if !matches!(
        schema_version,
        CONTRACT_SCHEMA_VERSION_V1 | CONTRACT_SCHEMA_VERSION_V2
    ) {
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
    if schema_version == CONTRACT_SCHEMA_VERSION_V2 {
        validate_lifecycle_bindings(package, &sources)?;
        validate_population_bindings(package, &methods)?;
    }
    Ok(())
}

fn validate_lifecycle_bindings(
    package: &Value,
    sources: &BTreeMap<String, &Value>,
) -> Result<(), String> {
    let authorities = index(array(package, "lifecycle_authorities")?, "authority_id")?;
    let attestations = index(array(package, "lifecycle_attestations")?, "attestation_id")?;
    let mut referenced = BTreeSet::new();

    for (source_id, source) in sources {
        let attestation_ref = text(source, "lifecycle_attestation_ref")?;
        let attestation = attestations
            .get(attestation_ref)
            .ok_or_else(|| format!("source {source_id} has unknown lifecycle attestation"))?;
        if text(attestation, "source_ref")? != source_id {
            return Err(format!(
                "attestation {attestation_ref} does not bind source {source_id}"
            ));
        }
        let authority_ref = text(attestation, "authority_ref")?;
        let authority = authorities
            .get(authority_ref)
            .ok_or_else(|| format!("attestation {attestation_ref} has unknown authority"))?;
        if text(attestation, "asserted_state")? != text(source, "state")?
            || text(attestation, "asserted_freshness")? != text(source, "freshness")?
            || text(attestation, "version_id")? != text(source, "version_id")?
            || text(attestation, "content_digest")? != text(source, "content_digest")?
        {
            return Err(format!(
                "attestation {attestation_ref} does not bind source state"
            ));
        }
        verify_lifecycle_attestation(attestation, authority)?;
        referenced.insert(attestation_ref.to_string());
    }
    if referenced != attestations.keys().cloned().collect() {
        return Err("every lifecycle attestation must bind exactly one source".into());
    }
    Ok(())
}

fn verify_lifecycle_attestation(attestation: &Value, authority: &Value) -> Result<(), String> {
    let public_key_bytes = STANDARD
        .decode(text(authority, "public_key_base64")?)
        .map_err(|_| "lifecycle authority public key is not valid base64".to_string())?;
    let fingerprint = format!(
        "sha256:{}",
        Sha256::digest(&public_key_bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    if fingerprint != text(authority, "public_key_fingerprint")? {
        return Err("lifecycle authority fingerprint does not match public key".into());
    }
    let public_key: [u8; 32] = public_key_bytes
        .try_into()
        .map_err(|_| "lifecycle authority public key is not valid Ed25519".to_string())?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| "lifecycle authority public key is not valid Ed25519".to_string())?;

    let payload = lifecycle_payload(attestation)?;
    let payload_bytes = serde_json::to_vec(&canonicalize(&payload)).map_err(|e| e.to_string())?;
    let payload_digest = format!(
        "sha256:{}",
        Sha256::digest(&payload_bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    if payload_digest != text(attestation, "payload_digest")? {
        return Err(format!(
            "attestation {} payload digest does not match",
            text(attestation, "attestation_id")?
        ));
    }
    if text(attestation, "signature_algorithm")? != "Ed25519" {
        return Err("unsupported lifecycle signature algorithm".into());
    }
    let signature_bytes = STANDARD
        .decode(text(attestation, "signature_base64")?)
        .map_err(|_| "lifecycle attestation signature is not valid base64".to_string())?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| "lifecycle attestation signature is not valid Ed25519".to_string())?;
    verifying_key
        .verify(&payload_bytes, &signature)
        .map_err(|_| {
            format!(
                "attestation {} Ed25519 verification failed",
                text(attestation, "attestation_id").unwrap_or("unknown")
            )
        })
}

fn lifecycle_payload(attestation: &Value) -> Result<Value, String> {
    let mut payload = Map::new();
    for key in [
        "asserted_freshness",
        "asserted_state",
        "attestation_id",
        "authority_ref",
        "content_digest",
        "issued_at",
        "source_ref",
        "version_id",
    ] {
        payload.insert(
            key.into(),
            attestation
                .get(key)
                .cloned()
                .ok_or_else(|| format!("missing lifecycle attestation field {key}"))?,
        );
    }
    Ok(Value::Object(payload))
}

fn validate_population_bindings(
    _package: &Value,
    methods: &BTreeMap<String, &Value>,
) -> Result<(), String> {
    let population_fields = [
        "estimand",
        "target_population",
        "analysis_population",
        "sampling_frame",
        "sampling_method",
    ];
    for (method_id, method) in methods {
        let population = method
            .get("population_binding")
            .ok_or_else(|| format!("method {method_id} lacks population binding"))?;
        let mut missing: BTreeSet<String> = population_fields
            .iter()
            .filter(|field| population.get(**field).is_none_or(Value::is_null))
            .map(|field| (*field).to_string())
            .collect();
        if text(population, "missingness_mechanism")? == "unknown" {
            missing.insert("missingness_mechanism".into());
        }
        let declared: BTreeSet<String> = strings(array(population, "unknown_fields")?)?
            .into_iter()
            .collect();
        if missing != declared {
            return Err(format!(
                "method {method_id} population unknown_fields do not match missing fields"
            ));
        }
        if !declared.is_empty()
            && population
                .get("unknown_reason")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
        {
            return Err(format!(
                "method {method_id} unknown population fields require a reason"
            ));
        }
    }
    Ok(())
}

fn qualify_claims(package: &Value) -> Result<Vec<ClaimQualification>, String> {
    let sources = index(array(package, "sources")?, "source_id")?;
    let methods = index(array(package, "methods")?, "method_id")?;
    let evidence = index(array(package, "evidence")?, "evidence_id")?;
    let lifecycle: BTreeMap<_, _> = qualify_source_lifecycle(package)?
        .into_iter()
        .map(|item| (item.source_id, item.state))
        .collect();
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
            let reasons = exclusion_reasons(claim, link, item, method, &sources, &lifecycle)?;
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

fn qualify_source_lifecycle(package: &Value) -> Result<Vec<SourceLifecycleQualification>, String> {
    if text(package, "schema_version")? == CONTRACT_SCHEMA_VERSION_V1 {
        return Ok(Vec::new());
    }
    let authorities = index(array(package, "lifecycle_authorities")?, "authority_id")?;
    let attestations = index(array(package, "lifecycle_attestations")?, "attestation_id")?;
    let mut results = Vec::new();
    for source in array(package, "sources")? {
        let source_id = text(source, "source_id")?;
        let attestation = attestations[text(source, "lifecycle_attestation_ref")?];
        let authority_id = text(attestation, "authority_ref")?;
        let authority = authorities[authority_id];
        let (state, reasons) = match text(authority, "trust_status")? {
            "trusted" => (SourceLifecycleState::Authenticated, Vec::new()),
            "revoked" => (
                SourceLifecycleState::RevokedAuthority,
                vec!["lifecycle_authority:revoked".into()],
            ),
            "unknown" => (
                SourceLifecycleState::UnknownAuthority,
                vec!["lifecycle_authority:unknown".into()],
            ),
            _ => return Err("unknown lifecycle authority trust status".into()),
        };
        results.push(SourceLifecycleQualification {
            source_id: source_id.into(),
            authority_id: authority_id.into(),
            state,
            reasons,
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
    lifecycle: &BTreeMap<String, SourceLifecycleState>,
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
        if let Some(state) = lifecycle.get(&source_ref) {
            match state {
                SourceLifecycleState::Authenticated => {}
                SourceLifecycleState::RevokedAuthority => {
                    reasons.insert(format!("source:{source_ref}:lifecycle:revoked_authority"));
                }
                SourceLifecycleState::UnknownAuthority => {
                    reasons.insert(format!("source:{source_ref}:lifecycle:unknown_authority"));
                }
            }
        }
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
    if link
        .get("requires_population_binding")
        .and_then(Value::as_bool)
        == Some(true)
    {
        let population = method
            .get("population_binding")
            .ok_or_else(|| "population binding is required".to_string())?;
        for field in ["estimand", "analysis_population", "sampling_frame"] {
            if population.get(field).is_none_or(Value::is_null) {
                reasons.insert(format!("population_binding:{field}:unknown"));
            }
        }
        if text(population, "missingness_mechanism")? == "unknown" {
            reasons.insert("population_binding:missingness_mechanism:unknown".into());
        }
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
    const FIXTURE_V3: &str =
        include_str!("../../../fixtures/evidence-centered-research/qualified-package-v3.json");
    const INVALID_SIGNATURE_V2: &str = include_str!(
        "../../../fixtures/evidence-centered-research/invalid-lifecycle-signature-v2.json"
    );
    const INVALID_POPULATION_V2: &str = include_str!(
        "../../../fixtures/evidence-centered-research/invalid-population-binding-v2.json"
    );

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

    #[test]
    fn v2_package_authenticates_lifecycle_and_preserves_population_unknowns() {
        let imported = import_research_package(FIXTURE_V3).expect("import v2 package fixture");
        assert_eq!(imported.schema_version, CONTRACT_SCHEMA_VERSION_V2);
        assert_eq!(imported.schema_digest, CONTRACT_SCHEMA_SHA256_V2);
        let unknown_authority = imported
            .source_lifecycle
            .iter()
            .find(|item| item.source_id == "source-unknown-authority")
            .expect("unknown authority result");
        assert_eq!(
            unknown_authority.state,
            SourceLifecycleState::UnknownAuthority
        );
        let unknown_claim = imported
            .qualification
            .iter()
            .find(|item| item.claim_id == "claim-unknown-authority")
            .expect("unknown authority claim");
        assert_eq!(unknown_claim.state, ResearchClaimState::Unknown);
        assert!(
            unknown_claim.excluded_evidence["evidence-unknown-authority"]
                .iter()
                .any(|reason| reason
                    == "source:source-unknown-authority:lifecycle:unknown_authority")
        );
        let missing_population = imported
            .qualification
            .iter()
            .find(|item| item.claim_id == "claim-missing-denominator")
            .expect("missing population claim");
        assert!(
            missing_population.excluded_evidence["evidence-missing-denominator"]
                .iter()
                .any(|reason| reason == "population_binding:analysis_population:unknown")
        );
        assert!(imported
            .alignment_projection
            .values()
            .all(|state| state != &AlignmentClassification::Verified));
    }

    #[test]
    fn v2_package_rejects_an_invalid_lifecycle_signature() {
        let error = import_research_package(INVALID_SIGNATURE_V2)
            .expect_err("invalid lifecycle signature must be rejected");
        assert!(error.contains("Ed25519 verification failed"), "{error}");
    }

    #[test]
    fn v2_package_rejects_an_invalid_population_unknown_declaration() {
        let error = import_research_package(INVALID_POPULATION_V2)
            .expect_err("invalid population unknown declaration must be rejected");
        assert!(error.contains("population unknown_fields do not match missing fields"));
    }
}
