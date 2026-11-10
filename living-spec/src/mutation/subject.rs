//! Conservative resolution of directly called Rust functions to source extents.
use crate::agents::{
    syntax::{close, lex_rust, Kind},
    test_index::{Language, TestEntry},
};
use crate::core::mutation::{subject_digest, Target, TargetKind};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    ops::Range,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectFunction {
    pub file: String,
    pub name: String,
    /// Whole item including its attributes, measured in bytes of `source`.
    pub span: Range<usize>,
    /// Full file contents; `span` selects the verified function item.
    pub source: String,
    pub attributes: Vec<String>,
    pub cfg_test: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub package_dir: PathBuf,
    pub target: Target,
    pub functions: Vec<SubjectFunction>,
}

fn unsupported(reason: &str) -> Unsupported {
    Unsupported(reason.into())
}

fn relative_file(entry: &TestEntry) -> Result<PathBuf, Unsupported> {
    let path = Path::new(&entry.file);
    if path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(unsupported("unsupported test path"));
    }
    Ok(path.to_path_buf())
}

fn target_for(file: &Path) -> Result<Target, Unsupported> {
    let s = file.to_string_lossy().replace('\\', "/");
    if let Some(stem) = s.strip_prefix("tests/").and_then(|s| s.strip_suffix(".rs")) {
        if !stem.is_empty() && !stem.contains('/') {
            return Ok(Target {
                kind: TargetKind::Test,
                name: stem.into(),
            });
        }
    }
    if s == "src/lib.rs" {
        return Ok(Target {
            kind: TargetKind::Lib,
            name: "".into(),
        });
    }
    Err(unsupported("unsupported target"))
}

fn id_file(entry: &TestEntry) -> Result<PathBuf, Unsupported> {
    let mut parts = entry.id.splitn(4, ':');
    if parts.next() != Some("libtest") {
        return Err(unsupported("unsupported target"));
    }
    let target = parts
        .next()
        .ok_or_else(|| unsupported("unsupported target"))?;
    if target.is_empty() {
        return Err(unsupported("unsupported target"));
    }
    let file = parts
        .next()
        .ok_or_else(|| unsupported("unsupported target"))?;
    if parts.next().is_none() {
        return Err(unsupported("unsupported target"));
    }
    if matches!(target, "doctest" | "doc" | "bin") {
        return Err(unsupported("unsupported target"));
    }
    if let Some(stem) = file
        .strip_prefix("tests/")
        .and_then(|s| s.strip_suffix(".rs"))
    {
        if target != stem {
            return Err(unsupported("unsupported target"));
        }
    }
    let path = Path::new(file);
    if path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(unsupported("unsupported target"));
    }
    Ok(path.to_path_buf())
}

fn within(root: &Path, path: &Path) -> bool {
    path.starts_with(root)
}

fn package_dir(root: &Path, file: &Path) -> Result<PathBuf, Unsupported> {
    let root = root
        .canonicalize()
        .map_err(|_| unsupported("no owning package"))?;
    let file = root
        .join(file)
        .canonicalize()
        .map_err(|_| unsupported("test file not found"))?;
    if !within(&root, &file) {
        return Err(unsupported("test file outside tests root"));
    }
    let mut current = file.parent();
    while let Some(dir) = current {
        if !within(&root, dir) {
            break;
        }
        if dir.join("Cargo.toml").is_file() {
            return Ok(dir.to_path_buf());
        }
        if dir == root {
            break;
        }
        current = dir.parent();
    }
    Err(unsupported("no owning package"))
}

fn path_values(manifest: &str) -> Result<Vec<String>, Unsupported> {
    let mut paths = Vec::new();
    for raw in manifest.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        if compact.contains("workspace=") {
            return Err(unsupported("dependencies outside crate"));
        }
        if !line.contains("path") {
            continue;
        }
        let mut remaining = line;
        while let Some(index) = remaining.find("path") {
            let after_key = &remaining[index + 4..];
            let Some((_, value)) = after_key.split_once('=') else {
                // A path-like key whose meaning is unknown is not safe to ignore.
                return Err(unsupported("dependencies outside crate"));
            };
            let value = value.trim_start();
            let Some(quote) = value.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                return Err(unsupported("dependencies outside crate"));
            };
            let value = &value[quote.len_utf8()..];
            let Some(end) = value.find(quote) else {
                return Err(unsupported("dependencies outside crate"));
            };
            paths.push(value[..end].to_owned());
            remaining = &value[end + quote.len_utf8()..];
        }
    }
    Ok(paths)
}

fn outside_dependencies(dir: &Path) -> Result<(), Unsupported> {
    let root = dir
        .canonicalize()
        .map_err(|_| unsupported("dependencies outside crate"))?;
    let mut pending = vec![root.clone()];
    let mut seen = BTreeSet::new();
    while let Some(package) = pending.pop() {
        let package = package
            .canonicalize()
            .map_err(|_| unsupported("dependencies outside crate"))?;
        if !within(&root, &package) {
            return Err(unsupported("dependencies outside crate"));
        }
        if !seen.insert(package.clone()) {
            continue;
        }
        let manifest_path = package.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .map_err(|_| unsupported("dependencies outside crate"))?;
        for dependency in path_values(&manifest)? {
            let candidate = package
                .join(dependency)
                .canonicalize()
                .map_err(|_| unsupported("dependencies outside crate"))?;
            if !within(&root, &candidate) {
                return Err(unsupported("dependencies outside crate"));
            }
            pending.push(candidate);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Module,
    Other,
}

fn cfg_test_fn(tokens: &[crate::agents::syntax::Token], index: usize) -> bool {
    let mut cursor = index;
    loop {
        if cursor > 0
            && matches!(
                tokens[cursor - 1].text.as_str(),
                "pub" | "unsafe" | "async" | "const" | "extern"
            )
        {
            cursor -= 1;
            continue;
        }
        if cursor >= 2 && tokens[cursor - 1].text == ")" {
            if let Some(open) = (0..cursor - 1).rev().find(|j| tokens[*j].text == "(") {
                if open > 0 && tokens[open - 1].text == "pub" {
                    cursor = open - 1;
                    continue;
                }
            }
        }
        break;
    }
    while cursor >= 2 && tokens[cursor - 1].text == "]" {
        let Some(open) = (0..cursor - 1)
            .rev()
            .find(|j| tokens[*j].text == "#" && tokens.get(*j + 1).is_some_and(|t| t.text == "["))
        else {
            break;
        };
        let Some(end) = close(tokens, open + 1) else {
            break;
        };
        if end != cursor - 1 {
            break;
        }
        let attr = &tokens[open + 2..end];
        if (attr.len() == 1 && attr[0].text == "test")
            || (attr.len() == 4
                && attr[0].text == "cfg"
                && attr[1].text == "("
                && attr[2].text == "test"
                && attr[3].text == ")")
        {
            return true;
        }
        cursor = open;
    }
    false
}

/// Find free functions with a syntactically closed item extent. The lexer does
/// not parse Rust; any declaration shape we cannot delimit is omitted.
fn definitions(file: &str, source: &str) -> (Vec<SubjectFunction>, BTreeSet<String>) {
    let ts = lex_rust(source);
    let mut result = Vec::new();
    let mut definition_counts = BTreeMap::<String, usize>::new();
    let mut supported_counts = BTreeMap::<String, usize>::new();
    let mut scopes = vec![Scope::Module];
    let mut brace_stack: Vec<Scope> = Vec::new();
    let mut last_boundary = 0usize;
    for i in 0..ts.len() {
        if ts[i].text == "{" {
            let header = &ts[last_boundary..i];
            let has_mod =
                header.iter().any(|t| t.text == "mod") && !header.iter().any(|t| t.text == "fn");
            let has_cfg = header
                .iter()
                .any(|t| t.text == "cfg" || t.text == "cfg_attr");
            // Only an unannotated module is traversed. cfg(test) modules and
            // cfg-dependent modules are outside the production subject set.
            let mode = if has_mod && !has_cfg {
                Scope::Module
            } else {
                Scope::Other
            };
            brace_stack.push(mode);
            scopes.push(mode);
            last_boundary = i + 1;
        } else if ts[i].text == "}" {
            if !brace_stack.is_empty() {
                brace_stack.pop();
                scopes.pop();
            }
            last_boundary = i + 1;
        } else if ts[i].text == ";" {
            last_boundary = i + 1;
        }
        if ts[i].text != "fn" || scopes.iter().any(|s| *s != Scope::Module) {
            continue;
        }
        let Some(name_token) = ts.get(i + 1).filter(|t| t.kind == Kind::Ident) else {
            continue;
        };
        if cfg_test_fn(&ts, i) {
            continue;
        }
        *definition_counts
            .entry(name_token.text.clone())
            .or_default() += 1;
        // Generic parameter and argument list must be closed; body must be a block.
        let paren = i + 2;
        if ts.get(paren).is_none_or(|t| t.text != "(") {
            continue;
        }
        let Some(paren_end) = close(&ts, paren) else {
            continue;
        };
        let Some(open) =
            (paren_end + 1..ts.len()).find(|j| ts[*j].text == "{" || ts[*j].text == ";")
        else {
            continue;
        };
        if ts[open].text != "{" {
            continue;
        }
        let Some(end) = close(&ts, open) else {
            continue;
        };
        let Some(body_end) = ts.get(end) else {
            continue;
        };
        let mut item_index = i;
        // Include conventional Rust qualifiers and visibility in the item
        // extent, including `pub(crate) unsafe fn` forms.
        loop {
            if item_index > 0
                && matches!(
                    ts[item_index - 1].text.as_str(),
                    "pub" | "unsafe" | "async" | "const" | "extern"
                )
            {
                item_index -= 1;
                if ts[item_index].text == "pub"
                    && ts.get(item_index + 1).is_some_and(|t| t.text == "(")
                {
                    if let Some(end) = close(&ts, item_index + 1) {
                        if end + 1 == item_index + 2 {
                        } else {
                            break;
                        }
                    }
                }
            } else {
                break;
            }
        }
        if item_index >= 2 && ts[item_index - 1].text == ")" {
            if let Some(start) = (0..item_index - 1).rev().find(|j| ts[*j].text == "(") {
                if start > 0 && ts[start - 1].text == "pub" {
                    item_index = start - 1;
                }
            }
        }
        let mut item_start = ts[item_index].start;
        let mut attributes = Vec::new();
        let mut cfg_test = false;
        let mut attr_start = item_index;
        while attr_start >= 2 && ts[attr_start - 1].text == "]" {
            let Some(open_attr) = (0..attr_start - 1)
                .rev()
                .find(|j| ts[*j].text == "#" && ts.get(*j + 1).is_some_and(|t| t.text == "["))
            else {
                break;
            };
            let Some(attr_end) = close(&ts, open_attr + 1) else {
                break;
            };
            if attr_end != attr_start - 1 {
                break;
            }
            let attr = &source[ts[open_attr].start..ts[attr_end].end];
            let inside = &ts[open_attr + 2..attr_end];
            if inside.len() == 1 && inside[0].text == "test" {
                cfg_test = true;
            } else if inside
                .iter()
                .any(|t| t.text == "cfg" || t.text == "cfg_attr")
            {
                if inside.len() == 4
                    && inside[0].text == "cfg"
                    && inside[1].text == "("
                    && inside[2].text == "test"
                    && inside[3].text == ")"
                {
                    cfg_test = true;
                } else {
                    attributes.push(attr.into());
                    item_start = usize::MAX;
                    break;
                }
            }
            attributes.push(attr.into());
            item_start = ts[open_attr].start;
            attr_start = open_attr;
        }
        if item_start == usize::MAX || cfg_test {
            continue;
        }
        // Don't accept declarations whose header contains uncertain attributes/macros.
        let header_start = ts
            .iter()
            .rposition(|t| t.end <= item_start && (t.text == ";" || t.text == "}"))
            .map_or(0, |j| j + 1);
        if ts[header_start..item_index]
            .iter()
            .any(|t| t.text == "!" || t.kind == Kind::Unsupported)
        {
            continue;
        }
        result.push(SubjectFunction {
            file: file.into(),
            name: name_token.text.clone(),
            span: item_start..body_end.end,
            source: source.into(),
            attributes,
            cfg_test,
        });
        *supported_counts.entry(name_token.text.clone()).or_default() += 1;
    }
    let uncertain = definition_counts
        .into_iter()
        .filter_map(|(name, count)| {
            (supported_counts.get(&name).copied().unwrap_or(0) < count).then_some(name)
        })
        .collect();
    (result, uncertain)
}

fn called_names(body: &str) -> BTreeSet<String> {
    let ts = lex_rust(body);
    let mut names = BTreeSet::new();
    let mut shadowed = BTreeSet::new();
    for pair in ts.windows(2) {
        if matches!(pair[0].text.as_str(), "let" | "fn") && pair[1].kind == Kind::Ident {
            shadowed.insert(pair[1].text.clone());
        }
    }
    let mut i = 0;
    while i + 1 < ts.len() {
        if ts[i].text == "!"
            && ts
                .get(i + 1)
                .is_some_and(|t| matches!(t.text.as_str(), "(" | "[" | "{"))
        {
            let assertion = i > 0
                && matches!(
                    ts[i - 1].text.as_str(),
                    "assert"
                        | "assert_eq"
                        | "assert_ne"
                        | "debug_assert"
                        | "debug_assert_eq"
                        | "debug_assert_ne"
                );
            if !assertion {
                let Some(end) = close(&ts, i + 1) else { break };
                i = end + 1;
                continue;
            }
        }
        if ts[i].kind != Kind::Ident || ts[i + 1].text != "(" {
            i += 1;
            continue;
        }
        if i > 0 && matches!(ts[i - 1].text.as_str(), "." | "!") {
            i += 1;
            continue;
        }
        if matches!(
            ts[i].text.as_str(),
            "if" | "while"
                | "for"
                | "match"
                | "loop"
                | "return"
                | "assert"
                | "assert_eq"
                | "assert_ne"
                | "debug_assert"
                | "debug_assert_eq"
                | "debug_assert_ne"
        ) {
            i += 1;
            continue;
        }
        if !shadowed.contains(&ts[i].text) {
            names.insert(ts[i].text.clone());
        }
        i += 1;
    }
    names
}

fn source_files(root: &Path) -> Vec<PathBuf> {
    fn visit(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                visit(&path, out);
            } else if kind.is_file() && path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    visit(&root.join("src"), &mut out);
    out.sort();
    out
}

pub fn resolve_subject(entry: &TestEntry, tests_root: &Path) -> Result<Subject, Unsupported> {
    if entry.language != Language::Rust {
        return Err(unsupported("only Rust tests are supported in v1"));
    }
    let rel = relative_file(entry)?;
    let target_path = id_file(entry)?;
    let mut target = target_for(&target_path)?;
    if target.kind == TargetKind::Lib {
        target.name = entry.id.split(':').nth(1).unwrap_or_default().to_owned();
    }
    let package_dir = package_dir(tests_root, &rel)?;
    outside_dependencies(&package_dir)?;
    let wanted = called_names(&entry.body);
    let mut defs: BTreeMap<String, Vec<SubjectFunction>> = BTreeMap::new();
    let mut uncertain_names = BTreeSet::new();
    for path in source_files(&package_dir) {
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        // Conditional external modules cannot be associated with their parent
        // safely by this lexer. Do not infer their status from their file name.
        let tokens = lex_rust(&source);
        // Conditional modules can hide competing definitions. Without evaluating
        // cfg expressions, reject a file with any uncertain cfg attribute.
        for i in 0..tokens.len().saturating_sub(2) {
            if tokens[i].text == "#"
                && tokens[i + 1].text == "["
                && matches!(tokens[i + 2].text.as_str(), "cfg" | "cfg_attr")
            {
                let exact_test = tokens.get(i + 3..i + 7).is_some_and(|t| {
                    t.iter()
                        .map(|t| t.text.as_str())
                        .eq(["(", "test", ")", "]"])
                });
                if tokens[i + 2].text != "cfg" || !exact_test {
                    return Err(unsupported("no resolvable subject"));
                }
            }
        }
        if tokens
            .windows(3)
            .any(|w| w[0].text == "mod" && w[1].kind == Kind::Ident && w[2].text == ";")
        {
            return Err(unsupported("no resolvable subject"));
        }
        let relative = path
            .strip_prefix(&package_dir)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let (functions, uncertain) = definitions(&relative, &source);
        uncertain_names.extend(uncertain);
        for function in functions {
            defs.entry(function.name.clone())
                .or_default()
                .push(function);
        }
    }
    let mut functions = Vec::new();
    for name in wanted {
        if uncertain_names.contains(&name) {
            continue;
        }
        if let Some(found) = defs.get(&name).filter(|v| v.len() == 1) {
            functions.push(found[0].clone());
        }
    }
    functions.sort_by(|a, b| (&a.file, &a.name).cmp(&(&b.file, &b.name)));
    if functions.is_empty() {
        return Err(unsupported("no resolvable subject"));
    }
    if functions.len() > 6 {
        return Err(unsupported("subject too large"));
    }
    Ok(Subject {
        package_dir,
        target,
        functions,
    })
}

pub fn subject_digest_for(
    entry: &TestEntry,
    tests_root: &Path,
) -> Result<Option<String>, Unsupported> {
    let subject = resolve_subject(entry, tests_root)?;
    let parts = subject
        .functions
        .iter()
        .map(|f| {
            (
                &f.file[..],
                &f.name[..],
                f.source[f.span.clone()].as_bytes(),
            )
        })
        .collect::<Vec<_>>();
    Ok(Some(subject_digest(&parts)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::test_index::TestEntry;
    fn entry(file: &str, body: &str) -> TestEntry {
        TestEntry {
            id: format!(
                "libtest:{}:{file}:case",
                Path::new(file).file_stem().unwrap().to_string_lossy()
            ),
            name: "case".into(),
            file: file.into(),
            line: 1,
            source: String::new(),
            body: body.into(),
            language: Language::Rust,
            should_panic: false,
            attributes: vec![],
            unsupported_attributes: false,
            decorated: false,
            case_ids: vec![],
            unknown_cases: false,
            fingerprint: String::new(),
            rename_hint: String::new(),
        }
    }
    #[test]
    fn target_derivation_lib_and_test() {
        assert_eq!(
            target_for(Path::new("src/lib.rs")).unwrap().kind,
            TargetKind::Lib
        );
        assert_eq!(
            target_for(Path::new("tests/check.rs")).unwrap().name,
            "check"
        );
        assert!(target_for(Path::new("src/main.rs")).is_err());
    }
    #[test]
    fn method_call_not_a_subject() {
        assert!(called_names("thing.check();").is_empty());
        assert!(called_names("stringify!(check()); let check = callback; check();").is_empty());
        assert!(called_names("assert_eq!(check(), 1);").contains("check"));
    }
    #[test]
    fn ambiguous_name_not_a_subject() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        fs::write(root.join("tests/a.rs"), "").unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "pub fn same() {}\nmod inner { pub fn same() {} }\n",
        )
        .unwrap();
        assert_eq!(
            resolve_subject(&entry("tests/a.rs", "same();"), root)
                .unwrap_err()
                .0,
            "no resolvable subject"
        );
    }

    #[test]
    fn cfg_test_code_not_mutated() {
        let source =
            "#[cfg(test)] mod tests { fn hidden() -> i32 { 4 } }\nfn visible() -> i32 { 2 }";
        let (found, uncertain) = definitions("src/lib.rs", source);
        assert!(uncertain.is_empty());
        assert_eq!(
            found.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            vec!["visible"]
        );
    }

    #[test]
    fn sample_subject_and_candidates() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mutation/sample-crate");
        let subject = resolve_subject(
            &entry("tests/strong.rs", "clamp(1); choose(true, 4);"),
            &root,
        )
        .unwrap();
        assert_eq!(subject.functions.len(), 2);
        let candidates = crate::mutation::generate::candidates_for(&subject, 4);
        assert_eq!(candidates.len(), 4);
        assert_eq!(
            candidates.iter().map(|c| &c.mutant.id).collect::<Vec<_>>(),
            crate::mutation::generate::candidates_for(&subject, 4)
                .iter()
                .map(|c| &c.mutant.id)
                .collect::<Vec<_>>()
        );
        let unit = resolve_subject(&entry("src/lib.rs", "clamp(1);"), &root).unwrap();
        assert_eq!(unit.target.name, "lib");
    }

    #[test]
    fn subject_digest_changes_when_subject_edited() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(root.join("tests/a.rs"), "").unwrap();
        let file = root.join("src/lib.rs");
        fs::write(&file, "fn value() -> i32 { 1 }").unwrap();
        let test = entry("tests/a.rs", "value();");
        let before = subject_digest_for(&test, root).unwrap();
        fs::write(&file, "fn value() -> i32 { 2 }").unwrap();
        assert_ne!(before, subject_digest_for(&test, root).unwrap());
    }

    #[test]
    fn path_dependency_outside_crate_unsupported() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("sample");
        let outside = temp.path().join("outside");
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(outside.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='sample'\nversion='0.1.0'\n[dependencies]\nextra={path='../outside'}\n",
        )
        .unwrap();
        fs::write(
            outside.join("Cargo.toml"),
            "[package]\nname='extra'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(root.join("tests/strong.rs"), "").unwrap();
        fs::write(root.join("src/lib.rs"), "fn value() -> i32 { 1 }").unwrap();
        assert_eq!(
            resolve_subject(&entry("tests/strong.rs", "value();"), &root)
                .unwrap_err()
                .0,
            "dependencies outside crate"
        );
    }

    #[test]
    fn transitive_path_dependency_outside_crate_unsupported() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("sample");
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("deps/inner/src")).unwrap();
        fs::create_dir_all(temp.path().join("outside/src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='sample'\nversion='0.1.0'\n[dependencies]\ninner={path='deps/inner'}\n",
        )
        .unwrap();
        fs::write(
            root.join("deps/inner/Cargo.toml"),
            "[package]\nname='inner'\nversion='0.1.0'\n[dependencies]\nouter={path='../../../outside'}\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("outside/Cargo.toml"),
            "[package]\nname='outer'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(root.join("tests/strong.rs"), "").unwrap();
        fs::write(root.join("src/lib.rs"), "fn value() -> i32 { 1 }").unwrap();
        assert_eq!(
            resolve_subject(&entry("tests/strong.rs", "value();"), &root)
                .unwrap_err()
                .0,
            "dependencies outside crate"
        );
    }

    #[test]
    fn unsupported_competing_definition_blocks_resolution() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(root.join("tests/a.rs"), "").unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "fn value() -> i32 { 1 }\n#[cfg(unix)] fn value() -> i32 { 2 }",
        )
        .unwrap();
        assert_eq!(
            resolve_subject(&entry("tests/a.rs", "value();"), root)
                .unwrap_err()
                .0,
            "no resolvable subject"
        );
        fs::write(
            root.join("src/lib.rs"),
            "fn value() -> i32 { 1 }\n#[cfg(unix)] mod conditional { fn value() -> i32 { 2 } }",
        )
        .unwrap();
        assert_eq!(
            resolve_subject(&entry("tests/a.rs", "value();"), root)
                .unwrap_err()
                .0,
            "no resolvable subject"
        );
    }

    #[test]
    fn subject_too_large_unsupported() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(root.join("tests/a.rs"), "").unwrap();
        let names = (0..7).map(|i| format!("f{i}();")).collect::<String>();
        let defs = (0..7)
            .map(|i| format!("fn f{i}() {{}}\n"))
            .collect::<String>();
        fs::write(root.join("src/lib.rs"), defs).unwrap();
        assert_eq!(
            resolve_subject(&entry("tests/a.rs", &names), root)
                .unwrap_err()
                .0,
            "subject too large"
        );
    }

    #[test]
    fn unresolvable_subject_unsupported() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(root.join("tests/a.rs"), "").unwrap();
        fs::write(root.join("src/lib.rs"), "fn other() {}\n").unwrap();
        assert_eq!(
            resolve_subject(&entry("tests/a.rs", "missing();"), root)
                .unwrap_err()
                .0,
            "no resolvable subject"
        );
    }

    #[test]
    fn bin_target_and_doctest_unsupported() {
        assert_eq!(
            resolve_subject(&entry("src/main.rs", "main();"), Path::new("."))
                .unwrap_err()
                .0,
            "unsupported target"
        );
        let mut doc = entry("src/lib.rs", "example();");
        doc.id = "libtest:doctest:src/lib.rs:example".into();
        assert_eq!(
            resolve_subject(&doc, Path::new(".")).unwrap_err().0,
            "unsupported target"
        );
    }
}
