use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ExecutionPolicyObservation {
    pub test_id: String,
    pub framework: String,
    pub controls: crate::models::test::ExecutionControls,
    pub missing_controls: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationPolicyStatus {
    Satisfied,
    Insufficient,
    Unavailable,
    NotApplicable,
    NotEvaluated,
}

impl VerificationPolicyStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Satisfied => "satisfied",
            Self::Insufficient => "insufficient",
            Self::Unavailable => "unavailable",
            Self::NotApplicable => "not_applicable",
            Self::NotEvaluated => "not_evaluated",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct VerificationPolicyEvidence {
    pub policy_id: String,
    pub status: VerificationPolicyStatus,
    pub required_controls: Vec<String>,
    pub observations: Vec<ExecutionPolicyObservation>,
    pub missing_controls: Vec<String>,
    pub summary: String,
}

impl Default for VerificationPolicyEvidence {
    fn default() -> Self {
        Self {
            policy_id: "not_evaluated".into(),
            status: VerificationPolicyStatus::NotEvaluated,
            required_controls: Vec::new(),
            observations: Vec::new(),
            missing_controls: Vec::new(),
            summary: "This stored report predates structured verification-policy evidence".into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlignmentReport {
    pub id: String,
    pub project_id: String,
    /// VERIFIED requirements only. This deliberately replaces the old
    /// "any passing process" interpretation of coverage.
    pub coverage_percent: f64,
    pub total_requirements: i64,
    pub covered_requirements: i64,
    pub verified_requirements: i64,
    pub partial_requirements: i64,
    pub failed_requirements: i64,
    pub unknown_requirements: i64,
    pub evidence_digest: String,
    #[serde(default)]
    pub integrity_status: String,
    pub checked_languages: Vec<String>,
    pub skipped_languages: Vec<String>,
    pub diagnostics: Vec<String>,
    pub generated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlignmentClassification {
    Verified,
    Partial,
    Failed,
    Unknown,
}

impl AlignmentClassification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verified => "VERIFIED",
            Self::Partial => "PARTIAL",
            Self::Failed => "FAILED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlignmentReason {
    MeaningfulTestPassed,
    PartialEvidence,
    InsufficientEnforcement,
    ImplementationUntested,
    TestNonProbative,
    TestFailed,
    TestTimedOut,
    TestNotRun,
    NoImplementationEvidence,
    UnsupportedLanguage,
    RuntimeUnavailable,
    EvidenceUnavailable,
}

impl AlignmentReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MeaningfulTestPassed => "meaningful_test_passed",
            Self::PartialEvidence => "partial_evidence",
            Self::InsufficientEnforcement => "insufficient_enforcement",
            Self::ImplementationUntested => "implementation_untested",
            Self::TestNonProbative => "test_non_probative",
            Self::TestFailed => "test_failed",
            Self::TestTimedOut => "test_timed_out",
            Self::TestNotRun => "test_not_run",
            Self::NoImplementationEvidence => "no_implementation_evidence",
            Self::UnsupportedLanguage => "unsupported_language",
            Self::RuntimeUnavailable => "runtime_unavailable",
            Self::EvidenceUnavailable => "evidence_unavailable",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Requirement,
    Implementation,
    Test,
    Assertion,
    Execution,
    Diagnostic,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub id: String,
    pub kind: EvidenceKind,
    pub path: Option<String>,
    pub line_start: Option<i64>,
    pub line_end: Option<i64>,
    pub symbol: Option<String>,
    pub status: String,
    pub summary: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RequirementAlignment {
    pub requirement_id: String,
    pub classification: AlignmentClassification,
    pub reason: AlignmentReason,
    pub description: String,
    pub section: String,
    pub source_line_start: i64,
    pub source_line_end: i64,
    pub summary: String,
    #[serde(default)]
    pub verification_policy: VerificationPolicyEvidence,
    pub evidence: Vec<EvidenceRecord>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlignmentReportWithEvidence {
    #[serde(flatten)]
    pub report: AlignmentReport,
    pub alignments: Vec<RequirementAlignment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_alignment_without_policy_is_honestly_not_evaluated() {
        let alignment: RequirementAlignment = serde_json::from_value(serde_json::json!({
            "requirement_id": "req-1",
            "classification": "UNKNOWN",
            "reason": "evidence_unavailable",
            "description": "Legacy requirement",
            "section": "Requirements",
            "source_line_start": 2,
            "source_line_end": 2,
            "summary": "Legacy report",
            "evidence": []
        }))
        .expect("legacy alignment");

        assert_eq!(
            alignment.verification_policy.status,
            VerificationPolicyStatus::NotEvaluated
        );
        assert!(alignment.verification_policy.observations.is_empty());
        assert!(alignment.verification_policy.missing_controls.is_empty());
    }
}
