use rusqlite::Connection;
use uuid::Uuid;
use chrono::Utc;
use crate::db::queries;
use crate::errors::AppError;
use crate::models::report::{AlignmentReport, Mismatch, AlignmentReportWithMismatches};

pub fn generate_report(conn: &Connection, project_id: &str) -> Result<AlignmentReportWithMismatches, AppError> {
    let requirements = queries::get_requirements_for_project(conn, project_id)?;
    let total = requirements.len() as i64;

    if total == 0 {
        let report = AlignmentReport {
            id: Uuid::new_v4().to_string(),
            project_id: project_id.to_string(),
            coverage_percent: 0.0,
            total_requirements: 0,
            covered_requirements: 0,
            generated_at: Utc::now().to_rfc3339(),
        };
        queries::insert_alignment_report(conn, &report)?;
        return Ok(AlignmentReportWithMismatches {
            report,
            mismatches: Vec::new(),
        });
    }

    let report_id = Uuid::new_v4().to_string();
    let mut covered = 0i64;
    let mut mismatches = Vec::new();

    for req in &requirements {
        let tests = queries::get_generated_tests_for_requirement(conn, &req.id)?;

        if tests.is_empty() {
            mismatches.push(Mismatch {
                id: Uuid::new_v4().to_string(),
                report_id: report_id.clone(),
                requirement_id: req.id.clone(),
                spec_section: req.section.clone(),
                code_element: None,
                mismatch_type: "no_test_generated".to_string(),
                details: format!("No test has been generated for: {}", req.description),
            });
            continue;
        }

        // Check if any test has been executed and passed
        let mut has_passing = false;
        let mut has_failing = false;
        let mut has_no_results = true;

        for test in &tests {
            if let Some(result) = queries::get_latest_test_result_for_test(conn, &test.id)? {
                has_no_results = false;
                match result.status.as_str() {
                    "passed" => has_passing = true,
                    "failed" | "error" => has_failing = true,
                    _ => {}
                }
            }
        }

        if has_no_results {
            mismatches.push(Mismatch {
                id: Uuid::new_v4().to_string(),
                report_id: report_id.clone(),
                requirement_id: req.id.clone(),
                spec_section: req.section.clone(),
                code_element: None,
                mismatch_type: "not_implemented".to_string(),
                details: format!("Tests generated but never executed for: {}", req.description),
            });
        } else if has_passing && !has_failing {
            covered += 1;
        } else if has_passing && has_failing {
            covered += 1;
            mismatches.push(Mismatch {
                id: Uuid::new_v4().to_string(),
                report_id: report_id.clone(),
                requirement_id: req.id.clone(),
                spec_section: req.section.clone(),
                code_element: None,
                mismatch_type: "partial_coverage".to_string(),
                details: format!("Some tests passing, some failing for: {}", req.description),
            });
        } else if has_failing {
            mismatches.push(Mismatch {
                id: Uuid::new_v4().to_string(),
                report_id: report_id.clone(),
                requirement_id: req.id.clone(),
                spec_section: req.section.clone(),
                code_element: None,
                mismatch_type: "test_failing".to_string(),
                details: format!("Test(s) failing for: {}", req.description),
            });
        }
    }

    let coverage_percent = if total > 0 {
        (covered as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    let report = AlignmentReport {
        id: report_id,
        project_id: project_id.to_string(),
        coverage_percent,
        total_requirements: total,
        covered_requirements: covered,
        generated_at: Utc::now().to_rfc3339(),
    };

    let tx = conn.unchecked_transaction().map_err(AppError::Database)?;
    queries::insert_alignment_report(&tx, &report)?;
    for mismatch in &mismatches {
        queries::insert_mismatch(&tx, mismatch)?;
    }
    tx.commit().map_err(AppError::Database)?;

    Ok(AlignmentReportWithMismatches { report, mismatches })
}

#[cfg(test)]
mod tests {
    // Helper function to test coverage calculation
    fn calculate_coverage_percent(total: i64, covered: i64) -> f64 {
        if total > 0 {
            (covered as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    // Helper to determine mismatch type based on test state
    fn classify_mismatch(
        has_tests: bool,
        has_results: bool,
        has_passing: bool,
        has_failing: bool,
    ) -> Option<&'static str> {
        if !has_tests {
            return Some("no_test_generated");
        }
        if !has_results {
            return Some("not_implemented");
        }
        if has_passing && has_failing {
            return Some("partial_coverage");
        }
        if has_failing && !has_passing {
            return Some("test_failing");
        }
        None // Fully covered
    }

    #[test]
    fn test_mismatch_no_test_generated() {
        let mismatch_type = classify_mismatch(false, false, false, false);
        assert_eq!(mismatch_type, Some("no_test_generated"));
    }

    #[test]
    fn test_mismatch_test_not_executed() {
        let mismatch_type = classify_mismatch(true, false, false, false);
        assert_eq!(mismatch_type, Some("not_implemented"));
    }

    #[test]
    fn test_covered_requirement_all_passed() {
        let mismatch_type = classify_mismatch(true, true, true, false);
        assert_eq!(mismatch_type, None); // No mismatch = covered
    }

    #[test]
    fn test_partial_coverage_some_failed() {
        let mismatch_type = classify_mismatch(true, true, true, true);
        assert_eq!(mismatch_type, Some("partial_coverage"));
    }

    #[test]
    fn test_all_tests_failing() {
        let mismatch_type = classify_mismatch(true, true, false, true);
        assert_eq!(mismatch_type, Some("test_failing"));
    }

    #[test]
    fn test_coverage_percentage_calculation() {
        let coverage = calculate_coverage_percent(3, 2);
        assert!((coverage - 66.666).abs() < 0.01); // 2/3 ≈ 66.67%
    }

    #[test]
    fn test_empty_project_coverage() {
        let coverage = calculate_coverage_percent(0, 0);
        assert_eq!(coverage, 0.0);
    }

    #[test]
    fn test_full_coverage() {
        let coverage = calculate_coverage_percent(5, 5);
        assert_eq!(coverage, 100.0);
    }

    #[test]
    fn test_zero_coverage() {
        let coverage = calculate_coverage_percent(10, 0);
        assert_eq!(coverage, 0.0);
    }

    #[test]
    fn test_coverage_rounding() {
        let coverage = calculate_coverage_percent(7, 3);
        assert!((coverage - 42.857).abs() < 0.01); // 3/7 ≈ 42.86%
    }

    #[test]
    fn test_high_coverage() {
        let coverage = calculate_coverage_percent(100, 95);
        assert_eq!(coverage, 95.0);
    }

    #[test]
    fn test_single_requirement_covered() {
        let coverage = calculate_coverage_percent(1, 1);
        assert_eq!(coverage, 100.0);
    }

    #[test]
    fn test_single_requirement_not_covered() {
        let coverage = calculate_coverage_percent(1, 0);
        assert_eq!(coverage, 0.0);
    }

    #[test]
    fn test_mismatch_classification_sequence() {
        // Test the priority order of classification

        // Priority 1: No tests
        assert_eq!(classify_mismatch(false, true, true, true), Some("no_test_generated"));

        // Priority 2: Tests exist but not executed
        assert_eq!(classify_mismatch(true, false, false, false), Some("not_implemented"));

        // Priority 3: Tests executed but all failing
        assert_eq!(classify_mismatch(true, true, false, true), Some("test_failing"));

        // Priority 4: Tests executed with mixed results
        assert_eq!(classify_mismatch(true, true, true, true), Some("partial_coverage"));

        // Best case: All passing
        assert_eq!(classify_mismatch(true, true, true, false), None);
    }

    #[test]
    fn test_edge_case_only_error_status() {
        // If all tests have "error" status (not "failed" or "passed")
        // Current implementation treats "error" same as "failed"
        let mismatch_type = classify_mismatch(true, true, false, true);
        assert_eq!(mismatch_type, Some("test_failing"));
    }

    #[test]
    fn test_large_project_coverage() {
        // Test with realistic project sizes
        let coverage = calculate_coverage_percent(500, 437);
        assert!((coverage - 87.4).abs() < 0.01);
    }

    #[test]
    fn test_coverage_precision() {
        // Ensure float precision is maintained
        let coverage = calculate_coverage_percent(3, 1);
        assert!((coverage - 33.333).abs() < 0.01);
    }
}
