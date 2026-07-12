// Project types
export interface Project {
  id: string;
  name: string;
  codebase_path: string;
  created_at: string;
  updated_at: string;
}

export interface CreateProjectRequest {
  name: string;
  codebase_path: string;
}

export interface ProjectWithStats extends Project {
  spec_count: number;
  coverage_percent: number | null;
  last_run_at: string | null;
}

// Spec types
export interface Spec {
  id: string;
  project_id: string;
  filename: string;
  content: string;
  parsed_at: string | null;
  created_at: string;
}

export interface Requirement {
  id: string;
  spec_id: string;
  section: string;
  description: string;
  req_type: "functional" | "non_functional" | "constraint";
  priority: "high" | "medium" | "low";
  content_fingerprint: string;
  source_line_start: number;
  source_line_end: number;
}

export interface ParsedSpec {
  spec: Spec;
  requirements: Requirement[];
}

// Test generation types
export interface GeneratedTest {
  id: string;
  requirement_id: string;
  framework: "jest" | "pytest" | "vitest" | "unittest";
  code: string;
  generation_mode: "template" | "llm" | "repository_link";
  file_path: string | null;
  created_at: string;
}

export interface RepositoryTestCandidate {
  path: string;
  language: "javascript" | "typescript" | "python";
  framework: "jest" | "pytest" | "vitest" | "unittest";
  assertion_status: "meaningful" | "placeholder" | "missing";
  assertion_lines: number[];
}

export interface LinkRepositoryTestRequest {
  project_id: string;
  requirement_id: string;
  path: string;
}

export interface GenerateTestsRequest {
  requirement_ids: string[];
  framework: "jest" | "pytest";
  mode: "template" | "llm";
  project_id: string;
}

// Test execution types
export interface TestResult {
  id: string;
  generated_test_id: string;
  status:
    | "passed"
    | "failed"
    | "timed_out"
    | "runtime_unavailable"
    | "blocked"
    | "unsupported"
    | "error";
  execution_time_ms: number;
  stdout: string;
  stderr: string;
  executed_at: string;
  execution_controls: ExecutionControls;
  provenance_digest: string;
  provenance_status: "verified" | "invalid" | "legacy_unverified" | "";
}

export interface ExecutionControls {
  platform: string;
  isolation_backend: string;
  profile: string;
  timeout: string;
  output_limit: string;
  process_tree_kill: string;
  network: string;
  filesystem_write: string;
  child_process: string;
}

export interface TestProgress {
  total: number;
  completed: number;
  current_test: string;
  status: "running" | "completed" | "error";
}

// Report types
export interface AlignmentReport {
  id: string;
  project_id: string;
  coverage_percent: number;
  total_requirements: number;
  covered_requirements: number;
  verified_requirements: number;
  partial_requirements: number;
  failed_requirements: number;
  unknown_requirements: number;
  evidence_digest: string;
  integrity_status: "verified" | "invalid" | "legacy_unverified" | "not_checked";
  checked_languages: string[];
  skipped_languages: string[];
  diagnostics: string[];
  generated_at: string;
}

export type AlignmentClassification = "VERIFIED" | "PARTIAL" | "FAILED" | "UNKNOWN";

export interface EvidenceRecord {
  id: string;
  kind: "requirement" | "implementation" | "test" | "assertion" | "execution" | "diagnostic";
  path: string | null;
  line_start: number | null;
  line_end: number | null;
  symbol: string | null;
  status: string;
  summary: string;
}

export interface RequirementAlignment {
  requirement_id: string;
  classification: AlignmentClassification;
  reason: string;
  description: string;
  section: string;
  source_line_start: number;
  source_line_end: number;
  summary: string;
  verification_policy: VerificationPolicyEvidence;
  evidence: EvidenceRecord[];
}

export interface ExecutionPolicyObservation {
  test_id: string;
  framework: string;
  controls: ExecutionControls;
  missing_controls: string[];
}

export interface VerificationPolicyEvidence {
  policy_id: string;
  status: "satisfied" | "insufficient" | "unavailable" | "not_applicable" | "not_evaluated";
  required_controls: string[];
  observations: ExecutionPolicyObservation[];
  missing_controls: string[];
  summary: string;
}

export interface AlignmentReportWithEvidence extends AlignmentReport {
  alignments: RequirementAlignment[];
}

export interface EvidenceBundleVerification {
  status:
    | "verified"
    | "signed_untrusted"
    | "trusted_signer"
    | "revoked"
    | "stale"
    | "invalid"
    | "unsupported";
  schema: string;
  report_id?: string;
  payload_integrity: "verified" | "invalid";
  bundle_integrity: "verified" | "invalid";
  report_integrity: "verified" | "invalid";
  signature_status: string;
  freshness_status: "fresh" | "stale" | "unknown";
  age_seconds?: number;
  diagnostics: string[];
  key_fingerprint?: string;
  signer_identity?: string;
  trust_status: "trusted" | "revoked" | "unknown";
  trust_provenance?: string;
}

export interface SignerTrustRecord {
  project_id: string;
  key_fingerprint: string;
  signer_identity: string;
  status: "trusted" | "revoked";
  provenance: string;
  updated_at: string;
}

export interface SignerTrustHistoryRecord {
  id: string;
  project_id: string;
  key_fingerprint: string;
  signer_identity: string;
  status: "trusted" | "revoked";
  provenance: string;
  recorded_at: string;
  previous_digest: string;
  event_digest: string;
}

export interface SignerTrustHistoryIntegrity {
  status: "verified" | "invalid" | "unknown";
  event_count: number;
  head_digest?: string;
  diagnostics: string[];
}

export interface TrustPolicyVerification {
  status: "valid_untrusted" | "invalid" | "unsupported" | "unknown";
  schema: string;
  signer_identity?: string;
  key_fingerprint?: string;
  source_project_name?: string;
  policy_count: number;
  payload_sha256?: string;
  source_history_head_digest?: string;
  source_history_event_count: number;
  conflicts: Array<{
    key_fingerprint: string;
    signer_identity: string;
    incoming_status: "trusted" | "revoked";
    current_status?: "trusted" | "revoked";
    action: "add" | "preserve" | "replace";
  }>;
  diagnostics: string[];
}

export interface SigningIdentityInfo {
  signer_identity: string;
  key_fingerprint: string;
  public_key: string;
  storage: "os_keychain";
}

// Settings
export interface AppSettings {
  api_key: string;
  default_framework: "jest" | "pytest";
  default_mode: "template" | "llm";
  scan_exclusions: string[];
  python_environment_root: string;
  python_environments: Record<string, TrustedPythonEnvironment>;
}

export interface TrustedPythonEnvironment {
  root: string;
  fingerprint: string;
  interpreter: string;
  capability_profile: "bounded" | "macos_isolated";
}

export interface PythonRuntimeStatus {
  configured: boolean;
  valid: boolean;
  root: string;
  interpreter: string;
  fingerprint: string;
  reason: string;
}
