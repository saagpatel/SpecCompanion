use crate::errors::AppError;
use crate::models::spec::Requirement;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

const MAX_DEPTH: usize = 12;
const MAX_FILE_BYTES: u64 = 1_024_000;
const IGNORE_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "target",
    ".next",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
];
const UNSUPPORTED_SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "go", "java", "rb", "cs", "c", "h", "cpp", "cc", "swift", "kt", "kts", "php",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementationCandidate {
    pub path: String,
    pub line: i64,
    pub symbol: String,
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCandidate {
    pub path: String,
    pub language: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEvidenceScan {
    pub implementation: Vec<ImplementationCandidate>,
    pub tests: Vec<TestCandidate>,
    pub checked_languages: Vec<String>,
    pub skipped_languages: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertionQuality {
    Meaningful(Vec<i64>),
    Placeholder(Vec<i64>),
    Missing,
}

pub fn scan_project(root: &str, exclusions: &[String]) -> Result<ProjectEvidenceScan, AppError> {
    let root = std::fs::canonicalize(root).map_err(AppError::Io)?;
    if !root.is_dir() {
        return Err(AppError::InvalidInput(
            "Project root is not a directory".into(),
        ));
    }

    let mut implementation = Vec::new();
    let mut tests = Vec::new();
    let mut checked = BTreeSet::new();
    let mut skipped = BTreeSet::new();
    let mut diagnostics = Vec::new();
    walk(
        &root,
        &root,
        exclusions,
        0,
        &mut implementation,
        &mut tests,
        &mut checked,
        &mut skipped,
        &mut diagnostics,
    )?;
    implementation.sort_by(|a, b| (&a.path, a.line, &a.symbol).cmp(&(&b.path, b.line, &b.symbol)));
    implementation.dedup();
    tests.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(ProjectEvidenceScan {
        implementation,
        tests,
        checked_languages: checked.into_iter().collect(),
        skipped_languages: skipped.into_iter().collect(),
        diagnostics,
    })
}

#[allow(clippy::too_many_arguments)]
fn walk(
    dir: &Path,
    root: &Path,
    exclusions: &[String],
    depth: usize,
    implementation: &mut Vec<ImplementationCandidate>,
    tests: &mut Vec<TestCandidate>,
    checked: &mut BTreeSet<String>,
    skipped: &mut BTreeSet<String>,
    diagnostics: &mut Vec<String>,
) -> Result<(), AppError> {
    if depth > MAX_DEPTH {
        diagnostics.push(format!(
            "Skipped directories deeper than {MAX_DEPTH} levels"
        ));
        return Ok(());
    }
    let mut entries = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            diagnostics.push(format!("Skipped symlink: {}", relative(root, &path)));
            continue;
        }
        if file_type.is_dir() {
            if IGNORE_DIRS.contains(&name.as_str()) || exclusions.iter().any(|value| value == &name)
            {
                continue;
            }
            let canonical = std::fs::canonicalize(&path)?;
            if !canonical.starts_with(root) {
                diagnostics.push(format!(
                    "Skipped directory outside project: {}",
                    relative(root, &path)
                ));
                continue;
            }
            walk(
                &canonical,
                root,
                exclusions,
                depth + 1,
                implementation,
                tests,
                checked,
                skipped,
                diagnostics,
            )?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let Some(ext) = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_lowercase)
        else {
            continue;
        };
        let language = match ext.as_str() {
            "js" | "jsx" => Some("javascript"),
            "ts" | "tsx" => Some("typescript"),
            "py" => Some("python"),
            value if UNSUPPORTED_SOURCE_EXTENSIONS.contains(&value) => {
                skipped.insert(value.to_string());
                None
            }
            _ => None,
        };
        let Some(language) = language else { continue };
        if std::fs::metadata(&path)?.len() > MAX_FILE_BYTES {
            diagnostics.push(format!(
                "Skipped oversized source file: {}",
                relative(root, &path)
            ));
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                diagnostics.push(format!(
                    "Unreadable source {}: {error}",
                    relative(root, &path)
                ));
                continue;
            }
        };
        checked.insert(language.to_string());
        if is_test_path(root, &path) {
            tests.push(TestCandidate {
                path: relative(root, &path),
                language: language.to_string(),
                code: content,
            });
            continue;
        }
        extract_implementation(&content, &relative(root, &path), language, implementation);
    }
    Ok(())
}

fn extract_implementation(
    content: &str,
    path: &str,
    language: &str,
    output: &mut Vec<ImplementationCandidate>,
) {
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        let names = match language {
            "python" => extract_python_names(trimmed),
            _ => extract_javascript_names(trimmed),
        };
        for symbol in names {
            output.push(ImplementationCandidate {
                path: path.to_string(),
                line: index as i64 + 1,
                symbol,
                language: language.to_string(),
            });
        }
    }
}

fn extract_python_names(line: &str) -> Vec<String> {
    ["async def ", "def ", "class "]
        .iter()
        .filter_map(|prefix| line.strip_prefix(prefix))
        .filter_map(|rest| identifier(rest))
        .collect()
}

fn extract_javascript_names(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    for prefix in [
        "export async function ",
        "export function ",
        "async function ",
        "function ",
        "export class ",
        "class ",
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            if let Some(name) = identifier(rest) {
                values.push(name);
            }
        }
    }
    for prefix in ["export const ", "const ", "export let ", "let "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            if (line.contains("=>") || line.contains("= function")) && identifier(rest).is_some() {
                values.push(identifier(rest).expect("checked"));
            }
        }
    }
    values
}

fn identifier(value: &str) -> Option<String> {
    let name: String = value
        .chars()
        .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '$')
        .collect();
    (!name.is_empty()).then_some(name)
}

pub fn implementation_matches(
    requirement: &Requirement,
    candidate: &ImplementationCandidate,
) -> bool {
    let requirement_tokens = tokens(&requirement.description);
    let symbol_tokens = tokens(&split_identifier(&candidate.symbol));
    if symbol_tokens.is_empty() {
        return false;
    }
    if symbol_tokens.len() == 1 {
        let token = symbol_tokens.iter().next().expect("one symbol token");
        return token.len() >= 4 && requirement_tokens.contains(token);
    }
    symbol_tokens
        .iter()
        .all(|token| requirement_tokens.contains(token))
}

pub fn analyze_assertions(code: &str) -> AssertionQuality {
    let mut meaningful = Vec::new();
    let mut placeholders = Vec::new();
    for (index, line) in code.lines().enumerate() {
        let line_number = index as i64 + 1;
        let compact = line.split_whitespace().collect::<String>().to_lowercase();
        if compact.is_empty() || compact.starts_with("//") || compact.starts_with('#') {
            continue;
        }
        let is_assertion = compact.contains("expect(")
            || compact.starts_with("assert")
            || compact.contains("pytest.raises")
            || compact.contains("assertequal(")
            || compact.contains("asserttrue(")
            || compact.contains("assertfalse(");
        if !is_assertion {
            continue;
        }
        if is_placeholder_assertion(&compact) {
            placeholders.push(line_number);
        } else {
            meaningful.push(line_number);
        }
    }
    if !meaningful.is_empty() {
        AssertionQuality::Meaningful(meaningful)
    } else if !placeholders.is_empty() {
        AssertionQuality::Placeholder(placeholders)
    } else {
        AssertionQuality::Missing
    }
}

pub fn has_requirement_trace(code: &str, requirement: &Requirement) -> bool {
    let id_marker = format!("requirement-id:{}", requirement.id.to_lowercase());
    let normalized = code.split_whitespace().collect::<String>().to_lowercase();
    if normalized.contains(&id_marker) {
        return true;
    }
    let description = requirement
        .description
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    code.lines().any(|line| {
        let trimmed = line.trim().trim_start_matches(['/', '#', '*', ' ']).trim();
        trimmed
            .strip_prefix("Requirement:")
            .map(str::trim)
            .is_some_and(|value| value == description)
    })
}

fn is_placeholder_assertion(compact: &str) -> bool {
    let tautologies = [
        "expect(true).tobe(true)",
        "expect(false).tobe(false)",
        "expect(1).tobe(1)",
        "asserttrue",
        "assert1==1",
        "assert0==0",
        "self.asserttrue(true)",
        "assertequal(true,true)",
        "assertequal(1,1)",
    ];
    if tautologies.iter().any(|value| compact.contains(value))
        || compact == "asserttrue"
        || compact.starts_with("asserttrue#")
    {
        return true;
    }

    if let Some(assertion) = compact.strip_prefix("assert") {
        let expression = assertion.split('#').next().unwrap_or(assertion);
        if matches!(expression, "true" | "false" | "notfalse" | "nottrue") {
            return true;
        }
        if !expression.chars().any(|ch| ch.is_alphabetic() || ch == '_') {
            return true;
        }
        for operator in ["==", "!=", ">=", "<="] {
            if let Some((left, right)) = expression.split_once(operator) {
                if left == right {
                    return true;
                }
            }
        }
    }

    if let Some(expect_start) = compact.find("expect(") {
        let inner_start = expect_start + "expect(".len();
        if let Some(close) = compact[inner_start..].find(')') {
            let actual = &compact[inner_start..inner_start + close];
            let remainder = &compact[inner_start + close + 1..];
            let literal = actual.parse::<f64>().is_ok()
                || matches!(actual, "true" | "false" | "null" | "undefined")
                || ((actual.starts_with('\'') && actual.ends_with('\''))
                    || (actual.starts_with('"') && actual.ends_with('"')));
            if literal {
                return true;
            }
            if let Some(argument_start) = remainder.find('(') {
                if let Some(argument_end) = remainder[argument_start + 1..].find(')') {
                    let expected =
                        &remainder[argument_start + 1..argument_start + 1 + argument_end];
                    if actual == expected {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn tokens(value: &str) -> BTreeSet<String> {
    const STOPWORDS: &[&str] = &[
        "the",
        "system",
        "shall",
        "must",
        "should",
        "application",
        "user",
        "with",
        "from",
        "into",
        "when",
        "then",
        "that",
        "this",
        "have",
        "will",
        "able",
        "allow",
        "using",
    ];
    value
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|token| token.len() >= 3 && !STOPWORDS.contains(token))
        .map(str::to_string)
        .collect()
}

fn split_identifier(value: &str) -> String {
    let mut result = String::new();
    let mut previous_lower = false;
    for ch in value.chars() {
        if ch == '_' || ch == '-' || ch == '$' {
            result.push(' ');
            previous_lower = false;
        } else {
            if ch.is_uppercase() && previous_lower {
                result.push(' ');
            }
            result.push(ch.to_ascii_lowercase());
            previous_lower = ch.is_lowercase() || ch.is_ascii_digit();
        }
    }
    result
}

pub fn evidence_id(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    digest
        .iter()
        .take(12)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_test_path(root: &Path, path: &Path) -> bool {
    let relative = relative(root, path).to_lowercase();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    relative.contains("/tests/")
        || relative.starts_with("tests/")
        || relative.contains("/__tests__/")
        || name.starts_with("test_")
        || name.contains(".test.")
        || name.contains(".spec.")
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirement(description: &str) -> Requirement {
        Requirement {
            id: "req_123".into(),
            spec_id: "spec".into(),
            section: "Requirements".into(),
            description: description.into(),
            req_type: "functional".into(),
            priority: "medium".into(),
            content_fingerprint: "fingerprint".into(),
            source_line_start: 4,
            source_line_end: 4,
        }
    }

    #[test]
    fn meaningful_and_placeholder_assertions_are_distinct() {
        assert!(matches!(
            analyze_assertions("expect(true).toBe(true);"),
            AssertionQuality::Placeholder(_)
        ));
        assert!(matches!(
            analyze_assertions("assert True"),
            AssertionQuality::Placeholder(_)
        ));
        assert!(matches!(
            analyze_assertions("expect(addNumbers(2, 3)).toBe(5);"),
            AssertionQuality::Meaningful(_)
        ));
        assert!(matches!(
            analyze_assertions("assert add_numbers(2, 3) == 5"),
            AssertionQuality::Meaningful(_)
        ));
        assert!(matches!(
            analyze_assertions("assert 2 + 2 == 4"),
            AssertionQuality::Placeholder(_)
        ));
        assert!(matches!(
            analyze_assertions("expect(result).toBe(result);"),
            AssertionQuality::Placeholder(_)
        ));
    }

    #[test]
    fn symbol_matching_requires_exact_identifier_tokens() {
        let req = requirement("The system shall expose add numbers behavior");
        let candidate = ImplementationCandidate {
            path: "src/math.ts".into(),
            line: 1,
            symbol: "addNumbers".into(),
            language: "typescript".into(),
        };
        assert!(implementation_matches(&req, &candidate));
        let unrelated = ImplementationCandidate {
            symbol: "authenticateUser".into(),
            ..candidate
        };
        assert!(!implementation_matches(&req, &unrelated));
    }

    #[test]
    fn trace_must_name_id_or_exact_requirement() {
        let req = requirement("The system shall add numbers");
        assert!(has_requirement_trace("// Requirement-ID: req_123", &req));
        assert!(has_requirement_trace(
            "# Requirement: The system shall add numbers",
            &req
        ));
        assert!(!has_requirement_trace("// Requirement: Adds stuff", &req));
    }

    #[test]
    fn checked_in_javascript_and_python_fixtures_scan_deterministically() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures");
        let javascript = scan_project(&fixtures.join("javascript").to_string_lossy(), &[])
            .expect("scan javascript fixture");
        assert_eq!(
            javascript.checked_languages,
            vec!["javascript", "typescript"]
        );
        assert!(javascript
            .implementation
            .iter()
            .any(|item| item.symbol == "addNumbers"));
        assert!(javascript
            .implementation
            .iter()
            .all(|item| !item.path.contains("tests/")));
        assert_eq!(javascript.tests.len(), 1);

        let python = scan_project(&fixtures.join("python").to_string_lossy(), &[])
            .expect("scan python fixture");
        assert_eq!(python.checked_languages, vec!["python"]);
        assert!(python
            .implementation
            .iter()
            .any(|item| item.symbol == "normalize_email"));

        let unsupported = scan_project(&fixtures.join("unsupported").to_string_lossy(), &[])
            .expect("scan unsupported fixture");
        assert!(unsupported.checked_languages.is_empty());
        assert_eq!(unsupported.skipped_languages, vec!["rs"]);
    }
}
