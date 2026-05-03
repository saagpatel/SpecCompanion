use crate::errors::AppError;
use crate::models::spec::Requirement;
use crate::services::codebase_scanner::CodeSymbol;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

pub async fn generate_test_with_llm(
    api_key: &str,
    requirement: &Requirement,
    framework: &str,
    symbols: &[CodeSymbol],
) -> Result<String, AppError> {
    let context = build_context(symbols);
    let prompt = build_prompt(requirement, framework, &context);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(AppError::Http)?;
    let request = ClaudeRequest {
        model: "claude-sonnet-4-6".to_string(),
        max_tokens: 2048,
        messages: vec![Message {
            role: "user".to_string(),
            content: prompt,
        }],
    };

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::General(format!(
            "Claude API error ({}): {}",
            status, body
        )));
    }

    let claude_response: ClaudeResponse = response.json().await?;
    let test_code = claude_response
        .content
        .into_iter()
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("\n");

    // Extract code block if wrapped in markdown
    let code = extract_code_block(&test_code).unwrap_or(test_code);
    Ok(code)
}

fn build_context(symbols: &[CodeSymbol]) -> String {
    if symbols.is_empty() {
        return String::from("No codebase context available.");
    }

    let mut context = String::from("Codebase symbols:\n");
    for sym in symbols.iter().take(30) {
        context.push_str(&format!("- {} {} (in {})\n", sym.kind, sym.name, sym.file_path));
    }
    context
}

fn build_prompt(requirement: &Requirement, framework: &str, context: &str) -> String {
    let framework_info = match framework {
        "jest" => "Jest (JavaScript/TypeScript testing framework). Use describe/it/expect syntax.",
        "pytest" => "pytest (Python testing framework). Use class/def test_ patterns with assert statements.",
        _ => "Unknown framework",
    };

    format!(
        r#"Generate a test for the following requirement. Output ONLY the test code, no explanations.

Requirement: {}
Section: {}
Type: {}
Priority: {}

Test framework: {}

{}

Generate a comprehensive test that:
1. Has clear arrange/act/assert structure
2. Includes meaningful assertions (not just placeholders)
3. Has a traceability comment linking to the requirement
4. Covers the main happy path and at least one edge case
5. Uses realistic mock data where needed"#,
        requirement.description,
        requirement.section,
        requirement.req_type,
        requirement.priority,
        framework_info,
        context
    )
}

fn extract_code_block(text: &str) -> Option<String> {
    // Try to find ```typescript, ```javascript, ```python, or generic ``` blocks
    let patterns = ["```typescript", "```javascript", "```python", "```js", "```ts", "```py", "```"];
    for pattern in patterns {
        if let Some(start) = text.find(pattern) {
            let code_start = start + pattern.len();
            // Skip to next line
            let code_start = text[code_start..].find('\n').map(|i| code_start + i + 1)?;
            let code_end = text[code_start..].find("```").map(|i| code_start + i)?;
            return Some(text[code_start..code_end].trim().to_string());
        }
    }
    None
}
