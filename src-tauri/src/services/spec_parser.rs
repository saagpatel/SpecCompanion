use crate::errors::AppError;
use crate::models::spec::Requirement;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const MAX_SPEC_BYTES: usize = 2 * 1024 * 1024;

pub fn parse_spec(spec_id: &str, content: &str) -> Result<Vec<Requirement>, AppError> {
    if spec_id.trim().is_empty() {
        return Err(AppError::InvalidInput("Spec ID cannot be empty".into()));
    }
    if content.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Spec content cannot be empty".into(),
        ));
    }
    if content.len() > MAX_SPEC_BYTES {
        return Err(AppError::InvalidInput(
            "Spec exceeds the 2 MB parsing limit".into(),
        ));
    }
    validate_fences(content)?;

    let parser = Parser::new(content).into_offset_iter();
    let mut requirements = Vec::new();
    let mut current_section = String::from("General");
    let mut in_heading = false;
    let mut heading_text = String::new();
    let mut in_list_item = false;
    let mut list_item_text = String::new();
    let mut item_start = 0usize;
    let mut item_end = 0usize;
    let mut is_requirement_section = false;
    let mut duplicate_counts: HashMap<(String, String), usize> = HashMap::new();

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = true;
                heading_text.clear();
                if matches!(
                    level,
                    HeadingLevel::H1 | HeadingLevel::H2 | HeadingLevel::H3
                ) {}
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                current_section = normalize_display_text(&heading_text);
                is_requirement_section = is_requirement_like_section(&current_section);
            }
            Event::Start(Tag::Item) => {
                in_list_item = true;
                list_item_text.clear();
                item_start = range.start;
                item_end = range.end;
            }
            Event::End(TagEnd::Item) => {
                in_list_item = false;
                item_end = item_end.max(range.end);
                let text = normalize_display_text(&list_item_text);
                if !text.is_empty() && (is_requirement_section || looks_like_requirement(&text)) {
                    let normalized_section = normalize_identity_text(&current_section);
                    let normalized_description = normalize_identity_text(&text);
                    let duplicate_key =
                        (normalized_section.clone(), normalized_description.clone());
                    let occurrence = duplicate_counts.entry(duplicate_key).or_insert(0);
                    let ordinal = *occurrence;
                    *occurrence += 1;
                    let fingerprint = stable_hash(&format!(
                        "{}\0{}\0{}",
                        normalized_section, normalized_description, ordinal
                    ));
                    requirements.push(Requirement {
                        id: format!(
                            "req_{}",
                            &stable_hash(&format!("{}\0{}", spec_id, fingerprint))[..24]
                        ),
                        spec_id: spec_id.to_string(),
                        section: current_section.clone(),
                        description: text.clone(),
                        req_type: classify_requirement_type(&current_section, &text),
                        priority: classify_priority(&text),
                        content_fingerprint: fingerprint,
                        source_line_start: line_number(content, item_start),
                        source_line_end: line_number(content, item_end.saturating_sub(1)),
                    });
                }
            }
            Event::Text(text) | Event::Code(text) => {
                if in_heading {
                    heading_text.push_str(&text);
                } else if in_list_item {
                    if !list_item_text.is_empty() {
                        list_item_text.push(' ');
                    }
                    list_item_text.push_str(&text);
                    item_end = item_end.max(range.end);
                }
            }
            _ => {}
        }
    }

    Ok(requirements)
}

fn validate_fences(content: &str) -> Result<(), AppError> {
    let mut active: Option<char> = None;
    for line in content.lines() {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("```") {
            Some('`')
        } else if trimmed.starts_with("~~~") {
            Some('~')
        } else {
            None
        };
        if let Some(marker) = marker {
            match active {
                None => active = Some(marker),
                Some(current) if current == marker => active = None,
                _ => {}
            }
        }
    }
    if active.is_some() {
        return Err(AppError::InvalidInput(
            "Malformed spec: unclosed fenced code block".into(),
        ));
    }
    Ok(())
}

fn line_number(content: &str, offset: usize) -> i64 {
    content.as_bytes()[..offset.min(content.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count() as i64
        + 1
}

fn stable_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn normalize_display_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_identity_text(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_requirement_like_section(section: &str) -> bool {
    let lower = section.to_lowercase();
    lower.contains("requirement")
        || lower.contains("user stor")
        || lower.contains("feature")
        || lower.contains("functional")
        || lower.contains("specification")
        || lower.contains("capability")
        || lower.contains("constraint")
        || lower.contains("acceptance criteria")
        || lower.contains("use case")
}

fn looks_like_requirement(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.starts_with("as a ")
        || lower.starts_with("the system shall ")
        || lower.starts_with("the system must ")
        || lower.starts_with("the application shall ")
        || lower.starts_with("the application must ")
        || lower.starts_with("shall ")
        || lower.starts_with("must ")
        || lower.contains("**shall**")
        || lower.contains("**must**")
        || (text.starts_with("**") && text.contains(' ') && lower.split_whitespace().count() >= 5)
}

fn classify_requirement_type(section: &str, text: &str) -> String {
    let lower_section = section.to_lowercase();
    let lower_text = text.to_lowercase();
    if lower_section.contains("non-functional")
        || lower_section.contains("performance")
        || lower_section.contains("security")
        || lower_section.contains("scalability")
        || lower_text.contains("performance")
        || lower_text.contains("latency")
        || lower_text.contains("availability")
    {
        "non_functional".to_string()
    } else if lower_section.contains("constraint")
        || lower_text.contains("constraint")
        || lower_text.contains("limitation")
    {
        "constraint".to_string()
    } else {
        "functional".to_string()
    }
}

fn classify_priority(text: &str) -> String {
    let lower = text.to_lowercase();
    if lower.contains("critical") || lower.contains("must have") || lower.contains("**must**") {
        "high".to_string()
    } else if lower.contains("nice to have")
        || lower.contains("optional")
        || lower.contains("could")
    {
        "low".to_string()
    } else {
        "medium".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_unique_ids_and_source_locations() {
        let content = "# Demo\n\n## Requirements\n\n- The system shall add numbers\n- The system shall add numbers\n";
        let first = parse_spec("spec-1", content).expect("parse");
        let second = parse_spec("spec-1", content).expect("parse again");
        assert_eq!(first[0].id, second[0].id);
        assert_ne!(first[0].id, first[1].id);
        assert_eq!(first[0].source_line_start, 5);
        assert_eq!(first[1].source_line_start, 6);

        let edited = parse_spec(
            "spec-1",
            &content.replace("add numbers", "subtract numbers"),
        )
        .expect("parse edited");
        assert_ne!(first[0].id, edited[0].id);
    }

    #[test]
    fn malformed_fence_is_rejected() {
        let err = parse_spec("spec-1", "## Requirements\n```\n- The system shall hide")
            .expect_err("unclosed fence must fail");
        assert!(err.to_string().contains("unclosed fenced code block"));
    }
}
