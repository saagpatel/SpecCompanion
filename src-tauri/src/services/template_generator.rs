use crate::models::spec::Requirement;
use crate::services::codebase_scanner::CodeSymbol;

pub fn generate_jest_test(requirement: &Requirement, symbols: &[CodeSymbol]) -> String {
    let desc = &requirement.description;
    let section = &requirement.section;
    let relevant = find_relevant_symbols(desc, symbols);

    let mut code = String::new();
    code.push_str(&format!("// Requirement-ID: {}\n", requirement.id));
    code.push_str(&format!("// Requirement: {}\n", desc));
    code.push_str(&format!("// Section: {}\n", section));
    code.push_str(&format!(
        "// Type: {} | Priority: {}\n\n",
        requirement.req_type, requirement.priority
    ));

    if !relevant.is_empty() {
        for sym in &relevant {
            code.push_str(&format!(
                "// import {{ {} }} from './{}';\n",
                sym.name, sym.file_path
            ));
        }
        code.push('\n');
    }

    code.push_str(&format!(
        "describe('{}', () => {{\n",
        escape_js_string(section)
    ));
    code.push_str(&format!(
        "  it('should {}', () => {{\n",
        escape_js_string(&make_test_description(desc))
    ));

    if let Some(assertion) = generate_assertion_hint(desc) {
        code.push_str(&format!("    // TODO: {}\n", assertion));
    }
    code.push_str("    // Arrange\n");
    code.push_str("    \n");
    code.push_str("    // Act\n");
    code.push_str("    \n");
    code.push_str("    // Assert\n");
    code.push_str("    expect(true).toBe(true); // TODO: Replace with actual assertion\n");
    code.push_str("  });\n");
    code.push_str("});\n");

    code
}

pub fn generate_pytest_test(requirement: &Requirement, symbols: &[CodeSymbol]) -> String {
    let desc = &requirement.description;
    let section = &requirement.section;
    let relevant = find_relevant_symbols(desc, symbols);

    let mut code = String::new();
    code.push_str(&format!("# Requirement-ID: {}\n", requirement.id));
    code.push_str(&format!("# Requirement: {}\n", desc));
    code.push_str(&format!("# Section: {}\n", section));
    code.push_str(&format!(
        "# Type: {} | Priority: {}\n\n",
        requirement.req_type, requirement.priority
    ));

    if !relevant.is_empty() {
        for sym in &relevant {
            code.push_str(&format!(
                "# from {} import {}\n",
                sym.file_path.replace('/', ".").replace(".py", ""),
                sym.name
            ));
        }
        code.push('\n');
    }

    let test_name = make_python_test_name(desc);
    code.push_str(&format!("class Test{}:\n", make_class_name(section)));
    code.push_str(&format!("    def {}(self):\n", test_name));
    code.push_str(&format!(
        "        \"\"\"Test: {}\"\"\"\n",
        desc.replace("\"\"\"", "\\\"\\\"\\\"")
    ));
    code.push_str("        # Arrange\n");
    code.push_str("        \n");
    code.push_str("        # Act\n");
    code.push_str("        \n");
    code.push_str("        # Assert\n");
    code.push_str("        assert True  # TODO: Replace with actual assertion\n");

    code
}

fn find_relevant_symbols<'a>(description: &str, symbols: &'a [CodeSymbol]) -> Vec<&'a CodeSymbol> {
    let lower_desc = description.to_lowercase();
    let words: Vec<&str> = lower_desc
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .collect();

    symbols
        .iter()
        .filter(|sym| {
            let lower_name = sym.name.to_lowercase();
            words
                .iter()
                .any(|word| lower_name.contains(word) || word.contains(lower_name.as_str()))
        })
        .take(5)
        .collect()
}

fn make_test_description(desc: &str) -> String {
    let lower = desc.to_lowercase();
    if let Some(rest) = lower.strip_prefix("the system shall ") {
        rest.to_string()
    } else if let Some(rest) = lower.strip_prefix("the system must ") {
        rest.to_string()
    } else if lower.starts_with("as a ") {
        if let Some(idx) = lower.find("i want to ") {
            format!("allow {}", &lower[idx + "i want to ".len()..])
        } else if let Some(idx) = lower.find("i should be able to ") {
            format!("allow {}", &lower[idx + "i should be able to ".len()..])
        } else {
            lower
        }
    } else {
        lower
    }
}

fn make_python_test_name(desc: &str) -> String {
    let words: Vec<&str> = desc.split_whitespace().take(8).collect();
    let name: String = words
        .join("_")
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("test_{}", name.trim_matches('_'))
}

fn make_class_name(section: &str) -> String {
    section
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    let rest: String = chars.filter(|c| c.is_alphanumeric()).collect();
                    format!("{}{}", upper, rest)
                }
                None => String::new(),
            }
        })
        .collect()
}

fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn generate_assertion_hint(desc: &str) -> Option<String> {
    let lower = desc.to_lowercase();
    if lower.contains("authenti") || lower.contains("login") {
        Some("Verify authentication flow works correctly".to_string())
    } else if lower.contains("creat") || lower.contains("add") {
        Some("Verify resource is created successfully".to_string())
    } else if lower.contains("delet") || lower.contains("remov") {
        Some("Verify resource is deleted successfully".to_string())
    } else if lower.contains("updat") || lower.contains("edit") || lower.contains("modif") {
        Some("Verify resource is updated correctly".to_string())
    } else if lower.contains("list")
        || lower.contains("display")
        || lower.contains("show")
        || lower.contains("view")
    {
        Some("Verify data is displayed correctly".to_string())
    } else if lower.contains("validat") || lower.contains("check") {
        Some("Verify validation rules are enforced".to_string())
    } else {
        None
    }
}
