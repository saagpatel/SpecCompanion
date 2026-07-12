use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GeneratedTest {
    pub id: String,
    pub requirement_id: String,
    pub framework: String,
    pub code: String,
    pub generation_mode: String,
    pub file_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RepositoryTestCandidate {
    pub path: String,
    pub language: String,
    pub framework: String,
    pub assertion_status: String,
    pub assertion_lines: Vec<i64>,
}

#[derive(Debug, Deserialize)]
pub struct LinkRepositoryTestRequest {
    pub project_id: String,
    pub requirement_id: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct GenerateTestsRequest {
    pub requirement_ids: Vec<String>,
    pub framework: String,
    pub mode: String,
    pub project_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TestResult {
    pub id: String,
    pub generated_test_id: String,
    pub status: String,
    pub execution_time_ms: i64,
    pub stdout: String,
    pub stderr: String,
    pub executed_at: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct TestProgress {
    pub total: usize,
    pub completed: usize,
    pub current_test: String,
    pub status: String,
}
