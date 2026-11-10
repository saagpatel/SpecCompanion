use super::syntax::{close, lex, lex_javascript, lex_rust, split_args, Kind, Token};
use crate::core::annotations::Link;
use crate::core::test_identity::fingerprint;
use regex::Regex;
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestEntry {
    pub id: String,
    pub name: String,
    pub file: String,
    pub line: usize,
    pub source: String,
    pub body: String,
    pub language: Language,
    pub should_panic: bool,
    pub attributes: Vec<String>,
    pub unsupported_attributes: bool,
    pub decorated: bool,
    pub case_ids: Vec<String>,
    pub unknown_cases: bool,
    pub fingerprint: String,
    pub rename_hint: String,
}
#[derive(Debug, Clone, Default)]
pub struct TestIndex {
    pub entries: Vec<TestEntry>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum SourceResolution<'a> {
    Exact(&'a TestEntry),
    Changed(&'a TestEntry),
    Renamed(&'a TestEntry),
    Unresolved,
}
impl TestIndex {
    /// R3 source resolution; ambiguity is never guessed and an edited exact id is never Exact.
    pub fn resolve(&self, link: &Link) -> SourceResolution<'_> {
        use crate::core::test_identity::{resolve_link_with_identities, Resolution};
        match resolve_link_with_identities(link, &[], &self.identities()) {
            Resolution::Exact => SourceResolution::Exact(self.get(&link.test_id).unwrap()),
            Resolution::Changed => SourceResolution::Changed(self.get(&link.test_id).unwrap()),
            Resolution::Renamed { new_id } => SourceResolution::Renamed(self.get(&new_id).unwrap()),
            Resolution::Unresolved => SourceResolution::Unresolved,
        }
    }
    pub fn identities(&self) -> crate::core::test_identity::Fingerprints {
        let mut identities = crate::core::test_identity::Fingerprints::new();
        for entry in &self.entries {
            let identity = crate::core::test_identity::TestIdentity {
                fingerprint: entry.fingerprint.clone(),
                rename_hint: Some(entry.rename_hint.clone()),
            };
            identities.insert(entry.id.clone(), identity.clone());
            for case in &entry.case_ids {
                identities.insert(format!("{}#{case}", entry.id), identity.clone());
            }
        }
        identities
    }
    /// Map explicit scoped metadata or pytest's file/class identity, never a bare name guess.
    pub fn junit_id(&self, class: &str, name: &str) -> String {
        let fallback = if class.is_empty() {
            name.to_owned()
        } else {
            format!("{class}::{name}")
        };
        let mut matches = self.entries.iter().filter_map(|entry| {
            if entry.id == fallback
                || entry.id == format!("{class}:{name}")
                || (entry.language == Language::Python
                    && format!("{class}.{name}")
                        == format!(
                            "{}.{}",
                            entry.file.trim_end_matches(".py").replace('/', "."),
                            entry.name.replace("::", ".")
                        ))
            {
                return Some(entry.id.clone());
            }
            if entry.language == Language::Python {
                let (base, case) = name.strip_suffix(']')?.split_once('[')?;
                if !case.is_empty()
                    && !base.contains('[')
                    && format!("{class}.{base}")
                        == format!(
                            "{}.{}",
                            entry.file.trim_end_matches(".py").replace('/', "."),
                            entry.name.replace("::", ".")
                        )
                {
                    return Some(format!("{}#{case}", entry.id));
                }
            }
            if entry.language == Language::JavaScript
                && entry
                    .id
                    .rsplit_once(':')
                    .is_some_and(|(scope, _)| scope == class)
                && [
                    "test.each",
                    "it.each",
                    "test.only.each",
                    "it.only.each",
                    "test.skip.each",
                    "it.skip.each",
                ]
                .iter()
                .any(|prefix| entry.source.starts_with(prefix))
            {
                if let Some(case) = each_case(&entry.name, name) {
                    return Some(format!("{}#{case}", entry.id));
                }
            }
            None
        });
        match (matches.next(), matches.next()) {
            (Some(id), None) => id,
            _ => fallback,
        }
    }
    /// Resolve an exact id before considering a parameterized case suffix.
    pub fn resolve_test_id(&self, id: &str) -> Option<&TestEntry> {
        self.get(id).or_else(|| {
            let (base, _) = id.rsplit_once('#')?;
            self.get(base)
                .filter(|entry| !entry.case_ids.is_empty() || entry.unknown_cases)
        })
    }
    pub fn get(&self, id: &str) -> Option<&TestEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }
    /// Scan text only: no imports, compilation, execution, or symlink traversal.
    pub fn build(root: &Path) -> io::Result<Self> {
        // A crate's tests directory is a scan boundary, not an identity root.
        // Cargo reports integration paths relative to the crate manifest.
        let identity_root = identity_root(root);
        let mut paths = Vec::new();
        collect(root, &mut paths)?;
        paths.sort();
        let mut entries = Vec::new();
        for path in paths {
            let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            if !matches!(
                extension,
                "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs"
            ) {
                continue;
            }
            let source = fs::read_to_string(&path)?;
            let file = path
                .strip_prefix(identity_root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            match extension {
                "rs" => {
                    let crate_root = path
                        .ancestors()
                        .skip(1)
                        .take_while(|dir| dir.starts_with(identity_root))
                        .find(|dir| dir.join("Cargo.toml").is_file());
                    if let Some(crate_root) = crate_root {
                        let relative = path
                            .strip_prefix(crate_root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/");
                        if let Some(target) = cargo_unit_target(crate_root, &relative)? {
                            let module_path = target_module_path(crate_root, &target, &relative)
                                .unwrap_or_default();
                            rust_tests(
                                &source,
                                &file,
                                &relative,
                                Some(&target),
                                Some(&module_path),
                                &mut entries,
                            );
                        }
                    } else {
                        rust_tests(&source, &file, &file, None, None, &mut entries);
                    }
                }
                "py" => python_tests(&source, &file, &mut entries),
                _ => js_tests(&source, &file, &mut entries),
            }
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        if entries.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "duplicate scoped test id",
            ));
        }
        Ok(Self { entries })
    }
}

/// Root used for stable identities when scanning a crate's tests directory.
pub fn identity_root(root: &Path) -> &Path {
    if root.file_name().is_some_and(|name| name == "tests")
        && root
            .parent()
            .is_some_and(|parent| parent.join("Cargo.toml").is_file())
    {
        root.parent().unwrap()
    } else {
        root
    }
}
// Accept only scalar labels whose framework rendering is unambiguous.
fn scalar_label(tokens: &[Token]) -> Option<String> {
    match tokens {
        [t] if t.kind == Kind::Number && t.text.chars().all(|c| c.is_ascii_digit()) => {
            (t.text.parse::<u64>().ok()?.to_string() == t.text).then(|| t.text.clone())
        }
        [t] if t.kind == Kind::String
            && (t.text.starts_with('\'') || t.text.starts_with('"'))
            && !t.text.contains('\\')
            && t.text.is_ascii()
            && !t.text.chars().any(char::is_control)
            && t.text.len() >= 2 =>
        {
            Some(t.text[1..t.text.len() - 1].to_owned())
        }
        _ => None,
    }
}
fn literal_cases(tokens: &[Token], unwrap_rows: bool) -> Option<Vec<String>> {
    if tokens.first()?.text != "[" || close(tokens, 0)? != tokens.len() - 1 {
        return None;
    }
    let mut cases = Vec::new();
    let rows = split_args(&tokens[1..tokens.len() - 1]);
    for mut row in rows.iter().copied() {
        if row.is_empty() {
            return None;
        }
        if matches!(row[0].text.as_str(), "[" | "(") {
            if !unwrap_rows {
                return None;
            }
            if close(row, 0)? != row.len() - 1 {
                return None;
            }
            row = &row[1..row.len() - 1];
        }
        let label = scalar_label(row)?;
        if label.is_empty() || cases.contains(&label) {
            return None;
        }
        cases.push(label);
    }
    (!cases.is_empty()).then_some(cases)
}
fn js_cases(tokens: &[Token], template: &str) -> Option<Vec<String>> {
    let cases = literal_cases(tokens, true)?;
    let placeholders: Vec<_> = template.match_indices('%').collect();
    let (pos, _) = *placeholders.first()?;
    if placeholders.len() != 1 {
        return None;
    }
    let kind = *template.as_bytes().get(pos + 1)?;
    if !matches!(kind, b's' | b'i' | b'd' | b'f') {
        return None;
    }
    if kind != b's'
        && cases
            .iter()
            .any(|c| c.parse::<u64>().ok().is_none_or(|n| n.to_string() != *c))
    {
        return None;
    }
    Some(cases)
}
fn python_cases(tokens: &[Token]) -> Option<Vec<String>> {
    let prefix = ["@", "pytest", ".", "mark", ".", "parametrize", "("];
    if tokens.len() < prefix.len() || !tokens.iter().zip(prefix).all(|(t, s)| t.text == s) {
        return None;
    }
    let end = close(tokens, 6)?;
    if end + 1 != tokens.len() {
        return None;
    }
    let args = split_args(&tokens[7..end]);
    if args.len() != 2 || args[0].len() != 1 || args[0][0].kind != Kind::String {
        return None;
    }
    if scalar_label(args[0])?.contains(',') {
        return None;
    }
    literal_cases(args[1], false)
}
// Only a single, bounded printf-style placeholder has an unambiguous case label.
fn each_case<'a>(template: &str, title: &'a str) -> Option<&'a str> {
    let placeholders: Vec<_> = template.match_indices('%').collect();
    let (position, _) = *placeholders.first()?;
    if placeholders.len() != 1 {
        return None;
    }
    let kind = template.as_bytes().get(position + 1).copied()?;
    if !matches!(kind, b's' | b'i' | b'd' | b'f') {
        return None;
    }
    let case = title
        .strip_prefix(&template[..position])?
        .strip_suffix(&template[position + 2..])?;
    if case.is_empty() || (kind != b's' && case.parse::<f64>().is_err()) {
        return None;
    }
    Some(case)
}
fn target(file: &str) -> &str {
    file.split_once('/').map_or("root", |(first, _)| first)
}
struct Context<'a> {
    source: &'a str,
    file: &'a str,
    identity_file: &'a str,
    framework: &'a str,
    target: &'a str,
    language: Language,
}
impl Context<'_> {
    fn entry(
        &self,
        name: String,
        start: usize,
        end: usize,
        body_start: usize,
        body_end: usize,
        should_panic: bool,
    ) -> TestEntry {
        let source = self.source[start..end].to_owned();
        let body = self.source[body_start..body_end].to_owned();
        // Unsupported declarations retain their full source, preventing guessed renames.
        let hint = crate::core::test_identity::rename_hint(&source)
            .unwrap_or_else(|| fingerprint(&source));
        TestEntry {
            id: format!(
                "{}:{}:{}:{name}",
                self.framework, self.target, self.identity_file
            ),
            name,
            file: self.file.to_owned(),
            line: self.source[..start].bytes().filter(|c| *c == b'\n').count() + 1,
            fingerprint: fingerprint(&source),
            rename_hint: hint,
            source,
            body,
            language: self.language,
            should_panic,
            attributes: Vec::new(),
            unsupported_attributes: false,
            decorated: false,
            case_ids: Vec::new(),
            unknown_cases: false,
        }
    }
}
#[derive(Debug)]
struct CargoUnitTarget {
    name: String,
    path: String,
    kind: &'static str,
}

// Read Cargo target declarations without compiling or executing the crate.
// Unsupported/ambiguous layouts deliberately produce no test identity.
fn cargo_unit_target(root: &Path, file: &str) -> io::Result<Option<CargoUnitTarget>> {
    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    let mut section = "";
    let mut package = None;
    let mut autobins = true;
    let mut declarations = Vec::new();
    let mut current: Option<(&'static str, String, Option<String>)> = None;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if let Some(declaration) = current.take() {
                declarations.push(declaration);
            }
            section = line;
            current = match section {
                "[lib]" => Some(("lib", String::new(), None)),
                "[[bin]]" => Some(("bin", String::new(), None)),
                "[[test]]" => Some(("test", String::new(), None)),
                _ => None,
            };
        } else if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .to_owned();
            if section == "[package]" {
                if key == "name" {
                    package = Some(value);
                } else if key == "autobins" {
                    autobins = value != "false";
                }
            } else if let Some((_, name, path)) = current.as_mut() {
                if key == "name" {
                    *name = value;
                } else if key == "path" {
                    *path = Some(value);
                }
            }
        }
    }
    if let Some(declaration) = current.take() {
        declarations.push(declaration);
    }
    let mut targets: Vec<_> = declarations
        .into_iter()
        .filter_map(|(kind, name, path)| finish_target(root, kind, name, path, package.as_deref()))
        .collect();

    if let Some(name) = package.as_deref() {
        if !targets.iter().any(|target| target.kind == "lib") && root.join("src/lib.rs").is_file() {
            targets.push(CargoUnitTarget {
                name: name.replace('-', "_"),
                path: "src/lib.rs".into(),
                kind: "lib",
            });
        }
        if autobins
            && root.join("src/main.rs").is_file()
            && !targets
                .iter()
                .any(|target| target.kind == "bin" && target.path == "src/main.rs")
        {
            targets.push(CargoUnitTarget {
                name: name.replace('-', "_"),
                path: "src/main.rs".into(),
                kind: "bin",
            });
        }
    }
    if autobins {
        let bin_dir = root.join("src/bin");
        if bin_dir.is_dir() {
            for item in fs::read_dir(bin_dir)? {
                let item = item?;
                let filename = item.file_name().to_string_lossy().into_owned();
                let (name, path) = if item.file_type()?.is_file() && filename.ends_with(".rs") {
                    (
                        filename.trim_end_matches(".rs").to_owned(),
                        format!("src/bin/{filename}"),
                    )
                } else if item.file_type()?.is_dir() && item.path().join("main.rs").is_file() {
                    (filename.clone(), format!("src/bin/{filename}/main.rs"))
                } else {
                    continue;
                };
                if !targets
                    .iter()
                    .any(|target| target.kind == "bin" && target.path == path)
                {
                    targets.push(CargoUnitTarget {
                        name: name.replace('-', "_"),
                        path,
                        kind: "bin",
                    });
                }
            }
        }
    }
    // Cargo's default integration targets are the direct tests/*.rs files.
    if let Some(name) = file
        .strip_prefix("tests/")
        .and_then(|path| path.strip_suffix(".rs"))
    {
        if !name.contains('/')
            && !targets
                .iter()
                .any(|target| target.kind == "test" && target.path == file)
        {
            targets.push(CargoUnitTarget {
                name: name.into(),
                path: file.into(),
                kind: "test",
            });
        }
    }
    let mut matches = targets
        .into_iter()
        .filter(|target| file == target.path || target_module_path(root, target, file).is_some());
    let found = matches.next();
    let second = matches.next();
    if second.is_some() {
        return Ok(None);
    }
    Ok(found)
}

fn finish_target(
    root: &Path,
    kind: &'static str,
    name: String,
    path: Option<String>,
    package: Option<&str>,
) -> Option<CargoUnitTarget> {
    let raw_name = if name.is_empty() {
        match kind {
            "lib" => package?.to_owned(),
            "bin" | "test" => path
                .as_deref()?
                .rsplit('/')
                .next()?
                .trim_end_matches(".rs")
                .to_owned(),
            _ => return None,
        }
    } else {
        name
    };
    let path = path.unwrap_or_else(|| match kind {
        "lib" => "src/lib.rs".into(),
        "bin" => {
            let flat = format!("src/bin/{raw_name}.rs");
            let directory = format!("src/bin/{raw_name}/main.rs");
            if root.join(&directory).is_file() {
                directory
            } else {
                flat
            }
        }
        "test" => format!("tests/{raw_name}.rs"),
        _ => String::new(),
    });
    let contained = Path::new(&path).is_relative()
        && Path::new(&path)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
    (contained && !path.is_empty()).then_some(CargoUnitTarget {
        name: raw_name.replace('-', "_"),
        path,
        kind,
    })
}

fn target_module_path(root: &Path, target: &CargoUnitTarget, file: &str) -> Option<Vec<String>> {
    if file == target.path {
        return Some(Vec::new());
    }
    let source = fs::read_to_string(root.join(&target.path)).ok()?;
    let tokens = lex_rust(&source);
    let mut modules = BTreeMap::new();
    let base = target
        .path
        .rsplit_once('/')
        .map_or("", |(parent, _)| parent);
    collect_module_paths(root, &tokens, 0, tokens.len(), base, &[], &mut modules)?;
    modules.remove(file)
}

fn collect_module_paths(
    root: &Path,
    tokens: &[Token],
    start: usize,
    end: usize,
    base: &str,
    prefix: &[String],
    found: &mut BTreeMap<String, Vec<String>>,
) -> Option<()> {
    let mut i = start;
    while i < end {
        if tokens[i].text == "#" && tokens.get(i + 1).is_some_and(|t| t.text == "[") {
            let attr_end = close(tokens, i + 1)?;
            if tokens[i + 2..attr_end]
                .iter()
                .any(|token| token.text == "path")
            {
                return None;
            }
            i = attr_end + 1;
            continue;
        }
        if tokens[i].kind == Kind::Ident
            && tokens.get(i + 1).is_some_and(|t| t.text == "!")
            && tokens
                .get(i + 2)
                .is_some_and(|t| matches!(t.text.as_str(), "(" | "[" | "{"))
        {
            i = close(tokens, i + 2)? + 1;
            continue;
        }
        if tokens[i].text == "mod" && tokens.get(i + 1).is_some_and(|t| t.kind == Kind::Ident) {
            let name = tokens[i + 1].text.clone();
            let mut child_prefix = prefix.to_vec();
            child_prefix.push(name.clone());
            match tokens.get(i + 2).map(|t| t.text.as_str()) {
                Some(";") => {
                    let candidates = [
                        relative_child(base, &format!("{name}.rs")),
                        relative_child(base, &format!("{name}/mod.rs")),
                    ];
                    let mut existing = candidates
                        .into_iter()
                        .filter(|candidate| root.join(candidate).is_file());
                    let candidate = existing.next()?;
                    if existing.next().is_some() {
                        return None;
                    }
                    if found
                        .insert(candidate.clone(), child_prefix.clone())
                        .is_some()
                    {
                        return None;
                    }
                    let child_source = fs::read_to_string(root.join(&candidate)).ok()?;
                    let child_tokens = lex_rust(&child_source);
                    let child_base = relative_child(base, &name);
                    collect_module_paths(
                        root,
                        &child_tokens,
                        0,
                        child_tokens.len(),
                        &child_base,
                        &child_prefix,
                        found,
                    )?;
                    i += 3;
                    continue;
                }
                Some("{") => {
                    let open = i + 2;
                    let close = close(tokens, open)?;
                    let child_base = relative_child(base, &name);
                    collect_module_paths(
                        root,
                        tokens,
                        open + 1,
                        close,
                        &child_base,
                        &child_prefix,
                        found,
                    )?;
                    i = close + 1;
                    continue;
                }
                _ => (),
            }
        }
        if matches!(tokens[i].text.as_str(), "(" | "[" | "{") {
            i = close(tokens, i)? + 1;
        } else {
            i += 1;
        }
    }
    Some(())
}

fn relative_child(base: &str, child: &str) -> String {
    if base.is_empty() {
        child.to_owned()
    } else {
        format!("{base}/{child}")
    }
}

fn rust_tests(
    source: &str,
    file: &str,
    rust_file: &str,
    unit_target: Option<&CargoUnitTarget>,
    cargo_modules: Option<&[String]>,
    entries: &mut Vec<TestEntry>,
) {
    let stem = rust_file
        .rsplit('/')
        .next()
        .unwrap()
        .trim_end_matches(".rs");
    let integration = unit_target.is_some_and(|target| target.kind == "test")
        || unit_target.is_none() && rust_file.starts_with("tests/");
    let target_file = unit_target.map_or_else(
        || if integration { rust_file } else { "src/lib.rs" },
        |target| target.path.as_str(),
    );
    let context = Context {
        source,
        file,
        // libtest reports the harness file rather than each module's declaration file.
        identity_file: target_file,
        framework: "libtest",
        target: unit_target.map_or_else(
            || if integration { stem } else { "lib" },
            |target| target.name.as_str(),
        ),
        language: Language::Rust,
    };
    let mut modules = Vec::new();
    if let Some(cargo_modules) = cargo_modules {
        modules.extend_from_slice(cargo_modules);
    } else if unit_target.is_some_and(|target| {
        (target.kind == "bin" || target.kind == "lib" && target.path != "src/lib.rs")
            && rust_file != target.path
    }) {
        let root_name = target_file
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".rs");
        let module_dir = if root_name == "main" {
            target_file
                .rsplit_once('/')
                .map_or("", |(parent, _)| parent)
                .to_owned()
        } else {
            let parent = target_file
                .rsplit_once('/')
                .map_or("", |(parent, _)| parent);
            format!("{parent}/{root_name}")
        };
        if let Some(relative) = rust_file.strip_prefix(&format!("{module_dir}/")) {
            modules.extend(relative.split('/').filter_map(|part| {
                let name = part.trim_end_matches(".rs");
                (!matches!(name, "main" | "mod")).then(|| name.to_owned())
            }));
        }
    } else if !integration
        && (unit_target.is_none()
            || unit_target
                .is_some_and(|target| target.kind == "lib" && target.path == "src/lib.rs"))
    {
        let path = rust_file
            .strip_prefix("src/")
            .unwrap_or(rust_file)
            .trim_end_matches(".rs");
        modules.extend(
            path.split('/')
                .filter(|s| !matches!(*s, "lib" | "main" | "mod"))
                .map(str::to_owned),
        );
    }
    rust_scope(&context, &lex_rust(source), &mut modules, entries);
}
fn rust_scope(
    context: &Context<'_>,
    tokens: &[Token],
    modules: &mut Vec<String>,
    entries: &mut Vec<TestEntry>,
) {
    let mut i = 0;
    let mut attributes = Vec::new();
    let mut attr_start = None;
    while i < tokens.len() {
        if tokens[i].text == "#" && tokens.get(i + 1).is_some_and(|t| t.text == "[") {
            if let Some(end) = close(tokens, i + 1) {
                attr_start.get_or_insert(tokens[i].start);
                attributes.push(&tokens[i + 2..end]);
                i = end + 1;
                continue;
            }
        }
        if tokens[i].text == "mod" && tokens.get(i + 2).is_some_and(|t| t.text == "{") {
            if let Some(end) = close(tokens, i + 2) {
                modules.push(tokens[i + 1].text.clone());
                rust_scope(context, &tokens[i + 3..end], modules, entries);
                modules.pop();
                i = end + 1;
                attributes.clear();
                attr_start = None;
                continue;
            }
        }
        if tokens[i].text == "fn" {
            if let Some(open) =
                (i + 2..tokens.len()).find(|p| matches!(tokens[*p].text.as_str(), "{" | ";"))
            {
                if let Some(end) = close(tokens, open) {
                    if attributes
                        .iter()
                        .any(|attr| rust_attribute(attr) == Some("test"))
                    {
                        let mut name = modules.clone();
                        name.push(tokens[i + 1].text.clone());
                        let mut entry = context.entry(
                            name.join("::"),
                            attr_start.unwrap_or(tokens[i].start),
                            tokens[end].end,
                            tokens[open].end,
                            tokens[end].start,
                            attributes
                                .iter()
                                .any(|attr| rust_attribute(attr) == Some("should_panic")),
                        );
                        entry.attributes = attributes
                            .iter()
                            .map(|attr| {
                                context.source[attr.first().map_or(tokens[i].start, |t| t.start)
                                    ..attr.last().map_or(tokens[i].start, |t| t.end)]
                                    .to_owned()
                            })
                            .collect();
                        entry.unsupported_attributes =
                            attributes.iter().any(|attr| rust_attribute(attr).is_none());
                        entries.push(entry);
                    }
                    i = end + 1;
                    attributes.clear();
                    attr_start = None;
                    continue;
                }
            }
        }
        // Macro invocations and macro_rules bodies are opaque token trees.
        if tokens[i].text == "!" {
            let mut open = i + 1;
            if tokens.get(open).is_some_and(|t| t.kind == Kind::Ident) {
                open += 1;
            }
            if tokens
                .get(open)
                .is_some_and(|t| matches!(t.text.as_str(), "(" | "[" | "{"))
            {
                let Some(end) = close(tokens, open) else {
                    break;
                };
                i = end + 1;
                attributes.clear();
                attr_start = None;
                continue;
            }
        }
        // Do not discover tests inside strings or arbitrary blocks.
        if tokens[i].text == "{" {
            if let Some(end) = close(tokens, i) {
                i = end;
            }
        }
        if matches!(tokens[i].text.as_str(), ";" | "}") {
            attributes.clear();
            attr_start = None;
        }
        i += 1;
    }
}
fn rust_attribute(tokens: &[Token]) -> Option<&str> {
    match tokens {
        [name, open, .., end]
            if name.text == "cfg"
                && open.text == "("
                && end.text == ")"
                && close(tokens, 1) == Some(tokens.len() - 1) =>
        {
            Some("cfg")
        }
        [name] if matches!(name.text.as_str(), "test" | "ignore" | "should_panic") => {
            Some(&name.text)
        }
        [name, open, expected, equals, value, end]
            if name.text == "should_panic"
                && open.text == "("
                && expected.text == "expected"
                && equals.text == "="
                && value.kind == Kind::String
                && end.text == ")" =>
        {
            Some("should_panic")
        }
        _ => None,
    }
}
fn python_tests(source: &str, file: &str, entries: &mut Vec<TestEntry>) {
    let context = Context {
        source,
        file,
        identity_file: file,
        framework: "pytest",
        target: target(file),
        language: Language::Python,
    };
    let tokens = lex(source, true);
    let definition = Regex::new(r"^([ \t]*)(?:async\s+)?def\s+(test_[A-Za-z0-9_]+)\s*\(").unwrap();
    let class = Regex::new(r"^([ \t]*)class\s+([A-Za-z_][A-Za-z0-9_]*)\b").unwrap();
    let mut scopes: Vec<(usize, String, bool)> = Vec::new();
    let lines: Vec<_> = source.split_inclusive('\n').collect();
    let mut offset = 0;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let code = tokens
            .iter()
            .any(|t| t.start == offset + indent && t.kind != Kind::String);
        if !code {
            offset += line.len();
            continue;
        }
        while scopes.last().is_some_and(|(depth, _, _)| indent <= *depth) {
            scopes.pop();
        }
        if let Some(c) = class.captures(line) {
            scopes.push((indent, c[2].to_owned(), true));
        } else if let Some(c) = definition.captures(line) {
            if scopes.iter().all(|(_, _, class)| *class) {
                let mut name: Vec<_> = scopes.iter().map(|(_, name, _)| name.clone()).collect();
                name.push(c[2].to_owned());
                let begin = tokens
                    .iter()
                    .position(|t| t.start == offset + indent)
                    .unwrap();
                if let Some(open) = (begin..tokens.len()).find(|j| tokens[*j].text == "(") {
                    if let Some(close) = close(&tokens, open) {
                        if let Some(colon) =
                            (close + 1..tokens.len()).find(|j| tokens[*j].text == ":")
                        {
                            let header_line = source[..tokens[colon].end]
                                .bytes()
                                .filter(|b| *b == b'\n')
                                .count();
                            let mut end = header_line + 1;
                            while end < lines.len() {
                                let next = lines[end];
                                if !next.trim().is_empty()
                                    && !next.trim_start().starts_with('#')
                                    && next.len() - next.trim_start().len() <= indent
                                {
                                    break;
                                }
                                end += 1;
                            }
                            let end_offset =
                                offset + lines[i..end].iter().map(|s| s.len()).sum::<usize>();
                            let start = python_decorator_start(source, &tokens, begin, indent);
                            let start_offset = if start == begin {
                                offset
                            } else {
                                tokens[start].start
                            };
                            if tokens[colon].end < end_offset {
                                let mut entry = context.entry(
                                    name.join("::"),
                                    start_offset,
                                    end_offset,
                                    tokens[colon].end,
                                    end_offset,
                                    false,
                                );
                                entry.decorated = start != begin;
                                if entry.decorated
                                    && tokens[start..begin].iter().any(|t| t.text == "parametrize")
                                {
                                    let cases = python_cases(&tokens[start..begin]);
                                    entry.unknown_cases = cases.is_none();
                                    entry.case_ids = cases.unwrap_or_default();
                                }
                                entries.push(entry);
                            }
                        }
                    }
                }
            }
            scopes.push((indent, c[2].to_owned(), false));
        } else if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
            scopes.push((indent, String::new(), false));
        }
        offset += line.len();
    }
}
// Walk complete logical decorator expressions, including parenthesized and
// backslash continuations. Comments/blank lines are absent from the token stream.
fn python_decorator_start(source: &str, tokens: &[Token], begin: usize, indent: usize) -> usize {
    let mut i = 0;
    let mut chain_start = begin;
    let mut chain_end = 0;
    while i < begin {
        let line = source[..tokens[i].start].rfind('\n').map_or(0, |n| n + 1);
        let prefix = &source[line..tokens[i].start];
        if tokens[i].text != "@" || prefix.len() != indent || !prefix.trim().is_empty() {
            // In particular, an @ operator inside a multiline expression is
            // not the start of a new decorator.
            if matches!(tokens[i].text.as_str(), "(" | "[" | "{") {
                if let Some(end) = close(tokens, i) {
                    i = end;
                }
            }
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < begin {
            if source[tokens[end - 1].end..tokens[end].start].contains('\n')
                && tokens[end - 1].text != "\\"
            {
                break;
            }
            if matches!(tokens[end].text.as_str(), "(" | "[" | "{") {
                let Some(last) = close(tokens, end) else {
                    return begin;
                };
                end = last;
            }
            end += 1;
        }
        if chain_start == begin || chain_end != i {
            chain_start = i;
        }
        if end == begin {
            return chain_start;
        }
        chain_end = end;
        i = end;
    }
    begin
}
fn js_tests(source: &str, file: &str, entries: &mut Vec<TestEntry>) {
    let tokens = lex_javascript(source);
    let framework = if tokens.iter().enumerate().any(|(i, t)| {
        t.kind == Kind::String
            && matches!(t.text.as_str(), "'vitest'" | "\"vitest\"")
            && (i > 0 && tokens[i - 1].text == "from" || i > 1 && tokens[i - 2].text == "require")
    }) {
        "vitest"
    } else {
        "jest"
    };
    let context = Context {
        source,
        file,
        identity_file: file,
        framework,
        target: target(file),
        language: Language::JavaScript,
    };
    js_scope(&context, &tokens, &mut Vec::new(), entries);
}
fn js_scope(
    context: &Context<'_>,
    tokens: &[Token],
    describes: &mut Vec<String>,
    entries: &mut Vec<TestEntry>,
) {
    let mut i = 0;
    while i + 2 < tokens.len() {
        let kind = tokens[i].text.as_str();
        if matches!(kind, "describe" | "it" | "test") && (i == 0 || tokens[i - 1].text != ".") {
            let mut open = i + 1;
            let mut each_table = None;
            if tokens[open].text == "."
                && tokens
                    .get(open + 1)
                    .is_some_and(|t| matches!(t.text.as_str(), "only" | "skip"))
            {
                open += 2;
            }
            if kind != "describe"
                && tokens.get(open).is_some_and(|t| t.text == ".")
                && tokens.get(open + 1).is_some_and(|t| t.text == "each")
                && tokens.get(open + 2).is_some_and(|t| t.text == "(")
            {
                if let Some(end) = close(tokens, open + 2) {
                    each_table = Some(&tokens[open + 3..end]);
                    open = end + 1;
                }
            } else if kind != "describe"
                && tokens.get(open).is_some_and(|t| t.text == ".")
                && tokens.get(open + 1).is_some_and(|t| t.text == "each")
                && tokens
                    .get(open + 2)
                    .is_some_and(|t| t.text.starts_with('`'))
            {
                each_table = Some(&tokens[open + 2..open + 3]);
                open += 3;
            }
            if tokens.get(open).is_some_and(|t| t.text == "(")
                && tokens.get(open + 1).is_some_and(|t| t.kind == Kind::String)
            {
                if let Some(end) = close(tokens, open) {
                    let title = &tokens[open + 1].text;
                    if title.contains("${") {
                        i = end + 1;
                        continue;
                    }
                    if let Some(body) = callback_body(tokens, open + 2, end) {
                        if let Some(body_end) = close(tokens, body) {
                            let name = title[1..title.len() - 1].to_owned();
                            if kind == "describe" {
                                describes.push(name);
                                js_scope(context, &tokens[body + 1..body_end], describes, entries);
                                describes.pop();
                            } else {
                                let mut names = describes.clone();
                                names.push(name);
                                let mut entry = context.entry(
                                    names.join(" > "),
                                    tokens[i].start,
                                    tokens[end].end,
                                    tokens[body].end,
                                    tokens[body_end].start,
                                    false,
                                );
                                if let Some(table) = each_table {
                                    let cases = js_cases(table, &entry.name);
                                    entry.unknown_cases = cases.is_none();
                                    entry.case_ids = cases.unwrap_or_default();
                                }
                                entries.push(entry);
                            }
                        }
                    }
                    i = end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
}
fn callback_body(tokens: &[Token], comma: usize, end: usize) -> Option<usize> {
    if tokens.get(comma)?.text != "," {
        return None;
    }
    let mut begin = comma + 1;
    if tokens.get(begin)?.text == "async" {
        begin += 1;
    }
    if tokens.get(begin)?.text == "function" {
        begin += 1;
        if tokens.get(begin)?.kind == Kind::Ident {
            begin += 1;
        }
        let signature_end = close(tokens, begin)?;
        return (signature_end + 1 < end && tokens[signature_end + 1].text == "{")
            .then_some(signature_end + 1);
    }
    let signature_end = if tokens.get(begin)?.text == "(" {
        close(tokens, begin)?
    } else {
        begin
    };
    (signature_end + 2 < end
        && tokens[signature_end + 1].text == "=>"
        && tokens[signature_end + 2].text == "{")
        .then_some(signature_end + 2)
}
fn collect(dir: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    for item in fs::read_dir(dir)? {
        let item = item?;
        let kind = item.file_type()?;
        if kind.is_dir() {
            if !matches!(
                item.file_name().to_str(),
                Some(".git" | "target" | "node_modules" | "__pycache__" | ".venv")
            ) {
                collect(&item.path(), paths)?;
            }
        } else if kind.is_file() {
            paths.push(item.path());
        }
    }
    Ok(())
}
