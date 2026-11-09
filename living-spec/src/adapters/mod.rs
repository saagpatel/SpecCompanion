//! Parsers for supported test-runner output formats.
//! The shared codecs also let saved-score input bindings track changed outcomes.
pub use crate::core::test_results::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::test_identity::{TestOutcome, TestStatus};

    #[test]
    fn libtest_parses_pass_failure_skip_and_failure_detail() {
        let input = "running 3 tests\nnoise from stdout\ntest alpha::passes ... ok\ntest beta::fails ... FAILED\ntest gamma::ignored ... ignored, intentionally disabled\n\nfailures:\n\n---- beta::fails stdout ----\npanic at assertion\nmore detail\n\nfailures:\n    beta::fails\n\ntest result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";
        let result = parse_libtest(input).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].status, TestStatus::Passed);
        assert_eq!(result[1].status, TestStatus::Failed);
        assert_eq!(
            result[1].failure_message.as_deref(),
            Some("panic at assertion")
        );
        assert_eq!(result[2].status, TestStatus::Skipped);
    }

    #[test]
    fn libtest_rejects_garbled_and_incomplete_logs() {
        assert!(parse_libtest("garbled output").is_err());
        assert!(parse_libtest("running 1 test\ntest alpha::x ... ok\n").is_err());
        assert!(parse_libtest("running 2 tests\ntest alpha::x ... ok\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n").is_err());
        assert!(parse_libtest("test alpha::x ... unknown\n").is_err());
    }

    #[test]
    fn libtest_accepts_interleaved_stdout_and_multiple_suites() {
        let input = "running 1 test\ntest alpha::writes ... arbitrary stdout marker\nline from test\nok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out\nrunning 1 test\ntest beta::passes ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
        let result = parse_libtest(input).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].status, TestStatus::Passed);
    }

    #[test]
    fn libtest_scopes_same_name_to_each_binary_and_doc_tests() {
        let input = "    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.01s\n\
     Running unittests src/lib.rs (target/debug/deps/sample-012abc)\n\
running 0 tests\n\
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\
     Running tests/alpha.rs (target/debug/deps/alpha-456def)\n\
running 1 test\n\
test same_name ... FAILED\n\n\
failures:\n\n\
---- same_name stdout ----\n\
first binary assertion failed\n\n\
failures:\n    same_name\n\
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\
     Running tests/beta.rs (target/debug/deps/beta-789abc)\n\
running 1 test\n\
test same_name ... ok\n\
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n\
   Doc-tests sample\n\
running 1 test\n\
test src/lib.rs - same_name (line 7) ... ok\n\
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";
        let outcomes = parse_libtest(input).unwrap();
        assert_eq!(
            libtest_outcome_ids(&outcomes),
            [
                "libtest:alpha:tests/alpha.rs:same_name",
                "libtest:beta:tests/beta.rs:same_name",
                "libtest:sample:src/lib.rs:src/lib.rs - same_name (line 7)",
            ]
        );
        assert_eq!(outcomes[0].status, TestStatus::Failed);
        assert_eq!(
            outcomes[0].failure_message.as_deref(),
            Some("first binary assertion failed")
        );
        assert_eq!(outcomes[1].status, TestStatus::Passed);
        assert_eq!(outcomes[1].failure_message, None);
        // Cargo's build hashes do not change stable test identity.
        let rebuilt = input.replace("alpha-456def", "alpha-fedcba");
        assert_eq!(parse_libtest(&rebuilt).unwrap(), outcomes);
    }

    fn libtest_outcome_ids(outcomes: &[TestOutcome]) -> Vec<&str> {
        outcomes
            .iter()
            .map(|outcome| outcome.test_id.as_str())
            .collect()
    }

    #[test]
    fn libtest_rejects_target_changes_inside_incomplete_suites() {
        assert!(parse_libtest("Running tests/alpha.rs (target/debug/deps/alpha-123abc)\nrunning 1 test\nRunning tests/beta.rs (target/debug/deps/beta-456def)\n").is_err());
    }

    #[test]
    fn junit_parses_failures_errors_skips_and_custom_identity() {
        let input = r#"<testsuites><testsuite><testcase classname="pkg::suite" name="passes" time="0.012"/><testcase classname="pkg::suite" name="fails"><failure message="assertion failed">trace line</failure></testcase><testcase classname="pkg::suite" name="errors"><error>setup failed</error></testcase><testcase classname="pkg::suite" name="skips"><skipped/></testcase></testsuite></testsuites>"#;
        let result = parse_junit(input).unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].test_id, "pkg::suite::passes");
        assert_eq!(result[0].duration_ms, Some(12));
        assert_eq!(result[1].status, TestStatus::Failed);
        assert_eq!(
            result[1].failure_message.as_deref(),
            Some("assertion failed")
        );
        assert_eq!(result[2].status, TestStatus::Errored);
        assert_eq!(result[3].status, TestStatus::Skipped);

        let mapped = parse_junit_with(input, |class, name| {
            format!("junit:pkg:tests.rs:{class}::{name}")
        })
        .unwrap();
        assert_eq!(mapped[0].test_id, "junit:pkg:tests.rs:pkg::suite::passes");
    }

    #[test]
    fn junit_checks_aggregate_counts_and_cdata_diagnostic() {
        let input = "<testsuites tests=\"1\" failures=\"1\"><testsuite tests=\"1\" failures=\"1\"><testcase name=\"broken\"><failure><![CDATA[first failure detail\nsecond line]]></failure></testcase></testsuite></testsuites>";
        let outcomes = parse_junit(input).unwrap();
        assert_eq!(outcomes[0].status, TestStatus::Failed);
        assert_eq!(
            outcomes[0].failure_message.as_deref(),
            Some("first failure detail")
        );
    }

    #[test]
    fn junit_rejects_garbled_and_incomplete_documents() {
        assert!(parse_junit("garbled").is_err());
        assert!(parse_junit("<testsuite><testcase name=\"a\">").is_err());
        assert!(parse_junit("<testsuite/>").is_err());
        assert!(parse_junit("<testsuite><testcase name=\"a\"/>").is_err());
        assert!(parse_junit("<testsuite tests=\"2\"><testcase name=\"a\"/></testsuite>").is_err());
        assert!(parse_junit("<testsuites tests=\"2\"><testsuite tests=\"1\"><testcase name=\"a\"/></testsuite></testsuites>").is_err());
        assert!(parse_junit("<testsuite errors=\"1\"><testcase name=\"a\"/></testsuite>").is_err());
        assert!(parse_junit(
            "<testsuite><testcase name=\"a\"><skipped/><failure/></testcase></testsuite>"
        )
        .is_err());
        assert!(parse_junit("<testsuite><testcase name=\"a\"/>").is_err());
        assert!(parse_junit("<testsuite tests=\"2\"><testcase name=\"a\"/></testsuite>").is_err());
        assert!(parse_junit(
            "<testsuite><testcase name=\"a\"><skipped/><failure/></testcase></testsuite>"
        )
        .is_err());
    }
}
