//! Parsers for the supported test-runner output formats.
//!
//! Adapters reject inputs that do not contain any recognizable, complete test
//! result. This keeps a truncated or unrelated log from looking like a clean run.

use super::test_identity::{TestOutcome, TestStatus};
use quick_xml::{events::Event, Reader};
use std::collections::{BTreeMap, BTreeSet};
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
    let mut packages = BTreeMap::new();
    let mut binaries = Vec::new();

    for raw_line in input.lines() {
        let line = strip_ansi_sequences(raw_line);
        let line = line.as_str();
        let trimmed = line.trim();
        if suite_expected.is_none() && cargo_artifact(trimmed, &mut packages)? {
            continue;
        }
        if trimmed.starts_with("Running ") || trimmed.starts_with("Doc-tests ") {
            if target_pending || suite_expected.is_some() || pending_stdout_test.is_some() {
                return Err(AdapterError(
                    "libtest target changed before suite completion".into(),
                ));
            }
            finish_failure_block(&mut current_failure, &mut failure_lines);
            scope = Some(LibtestScope::from_header(trimmed, &packages)?);
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
            ids.clear();
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
            // Bind diagnostics before equal plain ids from other binaries exist.
            finish_failure_block(&mut current_failure, &mut failure_lines);
            for (id, detail) in failure_lines.drain(..) {
                if let Some(outcome) = outcomes[suite_start..]
                    .iter_mut()
                    .find(|outcome| outcome.test_id == id && outcome.status == TestStatus::Failed)
                {
                    outcome.failure_message = Some(detail);
                }
            }
            binaries.extend((suite_start..outcomes.len()).map(|_| match &scope {
                Some(LibtestScope::Binary {
                    executable,
                    package,
                    ..
                }) => Some((executable.clone(), package.clone())),
                _ => None,
            }));
        } else if let Some(rest) = trimmed.strip_prefix("test ") {
            if suite_expected.is_none() {
                return Err(AdapterError("libtest case outside a running suite".into()));
            }
            if is_libtest_timeout_notice(rest) {
                continue;
            }
            if let Some((id, result)) = rest.rsplit_once(" ... ") {
                let id = scoped_libtest_id(scope.as_ref(), libtest_name(id.trim()));
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
            current_failure = Some((
                scoped_libtest_id(scope.as_ref(), libtest_name(id)),
                Vec::new(),
            ));
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

    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, outcome) in outcomes.iter().enumerate() {
        groups.entry(outcome.test_id.clone()).or_default().push(i);
    }
    for (plain, group) in groups.iter().filter(|(_, group)| group.len() > 1) {
        let mut seen_binaries = BTreeSet::new();
        let mut seen_packages = BTreeSet::new();
        for &i in group {
            let Some((binary, Some(package))) = &binaries[i] else {
                return Err(duplicate_libtest_id(plain));
            };
            if !seen_binaries.insert(binary) || !seen_packages.insert(package) {
                return Err(duplicate_libtest_id(plain));
            }
            outcomes[i].test_id = qualify_libtest_id(plain, package);
        }
    }
    ids.clear();
    for outcome in &outcomes {
        if !ids.insert(outcome.test_id.clone()) {
            return Err(duplicate_libtest_id(&outcome.test_id));
        }
    }
    Ok(outcomes)
}

fn duplicate_libtest_id(id: &str) -> AdapterError {
    AdapterError(format!("empty or duplicate libtest id: {id}"))
}

pub(crate) fn qualify_libtest_id(id: &str, package: &str) -> String {
    format!("libtest:{package}/{}", id.strip_prefix("libtest:").unwrap())
}

pub(crate) fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn cargo_package_name(id: &str) -> AdapterResult<&str> {
    let name = if let Some((source, fragment)) = id.rsplit_once('#') {
        if let Some((name, _version)) = fragment.split_once('@') {
            name
        } else if source.starts_with("path+file://") {
            source.rsplit('/').next().unwrap_or("")
        } else {
            ""
        }
    } else {
        id.split_once(' ').map_or("", |(name, _)| name)
    };
    if !valid_package_name(name) {
        return Err(AdapterError(
            "invalid Cargo package name in libtest artifact".into(),
        ));
    }
    Ok(name)
}

fn executable_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or("")
}

/// Only Cargo artifact records outside a running suite can supply identity.
fn cargo_artifact(line: &str, packages: &mut BTreeMap<String, String>) -> AdapterResult<bool> {
    if !line.starts_with('{') {
        return Ok(false);
    }
    let value: serde_json::Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) if line.contains("compiler-artifact") => {
            return Err(AdapterError(format!(
                "invalid Cargo libtest artifact: {error}"
            )));
        }
        Err(_) => return Ok(false),
    };
    if value.get("reason").and_then(serde_json::Value::as_str) != Some("compiler-artifact") {
        return Ok(false);
    }
    let Some(executable) = value.get("executable").and_then(serde_json::Value::as_str) else {
        return Ok(false);
    };
    super::json_input::reject_duplicate_keys(line.as_bytes())
        .map_err(|error| AdapterError(format!("invalid Cargo libtest artifact: {error}")))?;
    let package = cargo_package_name(
        value
            .get("package_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(""),
    )?;
    let binary = executable_name(executable);
    if binary.is_empty() {
        return Err(AdapterError(
            "empty Cargo libtest executable file name".into(),
        ));
    }
    if packages.get(binary).is_some_and(|prior| prior != package) {
        return Err(AdapterError(format!(
            "conflicting Cargo packages for libtest executable: {binary}"
        )));
    }
    packages.insert(binary.to_owned(), package.to_owned());
    Ok(true)
}

fn strip_ansi_sequences(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut clean = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            index += 2;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
        } else if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b']') {
            index += 2;
            while index < bytes.len() {
                if bytes[index] == 0x07 {
                    index += 1;
                    break;
                }
                if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'\\') {
                    index += 2;
                    break;
                }
                index += 1;
            }
        } else {
            clean.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&clean).into_owned()
}

fn is_libtest_timeout_notice(rest: &str) -> bool {
    rest.rsplit_once(" has been running for over ")
        .and_then(|(name, duration)| duration.strip_suffix(" seconds").map(|n| (name, n)))
        .is_some_and(|(name, n)| {
            !name.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
        })
}

fn libtest_name(mut name: &str) -> &str {
    loop {
        if let Some(prefix) = name
            .strip_suffix(" - should panic")
            .or_else(|| name.strip_suffix(" - compile fail"))
        {
            name = prefix;
        } else if let Some((prefix, line)) = name.rsplit_once(" (line ") {
            if !prefix.contains(" - ")
                && line
                    .strip_suffix(')')
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            {
                name = prefix;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    name
}

enum LibtestScope {
    Binary {
        target: String,
        file: String,
        executable: String,
        package: Option<String>,
    },
    DocTests {
        target: String,
    },
}
impl LibtestScope {
    fn from_header(header: &str, packages: &BTreeMap<String, String>) -> AdapterResult<Self> {
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
            let executable = executable_name(executable);
            let binary = executable;
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
            if !file.is_empty() && !target.is_empty() {
                return Ok(Self::Binary {
                    target: target.into(),
                    file: file.into(),
                    executable: executable.into(),
                    package: packages.get(executable).cloned(),
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
        Some(LibtestScope::Binary { target, file, .. }) => {
            format!("libtest:{target}:{file}:{name}")
        }
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
            Ok(Event::GeneralRef(event)) => {
                let resolved = event
                    .resolve_char_ref()
                    .map_err(|e| AdapterError(format!("invalid JUnit reference: {e}")))?
                    .map(|c| c.to_string())
                    .or_else(|| {
                        quick_xml::escape::resolve_predefined_entity(event.as_ref())
                            .map(str::to_owned)
                    })
                    .ok_or_else(|| AdapterError("unresolved JUnit entity reference".into()))?;
                if stack.is_empty() && !resolved.trim().is_empty() {
                    return Err(AdapterError("text outside JUnit root".into()));
                }
                if stack
                    .last()
                    .is_some_and(|frame| frame.name == "failure" || frame.name == "error")
                    && case_detail.is_none()
                    && !resolved.trim().is_empty()
                {
                    case_detail = Some(resolved);
                }
            }
            Ok(Event::DocType(_)) => {
                return Err(AdapterError("JUnit DOCTYPE is not supported".into()))
            }
            Ok(Event::Eof) => break,
            Ok(
                Event::Text(_)
                | Event::CData(_)
                | Event::Comment(_)
                | Event::Decl(_)
                | Event::PI(_),
            ) => {}
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
mod package_tests {
    use super::*;

    fn artifact(binary: &str, package_id: &str) -> String {
        serde_json::json!({
            "reason": "compiler-artifact", "package_id": package_id,
            "executable": format!("/capture/target/debug/deps/{binary}")
        })
        .to_string()
    }

    fn suite(binary: &str, name: &str) -> String {
        format!("Running tests/integration.rs (target/debug/deps/{binary})\nrunning 1 test\ntest {name} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n")
    }

    fn capture() -> String {
        format!(
            "{}\n{}\n{}{}",
            artifact("integration-abc", "path+file:///fixture/alpha#0.1.0"),
            artifact("integration-def", "path+file:///fixture/x#beta@0.1.0"),
            suite("integration-abc", "smoke"),
            suite("integration-def", "smoke")
        )
    }

    #[test]
    fn package_id_forms_keep_cargo_package_spelling() {
        for id in [
            "path+file:///fixture/alpha-test_2#0.1.0",
            "path+file:///fixture/x#alpha-test_2@0.1.0",
            "registry+https://example.invalid/index#alpha-test_2@0.1.0",
            "alpha-test_2 0.1.0 (path+file:///fixture/x)",
        ] {
            assert_eq!(cargo_package_name(id).unwrap(), "alpha-test_2");
        }
        for id in [
            "",
            "path+file:///fixture/x#@0.1.0",
            "registry+index#bad/name@1.0.0",
            "registry+index#café@1.0.0",
        ] {
            assert!(cargo_package_name(id).is_err(), "{id}");
        }
    }

    #[test]
    fn unique_outcomes_keep_plain_ids_with_package_context() {
        let input = format!(
            "{}\n{}",
            artifact(
                "integration-abc",
                "alpha 0.1.0 (path+file:///fixture/alpha)"
            ),
            suite("integration-abc", "unique")
        );
        assert_eq!(
            parse_libtest(&input).unwrap()[0].test_id,
            "libtest:integration:tests/integration.rs:unique"
        );
    }

    #[test]
    fn artifact_conflicts_and_invalid_records_fail_closed() {
        let alpha = artifact("integration-abc", "path+file:///fixture/alpha#0.1.0");
        let cases = [
            artifact("integration-abc", "path+file:///fixture/beta#0.1.0"),
            alpha.replace(
                "\"reason\":",
                "\"reason\":\"compiler-artifact\",\"reason\":",
            ),
            alpha.replace(
                "\"reason\":",
                "\"nested\":{\"key\":1,\"key\":2},\"reason\":",
            ),
            alpha.trim_end_matches('}').to_owned(),
            artifact("integration-aaa", "path+file:///fixture/x#bad/name@0.1.0"),
        ];
        for bad in cases {
            assert!(
                parse_libtest(&format!(
                    "{alpha}\n{bad}\n{}",
                    suite("integration-abc", "unique")
                ))
                .is_err(),
                "{bad}"
            );
        }
        // Repeated identical mappings are harmless.
        assert!(parse_libtest(&format!(
            "{alpha}\n{alpha}\n{}",
            suite("integration-abc", "unique")
        ))
        .is_ok());
    }

    #[test]
    fn artifacts_inside_suites_are_only_test_output() {
        let input = capture();
        let injected = input.replace(
            "running 1 test\n",
            &format!(
                "running 1 test\n{}\n{{\"reason\":\"compiler-artifact\",\n",
                artifact("integration-abc", "path+file:///fixture/spoof#0.1.0")
            ),
        );
        assert_eq!(
            parse_libtest(&injected).unwrap(),
            parse_libtest(&input).unwrap()
        );
        // A record first printed inside a suite cannot supply missing context.
        let no_context = format!(
            "{}{}",
            suite("integration-abc", "smoke"),
            suite("integration-def", "smoke")
        )
        .replace(
            "running 1 test\n",
            &format!(
                "running 1 test\n{}\n",
                artifact("integration-abc", "path+file:///fixture/alpha#0.1.0")
            ),
        );
        assert!(parse_libtest(&no_context)
            .unwrap_err()
            .0
            .contains("duplicate libtest id"));
    }

    #[test]
    fn missing_or_indistinguishable_binary_context_cannot_disambiguate() {
        let input = capture();
        for ambiguous in [
            input.replace("/x#beta@0.1.0", "/x#alpha@0.1.0"),
            input.replace(
                "\"executable\":\"/capture/target/debug/deps/integration-def\"",
                "\"executable\":null",
            ),
            format!("{}{}", input, suite("integration-abc", "smoke")),
        ] {
            assert!(parse_libtest(&ambiguous)
                .unwrap_err()
                .0
                .contains("duplicate libtest id"));
        }
        let duplicate_row = input.replace(
            "test smoke ... ok\n",
            "test smoke ... ok\ntest smoke ... ok\n",
        );
        assert!(parse_libtest(&duplicate_row)
            .unwrap_err()
            .0
            .contains("duplicate libtest id"));
    }

    #[test]
    fn qualified_failures_keep_each_binarys_diagnostic() {
        let input = capture().replace(" ... ok", " ... FAILED").replace(
            "test result: ok. 1 passed; 0 failed",
            "test result: FAILED. 0 passed; 1 failed",
        );
        let mut count = 0;
        let input = input
            .lines()
            .map(|line| {
                if line.starts_with("test result:") {
                    count += 1;
                    format!("---- smoke stdout ----\nfailure in binary {count}\n{line}")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let outcomes = parse_libtest(&input).unwrap();
        assert_eq!(
            outcomes[0].test_id,
            "libtest:alpha/integration:tests/integration.rs:smoke"
        );
        assert_eq!(
            outcomes[0].failure_message.as_deref(),
            Some("failure in binary 1")
        );
        assert_eq!(
            outcomes[1].test_id,
            "libtest:beta/integration:tests/integration.rs:smoke"
        );
        assert_eq!(
            outcomes[1].failure_message.as_deref(),
            Some("failure in binary 2")
        );
    }
}
