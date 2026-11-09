use super::syntax::{close, lex, lex_javascript, lex_rust, Kind, Token};
use crate::core::annotations::Link;
use crate::core::test_identity::fingerprint;
use regex::Regex;
use std::{
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
    pub decorated: bool,
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
        self.entries
            .iter()
            .map(|entry| {
                (
                    entry.id.clone(),
                    crate::core::test_identity::TestIdentity {
                        fingerprint: entry.fingerprint.clone(),
                        rename_hint: Some(entry.rename_hint.clone()),
                    },
                )
            })
            .collect()
    }
    /// Map explicit scoped metadata or pytest's file/class identity, never a bare name guess.
    pub fn junit_id(&self, class: &str, name: &str) -> String {
        let fallback = if class.is_empty() {
            name.to_owned()
        } else {
            format!("{class}::{name}")
        };
        let mut matches = self.entries.iter().filter(|entry| {
            entry.id == fallback
                || entry.id == format!("{class}:{name}")
                || (entry.language == Language::Python
                    && format!("{class}.{name}")
                        == format!(
                            "{}.{}",
                            entry.file.trim_end_matches(".py").replace('/', "."),
                            entry.name.replace("::", ".")
                        ))
        });
        match (matches.next(), matches.next()) {
            (Some(entry), None) => entry.id.clone(),
            _ => fallback,
        }
    }
    pub fn get(&self, id: &str) -> Option<&TestEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }
    /// Scan text only: no imports, compilation, execution, or symlink traversal.
    pub fn build(root: &Path) -> io::Result<Self> {
        // A crate's tests directory is a scan boundary, not an identity root.
        // Cargo reports integration paths relative to the crate manifest.
        let identity_root = if root.file_name().is_some_and(|name| name == "tests")
            && root
                .parent()
                .is_some_and(|parent| parent.join("Cargo.toml").is_file())
        {
            root.parent().unwrap()
        } else {
            root
        };
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
                    let (rust_file, unit_target) = if let Some(crate_root) = crate_root {
                        let relative = path
                            .strip_prefix(crate_root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/");
                        let target = cargo_unit_target(crate_root, &relative)?;
                        (relative, target)
                    } else {
                        (file.clone(), None)
                    };
                    rust_tests(
                        &source,
                        &file,
                        &rust_file,
                        unit_target.as_deref(),
                        &mut entries,
                    );
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
            decorated: false,
        }
    }
}
// Read the Cargo target names without executing the project.
fn cargo_unit_target(root: &Path, file: &str) -> io::Result<Option<String>> {
    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    let mut section = "";
    let mut package = None;
    let mut library = None;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line;
        } else if let Some((key, value)) = line.split_once('=') {
            if key.trim() == "name" {
                let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
                if section == "[package]" {
                    package = Some(value.to_owned());
                }
                if section == "[lib]" {
                    library = Some(value.to_owned());
                }
            }
        }
    }
    Ok(if file == "src/main.rs" {
        package.map(|name| name.replace('-', "_"))
    } else {
        library.or(package.map(|name| name.replace('-', "_")))
    })
}
fn rust_tests(
    source: &str,
    file: &str,
    rust_file: &str,
    unit_target: Option<&str>,
    entries: &mut Vec<TestEntry>,
) {
    let stem = rust_file
        .rsplit('/')
        .next()
        .unwrap()
        .trim_end_matches(".rs");
    let integration = rust_file.starts_with("tests/");
    let context = Context {
        source,
        file,
        // libtest reports the harness file rather than each module's declaration file.
        identity_file: if integration || rust_file == "src/main.rs" {
            rust_file
        } else {
            "src/lib.rs"
        },
        framework: "libtest",
        target: if integration {
            stem
        } else if rust_file == "src/main.rs" {
            unit_target.unwrap_or("bin")
        } else {
            unit_target.unwrap_or("lib")
        },
        language: Language::Rust,
    };
    let mut modules = Vec::new();
    if !integration {
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
                if i + 2 < end {
                    attributes.push(tokens[i + 2].text.as_str());
                }
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
                    if attributes.contains(&"test") {
                        let mut name = modules.clone();
                        name.push(tokens[i + 1].text.clone());
                        entries.push(context.entry(
                            name.join("::"),
                            attr_start.unwrap_or(tokens[i].start),
                            tokens[end].end,
                            tokens[open].end,
                            tokens[end].start,
                            attributes.contains(&"should_panic"),
                        ));
                    }
                    i = end + 1;
                    attributes.clear();
                    attr_start = None;
                    continue;
                }
            }
        }
        // Do not discover tests inside strings, macros, or arbitrary blocks.
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
            if tokens[open].text == "."
                && tokens
                    .get(open + 1)
                    .is_some_and(|t| matches!(t.text.as_str(), "only" | "skip"))
            {
                open += 2;
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
                                entries.push(context.entry(
                                    names.join(" > "),
                                    tokens[i].start,
                                    tokens[end].end,
                                    tokens[body].end,
                                    tokens[body_end].start,
                                    false,
                                ));
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
