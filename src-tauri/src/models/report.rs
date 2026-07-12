use serde::{Deserialize, Serialize};

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
    pub evidence: Vec<EvidenceRecord>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlignmentReportWithEvidence {
    #[serde(flatten)]
    pub report: AlignmentReport,
    pub alignments: Vec<RequirementAlignment>,
}
