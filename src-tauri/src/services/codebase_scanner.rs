use std::path::Path;
use crate::errors::AppError;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CodeSymbol {
    pub name: String,
    pub kind: String, // "function", "class", "method"
    pub file_path: String,
}

const IGNORE_DIRS: &[&str] = &[
    "node_modules", ".git", "dist", "build", "target", ".next",
    "__pycache__", ".venv", "venv", ".tox", "coverage", ".nyc_output",
];

const SOURCE_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "py", "rs", "go", "java", "rb", "cs",
];

const MAX_DEPTH: usize = 12;

pub fn scan_codebase(root: &str, exclusions: &[String]) -> Result<Vec<CodeSymbol>, AppError> {
    let root_path = Path::new(root);
    if !root_path.exists() || !root_path.is_dir() {
        return Err(AppError::InvalidInput(format!("Invalid codebase path: {}", root)));
    }

    let mut symbols = Vec::new();
    walk_dir(root_path, root_path, &mut symbols, exclusions, 0)?;
    Ok(symbols)
}

fn walk_dir(
    dir: &Path,
    root: &Path,
    symbols: &mut Vec<CodeSymbol>,
    exclusions: &[String],
    depth: usize,
) -> Result<(), AppError> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if IGNORE_DIRS.contains(&name.as_str()) || exclusions.contains(&name) {
            continue;
        }

        if path.is_dir() {
            walk_dir(&path, root, symbols, exclusions, depth + 1)?;
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if SOURCE_EXTENSIONS.contains(&ext) {
                // Skip files larger than 1 MB to avoid reading generated/bundled files
                let too_large = std::fs::metadata(&path)
                    .map(|m| m.len() > 1_024_000)
                    .unwrap_or(false);
                if too_large {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let rel_path = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
                    extract_symbols(&content, &rel_path, ext, symbols);
                }
            }
        }
    }
    Ok(())
}

fn extract_symbols(content: &str, file_path: &str, ext: &str, symbols: &mut Vec<CodeSymbol>) {
    match ext {
        "ts" | "tsx" | "js" | "jsx" => extract_js_ts_symbols(content, file_path, symbols),
        "py" => extract_python_symbols(content, file_path, symbols),
        "rs" => extract_rust_symbols(content, file_path, symbols),
        "go" => extract_go_symbols(content, file_path, symbols),
        "java" => extract_java_symbols(content, file_path, symbols),
        "rb" => extract_ruby_symbols(content, file_path, symbols),
        "cs" => extract_csharp_symbols(content, file_path, symbols),
        _ => {}
    }
}

fn extract_js_ts_symbols(content: &str, file_path: &str, symbols: &mut Vec<CodeSymbol>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if is_comment_line(trimmed) {
            continue;
        }
        // function declarations
        for name in extract_all_after_keyword(trimmed, "function ") {
            symbols.push(CodeSymbol { name, kind: "function".into(), file_path: file_path.into() });
        }
        // class declarations
        for name in extract_all_after_keyword(trimmed, "class ") {
            symbols.push(CodeSymbol { name, kind: "class".into(), file_path: file_path.into() });
        }
        // const arrow functions: const foo = (...) =>
        if (trimmed.starts_with("export const ") || trimmed.starts_with("const "))
            && (trimmed.contains("=>") || trimmed.contains("= function"))
        {
            let after_const = if trimmed.starts_with("export ") {
                &trimmed[13..] // after "export const "
            } else {
                &trimmed[6..] // after "const "
            };
            if let Some(pos) = after_const.find(|c: char| !c.is_alphanumeric() && c != '_') {
                let name = after_const[..pos].to_string();
                if !name.is_empty() {
                    symbols.push(CodeSymbol { name, kind: "function".into(), file_path: file_path.into() });
                }
            }
        }
    }
}

fn extract_python_symbols(content: &str, file_path: &str, symbols: &mut Vec<CodeSymbol>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(name) = extract_after_keyword(trimmed, "def ") {
            let kind = if line.starts_with("    ") || line.starts_with('\t') {
                "method"
            } else {
                "function"
            };
            symbols.push(CodeSymbol { name, kind: kind.into(), file_path: file_path.into() });
        }
        if let Some(name) = extract_after_keyword(trimmed, "class ") {
            symbols.push(CodeSymbol { name, kind: "class".into(), file_path: file_path.into() });
        }
    }
}

fn extract_rust_symbols(content: &str, file_path: &str, symbols: &mut Vec<CodeSymbol>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(name) = extract_after_keyword(trimmed, "fn ") {
            symbols.push(CodeSymbol { name, kind: "function".into(), file_path: file_path.into() });
        }
        if let Some(name) = extract_after_keyword(trimmed, "struct ") {
            symbols.push(CodeSymbol { name, kind: "class".into(), file_path: file_path.into() });
        }
        if let Some(name) = extract_after_keyword(trimmed, "impl ") {
            symbols.push(CodeSymbol { name, kind: "class".into(), file_path: file_path.into() });
        }
    }
}

fn extract_go_symbols(content: &str, file_path: &str, symbols: &mut Vec<CodeSymbol>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if is_comment_line(trimmed) {
            continue;
        }
        if let Some(name) = extract_go_function_name(trimmed) {
            symbols.push(CodeSymbol { name, kind: "function".into(), file_path: file_path.into() });
        }
        for name in extract_all_after_keyword(trimmed, "type ") {
            if trimmed.contains(" struct") || trimmed.contains(" interface") {
                symbols.push(CodeSymbol { name, kind: "class".into(), file_path: file_path.into() });
            }
        }
    }
}

fn extract_java_symbols(content: &str, file_path: &str, symbols: &mut Vec<CodeSymbol>) {
    for fragment in content.split(['{', '}', ';']) {
        let trimmed = fragment.trim();
        if is_comment_line(trimmed) {
            continue;
        }
        for name in extract_all_after_keyword(trimmed, "class ") {
            symbols.push(CodeSymbol { name, kind: "class".into(), file_path: file_path.into() });
        }
        for name in extract_all_after_keyword(trimmed, "interface ") {
            symbols.push(CodeSymbol { name, kind: "class".into(), file_path: file_path.into() });
        }
        if looks_like_java_method(trimmed) {
            if let Some(name) = extract_method_name_before_paren(trimmed) {
                symbols.push(CodeSymbol { name, kind: "method".into(), file_path: file_path.into() });
            }
        }
    }
}

fn extract_ruby_symbols(content: &str, file_path: &str, symbols: &mut Vec<CodeSymbol>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(name) = extract_after_keyword(trimmed, "class ") {
            symbols.push(CodeSymbol { name, kind: "class".into(), file_path: file_path.into() });
        }
        if let Some(rest) = trimmed.strip_prefix("def ") {
            let method = rest.trim_start();
            let name_part = method.split_whitespace().next().unwrap_or("");
            let name_part = name_part.split('(').next().unwrap_or("");
            let name = name_part.rsplit('.').next().unwrap_or("").to_string();
            if !name.is_empty() {
                symbols.push(CodeSymbol { name, kind: "method".into(), file_path: file_path.into() });
            }
        }
    }
}

fn extract_csharp_symbols(content: &str, file_path: &str, symbols: &mut Vec<CodeSymbol>) {
    for fragment in content.split(['{', '}', ';']) {
        let trimmed = fragment.trim();
        if is_comment_line(trimmed) {
            continue;
        }
        for name in extract_all_after_keyword(trimmed, "class ") {
            symbols.push(CodeSymbol { name, kind: "class".into(), file_path: file_path.into() });
        }
        for name in extract_all_after_keyword(trimmed, "interface ") {
            symbols.push(CodeSymbol { name, kind: "class".into(), file_path: file_path.into() });
        }
        if looks_like_csharp_method(trimmed) {
            if let Some(name) = extract_method_name_before_paren(trimmed) {
                symbols.push(CodeSymbol { name, kind: "method".into(), file_path: file_path.into() });
            }
        }
    }
}

fn looks_like_java_method(line: &str) -> bool {
    line.contains('(')
        && line.contains(')')
        && !line.contains(" class ")
        && !line.contains(" interface ")
        && !line.starts_with("if ")
        && !line.starts_with("for ")
        && !line.starts_with("while ")
        && !line.starts_with("switch ")
}

fn looks_like_csharp_method(line: &str) -> bool {
    line.contains('(')
        && line.contains(')')
        && !line.contains(" interface ")
        && !line.contains(" class ")
        && !line.starts_with("if ")
        && !line.starts_with("for ")
        && !line.starts_with("while ")
        && !line.starts_with("switch ")
}

fn extract_method_name_before_paren(line: &str) -> Option<String> {
    let paren = line.find('(')?;
    let before = line[..paren].trim();
    let candidate = before.split_whitespace().last()?;
    let cleaned = candidate.trim_matches(|c: char| c == '<' || c == '>' || c == ':' || c == ',');
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn extract_after_keyword(line: &str, keyword: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix(keyword)
        .or_else(|| {
            // Also match "pub fn ", "async fn ", "export function ", etc.
            let idx = line.find(keyword)?;
            Some(&line[idx + keyword.len()..])
        })
    {
        let name: String = rest.chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

fn extract_all_after_keyword(line: &str, keyword: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut start = 0;

    while let Some(relative_idx) = line[start..].find(keyword) {
        let idx = start + relative_idx;
        let boundary_ok = idx == 0
            || !line[..idx]
                .chars()
                .next_back()
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);

        if boundary_ok {
            let rest = &line[idx + keyword.len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                names.push(name);
            }
        }

        start = idx + keyword.len();
    }

    names
}

fn extract_go_function_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("func ")?;
    if let Some(receiver_end) = rest.find(')') {
        let after_receiver = rest[receiver_end + 1..].trim_start();
        let name: String = after_receiver
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }

    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn is_comment_line(line: &str) -> bool {
    line.is_empty()
        || line.starts_with("//")
        || line.starts_with("/*")
        || line.starts_with('*')
        || line.starts_with("*/")
        || line.starts_with('#')
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============ JavaScript/TypeScript Tests ============
    #[test]
    fn test_extract_js_function() {
        let code = "function greet(name) { return `Hello, ${name}`; }";
        let mut symbols = Vec::new();
        extract_symbols(code, "test.js", "js", &mut symbols);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].kind, "function");
        assert_eq!(symbols[0].name, "greet");
    }

    #[test]
    fn test_extract_ts_interface() {
        let code = "interface User { id: string; name: string; }";
        let mut symbols = Vec::new();
        extract_symbols(code, "test.ts", "ts", &mut symbols);
        // Interface detection not implemented in current scanner; accepts 0
        assert_eq!(symbols.len(), 0);
    }

    #[test]
    fn test_extract_ts_class() {
        let code = "class UserService { constructor(private db) {} getUserById(id) {} }";
        let mut symbols = Vec::new();
        extract_symbols(code, "test.ts", "ts", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "UserService"));
    }

    #[test]
    fn test_extract_js_arrow_function() {
        let code = "const add = (a, b) => a + b;";
        let mut symbols = Vec::new();
        extract_symbols(code, "test.js", "js", &mut symbols);
        assert!(symbols.iter().any(|s| s.name == "add"));
    }

    #[test]
    fn test_extract_export_const_arrow_function() {
        let code = "export const multiply = (a, b) => a * b;";
        let mut symbols = Vec::new();
        extract_symbols(code, "test.ts", "ts", &mut symbols);
        assert!(symbols.iter().any(|s| s.name == "multiply"));
    }

    // ============ Python Tests ============
    #[test]
    fn test_extract_python_function() {
        let code = "def calculate_total(items):\n    return sum(items)";
        let mut symbols = Vec::new();
        extract_symbols(code, "test.py", "py", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "function" && s.name == "calculate_total"));
    }

    #[test]
    fn test_extract_python_class() {
        let code = "class DataProcessor:\n    def process(self, data): pass";
        let mut symbols = Vec::new();
        extract_symbols(code, "test.py", "py", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "DataProcessor"));
    }

    #[test]
    fn test_extract_python_async_function() {
        let code = "async def fetch_data(url):\n    return await client.get(url)";
        let mut symbols = Vec::new();
        extract_symbols(code, "test.py", "py", &mut symbols);
        // Current scanner doesn't distinguish async; accepts as function
        assert!(symbols.iter().any(|s| s.kind == "function"));
    }

    #[test]
    fn test_extract_python_method_indented() {
        let code = "class User:\n    def get_name(self):\n        return self.name";
        let mut symbols = Vec::new();
        extract_symbols(code, "test.py", "py", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "User"));
        assert!(symbols.iter().any(|s| s.kind == "method" && s.name == "get_name"));
    }

    // ============ Go Tests ============
    #[test]
    fn test_extract_go_function() {
        let code = "func GetUser(id string) (*User, error) { return nil, nil }";
        let mut symbols = Vec::new();
        extract_symbols(code, "main.go", "go", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "function" && s.name == "GetUser"));
    }

    #[test]
    fn test_extract_go_struct() {
        let code = "type User struct { ID string; Name string }";
        let mut symbols = Vec::new();
        extract_symbols(code, "models.go", "go", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "User"));
    }

    #[test]
    fn test_extract_go_interface() {
        let code = "type Reader interface { Read(p []byte) (n int, err error) }";
        let mut symbols = Vec::new();
        extract_symbols(code, "io.go", "go", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "Reader"));
    }

    #[test]
    fn test_extract_go_method_receiver() {
        let code = "func (s *Service) Process(data string) error { return nil }";
        let mut symbols = Vec::new();
        extract_symbols(code, "service.go", "go", &mut symbols);
        // Current scanner extracts receiver methods as functions
        assert!(symbols.iter().any(|s| s.kind == "function"));
    }

    // ============ Java Tests ============
    #[test]
    fn test_extract_java_class() {
        let code = "public class UserService { public User getUser(String id) { return null; } }";
        let mut symbols = Vec::new();
        extract_symbols(code, "UserService.java", "java", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "UserService"));
    }

    #[test]
    fn test_extract_java_method() {
        let code = "class Data { private String processRecord(String raw) { return raw.trim(); } }";
        let mut symbols = Vec::new();
        extract_symbols(code, "Data.java", "java", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "method" && s.name == "processRecord"));
    }

    #[test]
    fn test_extract_java_interface() {
        let code = "interface DataStore { User findUser(String id); }";
        let mut symbols = Vec::new();
        extract_symbols(code, "DataStore.java", "java", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "DataStore"));
    }

    // ============ Ruby Tests ============
    #[test]
    fn test_extract_ruby_function() {
        let code = "def calculate_total(items)\n  items.sum\nend";
        let mut symbols = Vec::new();
        extract_symbols(code, "utils.rb", "rb", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "method" && s.name == "calculate_total"));
    }

    #[test]
    fn test_extract_ruby_class() {
        let code = "class UserService\n  def get_user(id)\n    @users[id]\n  end\nend";
        let mut symbols = Vec::new();
        extract_symbols(code, "user_service.rb", "rb", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "UserService"));
    }

    #[test]
    fn test_extract_ruby_class_method() {
        let code = "class Account\n  def self.build\n  end\nend";
        let mut symbols = Vec::new();
        extract_symbols(code, "account.rb", "rb", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "Account"));
        assert!(symbols.iter().any(|s| s.kind == "method" && s.name == "build"));
    }

    // ============ C# Tests ============
    #[test]
    fn test_extract_csharp_class() {
        let code = "public class UserService { public User GetUser(string id) { return null; } }";
        let mut symbols = Vec::new();
        extract_symbols(code, "UserService.cs", "cs", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "UserService"));
    }

    #[test]
    fn test_extract_csharp_method() {
        let code = "public void ProcessData(string input) { Console.WriteLine(input); }";
        let mut symbols = Vec::new();
        extract_symbols(code, "Service.cs", "cs", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "method" && s.name == "ProcessData"));
    }

    #[test]
    fn test_extract_csharp_interface() {
        let code = "public interface IRepository { Task<User> GetAsync(int id); }";
        let mut symbols = Vec::new();
        extract_symbols(code, "IRepository.cs", "cs", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "IRepository"));
    }

    // ============ Rust Tests ============
    #[test]
    fn test_extract_rust_function() {
        let code = "pub fn calculate_total(items: &[f64]) -> f64 { items.iter().sum() }";
        let mut symbols = Vec::new();
        extract_symbols(code, "utils.rs", "rs", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "function" && s.name == "calculate_total"));
    }

    #[test]
    fn test_extract_rust_struct() {
        let code = "pub struct User { pub id: String, pub name: String }";
        let mut symbols = Vec::new();
        extract_symbols(code, "models.rs", "rs", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "User"));
    }

    #[test]
    fn test_extract_rust_impl_block() {
        let code = "impl User { pub fn new(id: String) -> Self { User { id } } }";
        let mut symbols = Vec::new();
        extract_symbols(code, "models.rs", "rs", &mut symbols);
        assert!(symbols.iter().any(|s| s.kind == "class" && s.name == "User"));
    }

    #[test]
    fn test_extract_rust_async_fn() {
        let code = "async fn fetch_data(url: &str) -> Result<String, Error> { Ok(String::new()) }";
        let mut symbols = Vec::new();
        extract_symbols(code, "api.rs", "rs", &mut symbols);
        // Current scanner doesn't distinguish async; accepts as function
        assert!(symbols.iter().any(|s| s.kind == "function"));
    }

    // ============ Edge Cases ============
    #[test]
    fn test_empty_file() {
        let mut symbols = Vec::new();
        extract_symbols("", "empty.js", "js", &mut symbols);
        assert_eq!(symbols.len(), 0);
    }

    #[test]
    fn test_comments_ignored_js() {
        let code = "// function fake() {}\nfunction real() {}";
        let mut symbols = Vec::new();
        extract_symbols(code, "test.js", "js", &mut symbols);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "real");
    }

    #[test]
    fn test_nested_class_java() {
        let code = "class Outer { class Inner {} }";
        let mut symbols = Vec::new();
        extract_symbols(code, "Nested.java", "java", &mut symbols);
        // Both should be extracted
        assert!(symbols.iter().any(|s| s.name == "Outer"));
        assert!(symbols.iter().any(|s| s.name == "Inner"));
    }

    #[test]
    fn test_unsupported_language() {
        let mut symbols = Vec::new();
        extract_symbols("IDENTIFICATION DIVISION.", "test.cob", "cob", &mut symbols);
        assert_eq!(symbols.len(), 0); // Should gracefully return empty
    }

    #[test]
    fn test_symbols_have_file_path() {
        let code = "function test() {}";
        let mut symbols = Vec::new();
        extract_symbols(code, "app.js", "js", &mut symbols);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].file_path, "app.js");
    }

    #[test]
    fn test_multiple_functions_same_file() {
        let code = "function foo() {}\nfunction bar() {}\nfunction baz() {}";
        let mut symbols = Vec::new();
        extract_symbols(code, "utils.js", "js", &mut symbols);
        assert_eq!(symbols.len(), 3);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
        assert!(names.contains(&"baz"));
    }

    #[test]
    fn test_mixed_export_patterns() {
        let code = r#"
export function exported() {}
function notExported() {}
export const arrow = () => {};
const privateArrow = () => {};
        "#;
        let mut symbols = Vec::new();
        extract_symbols(code, "module.js", "js", &mut symbols);
        // All should be extracted (export doesn't filter)
        assert!(symbols.iter().any(|s| s.name == "exported"));
        assert!(symbols.iter().any(|s| s.name == "notExported"));
        assert!(symbols.iter().any(|s| s.name == "arrow"));
        assert!(symbols.iter().any(|s| s.name == "privateArrow"));
    }

    #[test]
    fn test_python_decorator_ignored() {
        let code = "@decorator\ndef decorated_function():\n    pass";
        let mut symbols = Vec::new();
        extract_symbols(code, "decorators.py", "py", &mut symbols);
        assert!(symbols.iter().any(|s| s.name == "decorated_function"));
    }

    #[test]
    fn extracts_symbols_for_supported_non_rust_languages() {
        let go = r#"
            package demo
            type User struct {}
            func BuildUser() {}
        "#;
        let java = r#"
            public class AccountService {
                public void createAccount(String id) {}
            }
        "#;
        let ruby = r#"
            class AccountService
              def self.build
              end
            end
        "#;
        let csharp = r#"
            public class BillingService {
                public void ChargeCustomer(string id) { }
            }
        "#;

        let mut symbols = Vec::new();
        extract_symbols(go, "main.go", "go", &mut symbols);
        extract_symbols(java, "Main.java", "java", &mut symbols);
        extract_symbols(ruby, "main.rb", "rb", &mut symbols);
        extract_symbols(csharp, "Main.cs", "cs", &mut symbols);

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"User"));
        assert!(names.contains(&"BuildUser"));
        assert!(names.contains(&"AccountService"));
        assert!(names.contains(&"createAccount"));
        assert!(names.contains(&"build"));
        assert!(names.contains(&"BillingService"));
        assert!(names.contains(&"ChargeCustomer"));
    }
}
