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
