//! Parsers for the supported test-runner output formats.
//!
//! Adapters reject inputs that do not contain any recognizable, complete test
//! result. This keeps a truncated or unrelated log from looking like a clean run.

use crate::core::test_identity::{TestOutcome, TestStatus};
use quick_xml::{events::Event, Reader};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterError(pub String);

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for AdapterError {}

pub type AdapterResult<T> = Result<T, AdapterError>;

/// Parse cargo/libtest output. Failure details are attached to each failed case
/// when the harness printed a `---- test_id stdout ----` block.
pub fn parse_libtest(input: &str) -> AdapterResult<Vec<TestOutcome>> {
    let mut outcomes: Vec<TestOutcome> = Vec::new();
    let mut ids = BTreeSet::new();
    let mut current_failure: Option<(String, Vec<String>)> = None;
    let mut failure_lines = Vec::new();
    let mut suite_expected = None;
    let mut suite_start = 0usize;
    let mut suites = 0usize;
    let mut pending_stdout_test: Option<String> = None;
    let mut scope: Option<LibtestScope> = None;
    let mut target_pending = false;

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Running ") || trimmed.starts_with("Doc-tests ") {
            if target_pending || suite_expected.is_some() || pending_stdout_test.is_some() {
                return Err(AdapterError(
                    "libtest target changed before suite completion".into(),
                ));
            }
            finish_failure_block(&mut current_failure, &mut failure_lines);
            scope = Some(LibtestScope::from_header(trimmed)?);
            target_pending = true;
        } else if let Some(rest) = trimmed.strip_prefix("running ") {
            if suite_expected.is_some() {
                return Err(AdapterError(
                    "new libtest suite started before prior suite summary".into(),
                ));
            }
            let (count, noun) = rest
                .split_once(' ')
                .ok_or_else(|| AdapterError(format!("malformed libtest header: {line}")))?;
            let expected = count
                .parse::<usize>()
                .map_err(|_| AdapterError(format!("malformed libtest header: {line}")))?;
            if !matches!(noun, "test" | "tests") {
                return Err(AdapterError(format!("malformed libtest header: {line}")));
            }
            suite_expected = Some(expected);
            suite_start = outcomes.len();
        } else if trimmed.starts_with("test result:") {
            let summary = parse_libtest_summary(trimmed)?;
            let Some(expected) = suite_expected.take() else {
                return Err(AdapterError(
                    "libtest summary without running header".into(),
                ));
            };
            let parsed = outcomes.len() - suite_start;
            if parsed != summary.executed || parsed != expected {
                return Err(AdapterError(format!(
                    "incomplete libtest suite: header {expected}, summary {}, parsed {parsed}",
                    summary.executed
                )));
            }
            let suite_outcomes = &outcomes[suite_start..];
            let passed = suite_outcomes
                .iter()
                .filter(|outcome| outcome.status == TestStatus::Passed)
                .count();
            let failed = suite_outcomes
                .iter()
                .filter(|outcome| {
                    matches!(outcome.status, TestStatus::Failed | TestStatus::Errored)
                })
                .count();
            let ignored = suite_outcomes
                .iter()
                .filter(|outcome| outcome.status == TestStatus::Skipped)
                .count();
            if passed != summary.passed
                || failed != summary.failed
                || ignored != summary.ignored
                || summary.measured != 0
                || summary.failed_summary != (failed > 0)
            {
                return Err(AdapterError(
                    "libtest summary statuses disagree with parsed case results".into(),
                ));
            }
            suites += 1;
            target_pending = false;
        } else if let Some(rest) = trimmed.strip_prefix("test ") {
            if suite_expected.is_none() {
                return Err(AdapterError("libtest case outside a running suite".into()));
            }
            if let Some((id, result)) = rest.rsplit_once(" ... ") {
                let id = scoped_libtest_id(scope.as_ref(), id.trim());
                let (token, suffix) = result
                    .split_once(char::is_whitespace)
                    .map_or((result, ""), |(a, b)| (a, b.trim()));
                if !matches!(token, "ok" | "FAILED" | "ignored" | "ignored,") {
                    if id.is_empty() || pending_stdout_test.replace(id.to_owned()).is_some() {
                        return Err(AdapterError(format!(
                            "invalid interleaved libtest row: {line}"
                        )));
                    }
                    continue;
                }
                add_libtest_outcome(&id, token, suffix, &mut ids, &mut outcomes)?;
            } else {
                return Err(AdapterError(format!("malformed libtest case: {line}")));
            }
        } else if let Some(id) = pending_stdout_test.as_ref() {
            if matches!(trimmed, "ok" | "FAILED" | "ignored" | "ignored,") {
                add_libtest_outcome(id, trimmed, "", &mut ids, &mut outcomes)?;
                pending_stdout_test = None;
            }
        }

        if let Some(id) = trimmed
            .strip_prefix("---- ")
            .and_then(|s| s.strip_suffix(" stdout ----"))
        {
            finish_failure_block(&mut current_failure, &mut failure_lines);
            current_failure = Some((scoped_libtest_id(scope.as_ref(), id), Vec::new()));
        } else if let Some((_, lines)) = current_failure.as_mut() {
            if !trimmed.is_empty() {
                lines.push(trimmed.to_owned());
            }
        }
    }
    finish_failure_block(&mut current_failure, &mut failure_lines);

    if target_pending || pending_stdout_test.is_some() || suite_expected.is_some() || suites == 0 {
        return Err(AdapterError(
            "incomplete libtest output: missing suite completion".into(),
        ));
    }
    if outcomes.is_empty() {
        return Err(AdapterError("libtest output has zero test cases".into()));
    }

    for (id, detail) in failure_lines {
        if let Some(outcome) = outcomes
            .iter_mut()
            .find(|outcome| outcome.test_id == id && outcome.status == TestStatus::Failed)
        {
            outcome.failure_message = Some(detail);
        }
    }
    Ok(outcomes)
}

enum LibtestScope {
    Binary { target: String, file: String },
    DocTests { target: String },
}
impl LibtestScope {
    fn from_header(header: &str) -> AdapterResult<Self> {
        if let Some(target) = header.strip_prefix("Doc-tests ").map(str::trim) {
            if !target.is_empty() {
                return Ok(Self::DocTests {
                    target: target.into(),
                });
            }
        } else if let Some((source, executable)) = header
            .strip_prefix("Running ")
            .and_then(|s| s.strip_suffix(')'))
            .and_then(|s| s.rsplit_once(" ("))
        {
            let file = source.strip_prefix("unittests ").unwrap_or(source).trim();
            let binary = executable.rsplit('/').next().unwrap_or("");
            let binary = binary.strip_suffix(".exe").unwrap_or(binary);
            // Cargo appends a build hash, which is not part of stable identity.
            let target = match binary.rsplit_once('-') {
                Some((target, hash))
                    if !hash.is_empty() && hash.bytes().all(|b| b.is_ascii_hexdigit()) =>
                {
                    target
                }
                _ => binary,
            };
            // Unit-test target names are logical harnesses, independent of package spelling.
            let target = match source.trim() {
                "unittests src/lib.rs" => "lib",
                "unittests src/main.rs" => "bin",
                _ => target,
            };
            if !file.is_empty() && !target.is_empty() {
                return Ok(Self::Binary {
                    target: target.into(),
                    file: file.into(),
                });
            }
        }
        Err(AdapterError(format!(
            "malformed libtest target header: {header}"
        )))
    }
}
fn scoped_libtest_id(scope: Option<&LibtestScope>, name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    match scope {
        Some(LibtestScope::Binary { target, file }) => format!("libtest:{target}:{file}:{name}"),
        Some(LibtestScope::DocTests { target }) => {
            let file = name.split_whitespace().next().unwrap_or("");
            format!("libtest:{target}:{file}:{name}")
        }
        // Standalone harness captures have no target metadata; keep their explicit ids.
        None => name.to_owned(),
    }
}

fn add_libtest_outcome(
    id: &str,
    token: &str,
    suffix: &str,
    ids: &mut BTreeSet<String>,
    outcomes: &mut Vec<TestOutcome>,
) -> AdapterResult<()> {
    let status = match token {
        "ok" => TestStatus::Passed,
        "FAILED" => TestStatus::Failed,
        "ignored" => TestStatus::Skipped,
        "ignored," => TestStatus::Skipped,
        _ => {
            return Err(AdapterError(format!(
                "unrecognized libtest result: {token}"
            )))
        }
    };
    let id = id.trim();
    if id.is_empty() || !ids.insert(id.to_owned()) {
        return Err(AdapterError(format!("empty or duplicate libtest id: {id}")));
    }
    let duration_ms = if token == "ignored," {
        None
    } else {
        parse_libtest_duration(suffix)?
    };
    outcomes.push(TestOutcome {
        test_id: id.to_owned(),
        status,
        duration_ms,
        failure_message: None,
    });
    Ok(())
}

fn parse_libtest_duration(suffix: &str) -> AdapterResult<Option<u64>> {
    let suffix = suffix.trim();
    if suffix.is_empty() {
        return Ok(None);
    }
    let Some(seconds) = suffix
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .and_then(|s| s.strip_suffix('s'))
    else {
        return Err(AdapterError(format!(
            "unrecognized libtest suffix: {suffix}"
        )));
    };
    let seconds: f64 = seconds
        .parse()
        .map_err(|_| AdapterError(format!("invalid libtest duration: {suffix}")))?;
    if !seconds.is_finite() || seconds < 0.0 || seconds * 1000.0 > u64::MAX as f64 {
        return Err(AdapterError(format!("invalid libtest duration: {suffix}")));
    }
    Ok(Some((seconds * 1000.0).round() as u64))
}

struct LibtestSummary {
    executed: usize,
    passed: usize,
    failed: usize,
    ignored: usize,
    measured: usize,
    failed_summary: bool,
}

fn parse_libtest_summary(summary: &str) -> AdapterResult<LibtestSummary> {
    // libtest: `test result: FAILED. 1 passed; 1 failed; 0 ignored; ...`
    let (result_kind, after) = summary
        .strip_prefix("test result:")
        .and_then(|rest| rest.trim().split_once('.'))
        .ok_or_else(|| AdapterError(format!("malformed libtest summary: {summary}")))?;
    let mut counts = [None; 5];
    for field in after.split(';') {
        let mut words = field.split_whitespace();
        let Some(number) = words.next() else { continue };
        let Ok(count) = number.parse::<usize>() else {
            continue;
        };
        let Some(kind) = words.next() else { continue };
        let index = match kind {
            "passed" => Some(0),
            "failed" => Some(1),
            "ignored" => Some(2),
            "measured" => Some(3),
            "filtered" => Some(4),
            _ => None,
        };
        if let Some(index) = index {
            if counts[index].replace(count).is_some() {
                return Err(AdapterError(format!(
                    "duplicate libtest summary count: {kind}"
                )));
            }
        }
    }
    if counts.iter().any(Option::is_none) {
        return Err(AdapterError(format!(
            "malformed libtest summary: {summary}"
        )));
    }
    let [passed, failed, ignored, measured, _filtered] = counts.map(Option::unwrap);
    let failed_summary = match result_kind.trim() {
        "FAILED" => true,
        "ok" => false,
        _ => {
            return Err(AdapterError(format!(
                "malformed libtest result kind: {result_kind}"
            )))
        }
    };
    if failed_summary != (failed > 0) {
        return Err(AdapterError(format!(
            "libtest summary result disagrees with counts: {summary}"
        )));
    }
    let executed = passed
        .checked_add(failed)
        .and_then(|n| n.checked_add(ignored))
        .and_then(|n| n.checked_add(measured))
        .ok_or_else(|| AdapterError("libtest summary count overflow".into()))?;
    Ok(LibtestSummary {
        executed,
        passed,
        failed,
        ignored,
        measured,
        failed_summary,
    })
}

fn finish_failure_block(
    current: &mut Option<(String, Vec<String>)>,
    details: &mut Vec<(String, String)>,
) {
    if let Some((id, lines)) = current.take() {
        if let Some(first) = lines.first() {
            details.push((id, first.clone()));
        }
    }
}

/// Parse JUnit XML using the conventional `classname::name` identity.
pub fn parse_junit(input: &str) -> AdapterResult<Vec<TestOutcome>> {
    parse_junit_with(input, |classname, name| {
        if classname.is_empty() {
            name.to_owned()
        } else {
            format!("{classname}::{name}")
        }
    })
}

/// Parse JUnit XML with a caller-provided test identity mapping.
///
/// The callback receives the decoded `classname` and `name` attributes. JUnit
/// does not carry the packet's scoped framework/package/file identity, so callers
/// that need that identity must supply it through this mapping.
pub fn parse_junit_with<F>(input: &str, mut make_id: F) -> AdapterResult<Vec<TestOutcome>>
where
    F: FnMut(&str, &str) -> String,
{
    let mut reader = Reader::from_str(input);
    reader.config_mut().trim_text(true);
    let mut outcomes = Vec::new();
    let mut ids = BTreeSet::new();
    let mut stack = Vec::<XmlFrame>::new();
    let mut case_id = String::new();
    let mut case_status: Option<TestStatus> = None;
    let mut case_detail: Option<String> = None;
    let mut case_duration = None;
    let mut cases = 0usize;
    let mut root_seen = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = event.local_name().as_ref().to_owned();
                validate_junit_child(&stack, &name, root_seen)?;
                if stack.is_empty() {
                    root_seen = true;
                }
                let attrs = attributes(&event)?;
                match name.as_str() {
                    "testsuite" | "testsuites" => {
                        stack.push(suite_frame(name, &attrs)?);
                    }
                    "testcase" | "properties" | "property" | "system-out" | "system-err" => {
                        if name == "testcase" {
                            begin_case(
                                &attrs,
                                &mut make_id,
                                &mut ids,
                                &mut case_id,
                                &mut case_status,
                                &mut case_detail,
                                &mut case_duration,
                            )?;
                        }
                        stack.push(XmlFrame::plain(name));
                    }
                    "failure" | "error" | "skipped" => {
                        if stack.last().map(|frame| frame.name.as_str()) == Some("testcase") {
                            set_case_status(&mut case_status, status_for(&name))?;
                            case_detail = attrs
                                .get("message")
                                .cloned()
                                .filter(|s| !s.trim().is_empty());
                        }
                        stack.push(XmlFrame::plain(name));
                    }
                    _ => unreachable!("child validation rejects unsupported elements"),
                }
            }
            Ok(Event::Empty(event)) => {
                let name = event.local_name().as_ref().to_owned();
                validate_junit_child(&stack, &name, root_seen)?;
                if stack.is_empty() {
                    root_seen = true;
                }
                let attrs = attributes(&event)?;
                match name.as_str() {
                    "testsuite" | "testsuites" => {
                        validate_frame_counts(&suite_frame(name, &attrs)?)?;
                    }
                    "testcase" => {
                        begin_case(
                            &attrs,
                            &mut make_id,
                            &mut ids,
                            &mut case_id,
                            &mut case_status,
                            &mut case_detail,
                            &mut case_duration,
                        )?;
                        finish_case(
                            &mut outcomes,
                            &mut case_id,
                            &mut case_status,
                            &mut case_detail,
                            &mut case_duration,
                        );
                        cases += 1;
                        increment_suite_case_counts(&mut stack, TestStatus::Passed);
                    }
                    "failure" | "error" | "skipped" => {
                        set_case_status(&mut case_status, status_for(&name))?;
                        case_detail = attrs
                            .get("message")
                            .cloned()
                            .filter(|s| !s.trim().is_empty());
                    }
                    "property" | "properties" | "system-out" | "system-err" => {}
                    _ => unreachable!("child validation rejects unsupported elements"),
                }
            }
            Ok(Event::Text(event))
                if stack
                    .last()
                    .is_some_and(|frame| frame.name == "failure" || frame.name == "error")
                    && case_detail.is_none() =>
            {
                let text = quick_xml::escape::unescape(event.as_ref())
                    .map_err(|e| AdapterError(format!("invalid JUnit text: {e}")))?;
                let first = text.lines().map(str::trim).find(|s| !s.is_empty());
                if let Some(first) = first {
                    case_detail = Some(first.to_owned());
                }
            }
            Ok(Event::CData(event))
                if stack
                    .last()
                    .is_some_and(|frame| frame.name == "failure" || frame.name == "error")
                    && case_detail.is_none() =>
            {
                let text = event.as_ref();
                if let Some(first) = text.lines().map(str::trim).find(|s| !s.is_empty()) {
                    case_detail = Some(first.to_owned());
                }
            }
            Ok(Event::End(event)) => {
                let name = event.local_name().as_ref().to_owned();
                let frame = stack
                    .pop()
                    .ok_or_else(|| AdapterError(format!("unexpected JUnit close tag: {name}")))?;
                if frame.name != name {
                    return Err(AdapterError(format!("mismatched JUnit close tag: {name}")));
                }
                if name == "testcase" {
                    let status = case_status.unwrap_or(TestStatus::Passed);
                    increment_suite_case_counts(&mut stack, status);
                    finish_case(
                        &mut outcomes,
                        &mut case_id,
                        &mut case_status,
                        &mut case_detail,
                        &mut case_duration,
                    );
                    cases += 1;
                } else if name == "testsuite" || name == "testsuites" {
                    validate_frame_counts(&frame)?;
                }
            }
            Ok(Event::Text(event)) if stack.is_empty() => {
                let text = quick_xml::escape::unescape(event.as_ref())
                    .map_err(|e| AdapterError(format!("invalid JUnit text: {e}")))?;
                if !text.trim().is_empty() {
                    return Err(AdapterError("text outside JUnit root".into()));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(AdapterError(format!("invalid JUnit XML: {error}"))),
        }
    }
    if !stack.is_empty() {
        return Err(AdapterError(format!(
            "incomplete JUnit document: open {}",
            stack.last().unwrap().name
        )));
    }
    if !root_seen || cases == 0 {
        return Err(AdapterError(
            "JUnit input contains no testcases (zero-case reports are errors)".into(),
        ));
    }
    Ok(outcomes)
}

struct XmlFrame {
    name: String,
    declared: Option<usize>,
    declared_failures: Option<usize>,
    declared_errors: Option<usize>,
    declared_skipped: Option<usize>,
    cases: usize,
    failures: usize,
    errors: usize,
    skipped: usize,
}

impl XmlFrame {
    fn plain(name: String) -> Self {
        Self {
            name,
            declared: None,
            declared_failures: None,
            declared_errors: None,
            declared_skipped: None,
            cases: 0,
            failures: 0,
            errors: 0,
            skipped: 0,
        }
    }
}

fn validate_junit_child(stack: &[XmlFrame], child: &str, root_seen: bool) -> AdapterResult<()> {
    let parent = stack.last().map(|frame| frame.name.as_str());
    let allowed = match parent {
        None => !root_seen && matches!(child, "testsuite" | "testsuites"),
        Some("testsuites") => matches!(child, "testsuite" | "properties"),
        Some("testsuite") => matches!(
            child,
            "testsuite" | "testcase" | "properties" | "system-out" | "system-err"
        ),
        Some("testcase") => matches!(
            child,
            "failure" | "error" | "skipped" | "system-out" | "system-err"
        ),
        Some("properties") => child == "property",
        Some("failure" | "error" | "skipped" | "property" | "system-out" | "system-err") => false,
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(AdapterError(format!(
            "unexpected JUnit element {child} under {}",
            parent.unwrap_or("document")
        )))
    }
}

fn begin_case<F>(
    attrs: &std::collections::BTreeMap<String, String>,
    make_id: &mut F,
    ids: &mut BTreeSet<String>,
    case_id: &mut String,
    case_status: &mut Option<TestStatus>,
    case_detail: &mut Option<String>,
    case_duration: &mut Option<u64>,
) -> AdapterResult<()>
where
    F: FnMut(&str, &str) -> String,
{
    let name = attrs.get("name").map(String::as_str).unwrap_or("");
    if name.is_empty() {
        return Err(AdapterError("JUnit testcase is missing name".into()));
    }
    let classname = attrs.get("classname").map(String::as_str).unwrap_or("");
    *case_id = make_id(classname, name);
    if case_id.trim().is_empty() || !ids.insert(case_id.clone()) {
        return Err(AdapterError(format!(
            "empty or duplicate JUnit test id: {case_id}"
        )));
    }
    *case_status = None;
    *case_detail = None;
    *case_duration = duration_ms(attrs)?;
    Ok(())
}

fn status_for(element: &str) -> TestStatus {
    match element {
        "failure" => TestStatus::Failed,
        "error" => TestStatus::Errored,
        "skipped" => TestStatus::Skipped,
        _ => unreachable!("only status elements are passed"),
    }
}

fn suite_frame(
    name: String,
    attrs: &std::collections::BTreeMap<String, String>,
) -> AdapterResult<XmlFrame> {
    fn parse_count(
        attrs: &std::collections::BTreeMap<String, String>,
        key: &str,
    ) -> AdapterResult<Option<usize>> {
        attrs
            .get(key)
            .map(|value| {
                value
                    .parse::<usize>()
                    .map_err(|_| AdapterError(format!("invalid JUnit {key} count")))
            })
            .transpose()
    }
    let mut frame = XmlFrame::plain(name);
    frame.declared = parse_count(attrs, "tests")?;
    frame.declared_failures = parse_count(attrs, "failures")?;
    frame.declared_errors = parse_count(attrs, "errors")?;
    frame.declared_skipped = parse_count(attrs, "skipped")?;
    Ok(frame)
}

fn set_case_status(slot: &mut Option<TestStatus>, next: TestStatus) -> AdapterResult<()> {
    if slot.replace(next).is_some() {
        return Err(AdapterError(
            "JUnit testcase has conflicting or duplicate result elements".into(),
        ));
    }
    Ok(())
}

fn finish_case(
    outcomes: &mut Vec<TestOutcome>,
    case_id: &mut String,
    case_status: &mut Option<TestStatus>,
    case_detail: &mut Option<String>,
    case_duration: &mut Option<u64>,
) {
    outcomes.push(TestOutcome {
        test_id: std::mem::take(case_id),
        status: case_status.take().unwrap_or(TestStatus::Passed),
        duration_ms: case_duration.take(),
        failure_message: case_detail.take(),
    });
}

fn increment_suite_case_counts(stack: &mut [XmlFrame], status: TestStatus) {
    for frame in stack
        .iter_mut()
        .filter(|frame| frame.name == "testsuite" || frame.name == "testsuites")
    {
        frame.cases += 1;
        match status {
            TestStatus::Failed => frame.failures += 1,
            TestStatus::Errored => frame.errors += 1,
            TestStatus::Skipped => frame.skipped += 1,
            TestStatus::Passed => {}
        }
    }
}

fn validate_frame_counts(frame: &XmlFrame) -> AdapterResult<()> {
    for (field, expected, actual) in [
        ("tests", frame.declared, frame.cases),
        ("failures", frame.declared_failures, frame.failures),
        ("errors", frame.declared_errors, frame.errors),
        ("skipped", frame.declared_skipped, frame.skipped),
    ] {
        if let Some(expected) = expected {
            if expected != actual {
                return Err(AdapterError(format!(
                    "incomplete JUnit suite: declared {expected} {field}, parsed {actual}"
                )));
            }
        }
    }
    Ok(())
}

fn attributes(
    event: &quick_xml::events::BytesStart<'_>,
) -> AdapterResult<std::collections::BTreeMap<String, String>> {
    let mut values = std::collections::BTreeMap::new();
    for attribute in event.attributes() {
        let attribute =
            attribute.map_err(|e| AdapterError(format!("invalid JUnit attribute: {e}")))?;
        // Extension attributes (such as q:name) must never alias JUnit's
        // unqualified identity, count, or duration attributes.
        let key = attribute.key.as_ref().to_owned();
        let value = attribute
            .normalized_value(Default::default())
            .map_err(|e| AdapterError(format!("invalid JUnit attribute value: {e}")))?
            .into_owned();
        if values.insert(key, value).is_some() {
            return Err(AdapterError("duplicate JUnit attribute".into()));
        }
    }
    Ok(values)
}

fn duration_ms(attrs: &std::collections::BTreeMap<String, String>) -> AdapterResult<Option<u64>> {
    let Some(seconds) = attrs.get("time") else {
        return Ok(None);
    };
    let seconds: f64 = seconds
        .parse()
        .map_err(|_| AdapterError(format!("invalid JUnit duration: {seconds}")))?;
    if !seconds.is_finite() || seconds < 0.0 || seconds * 1000.0 > u64::MAX as f64 {
        return Err(AdapterError(format!("invalid JUnit duration: {seconds}")));
    }
    Ok(Some((seconds * 1000.0).round() as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

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
            outcomes
                .iter()
                .map(|o| o.test_id.as_str())
                .collect::<Vec<_>>(),
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
