use living_spec::core::test_identity::{TestOutcome, TestStatus};

/// Supported result bytes for synthetic saved-score fixtures.
pub fn junit(outcomes: &[TestOutcome]) -> String {
    let mut xml = String::from("<testsuite>");
    if outcomes.is_empty() {
        xml.push_str("<testcase name=\"unlinked-fixture\"/>");
    }
    for outcome in outcomes {
        xml.push_str(&format!(
            "<testcase name=\"{}\"{}>",
            quick_xml::escape::escape(&outcome.test_id),
            outcome.duration_ms.map_or_else(String::new, |ms| format!(
                " time=\"{}.{:03}\"",
                ms / 1000,
                ms % 1000
            )),
        ));
        let element = match outcome.status {
            TestStatus::Passed => None,
            TestStatus::Failed => Some("failure"),
            TestStatus::Errored => Some("error"),
            TestStatus::Skipped => Some("skipped"),
        };
        if let Some(element) = element {
            xml.push_str(&format!("<{element}/>"));
        }
        xml.push_str("</testcase>");
    }
    xml.push_str("</testsuite>");
    xml
}
