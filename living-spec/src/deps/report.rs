use super::model::DepGraph;
use crate::core::{
    badge::{Badge, LineStatus, Reason},
    test_identity::TestStatus,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize)]
pub struct Prerequisite {
    pub id: String,
    pub text: String,
    pub badge: Badge,
    pub reasons: Vec<Reason>,
    pub boundary_reference: Option<String>,
    pub failing_test: Option<String>,
    pub failing_test_status: Option<TestStatus>,
}
#[derive(Debug, Serialize)]
pub struct ImpactReport {
    pub dependent_id: String,
    pub dependent_text: String,
    pub path: Vec<String>,
    pub explanation: String,
    pub selected_prerequisite: Option<Prerequisite>,
    pub broken_prerequisites: Vec<Prerequisite>,
    pub unproven_prerequisites: Vec<Prerequisite>,
    pub qualified_prerequisites: Vec<Prerequisite>,
}

pub fn impacts(graph: &DepGraph, statuses: &[LineStatus]) -> Vec<ImpactReport> {
    let by_id: BTreeMap<_, _> = statuses.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut result = Vec::new();
    for line in statuses.iter().filter(|s| s.badge == Badge::Qualified) {
        let path = line
            .reasons
            .iter()
            .find_map(|r| match r {
                Reason::PrerequisiteBroken { path }
                | Reason::PrerequisiteUnproven { path }
                | Reason::PrerequisiteQualified { path } => Some(path.clone()),
                Reason::DependencyCycle { path } => Some(path.as_ref().clone()),
                _ => None,
            })
            .unwrap_or_default();
        let mut seen = BTreeSet::from([line.id.clone()]);
        let mut todo = graph.edges.get(&line.id).cloned().unwrap_or_default();
        let mut broken = Vec::new();
        let mut unproven = Vec::new();
        let mut qualified = Vec::new();
        while let Some(id) = todo.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(s) = by_id.get(id.as_str()) {
                match s.badge {
                    Badge::Broken => broken.push(prerequisite(graph, s)),
                    Badge::Unproven => unproven.push(prerequisite(graph, s)),
                    Badge::Qualified => qualified.push(prerequisite(graph, s)),
                    Badge::Proven => {}
                }
            }
            if let Some(edges) = graph.edges.get(&id) {
                todo.extend(edges.iter().cloned());
            }
        }
        broken.sort_by(|a, b| a.id.cmp(&b.id));
        unproven.sort_by(|a, b| a.id.cmp(&b.id));
        qualified.sort_by(|a, b| a.id.cmp(&b.id));
        let selected = path
            .last()
            .filter(|id| *id != &line.id)
            .and_then(|id| by_id.get(id.as_str()))
            .map(|status| prerequisite(graph, status));
        let mut explanation = path.join(" → ");
        if let Some(prerequisite) = &selected {
            explanation.push_str(&format!(" ({})", describe(prerequisite)));
        }
        result.push(ImpactReport {
            dependent_id: line.id.clone(),
            dependent_text: graph.texts.get(&line.id).cloned().unwrap_or_default(),
            path,
            explanation,
            selected_prerequisite: selected,
            broken_prerequisites: broken,
            unproven_prerequisites: unproven,
            qualified_prerequisites: qualified,
        });
    }
    result
}
pub fn markdown(reports: &[ImpactReport]) -> String {
    let mut output = String::from("# Dependency impact report\n\nNon-proven prerequisites qualify dependents, including unproven and qualified prerequisites. Test outcomes are unchanged.\n\n");
    for r in reports {
        output.push_str(&format!(
            "## {} — {}\n\n{}\n\n",
            r.dependent_id, r.dependent_text, r.explanation
        ));
        if let Some(prerequisite) = &r.selected_prerequisite {
            render_prerequisite(&mut output, "Selected prerequisite", prerequisite);
        }
        for b in &r.broken_prerequisites {
            render_prerequisite(&mut output, "Broken", b);
        }
        for prerequisite in &r.unproven_prerequisites {
            render_prerequisite(&mut output, "Unproven", prerequisite);
        }
        for prerequisite in &r.qualified_prerequisites {
            render_prerequisite(&mut output, "Qualified", prerequisite);
        }
        output.push('\n');
    }
    output
}

fn prerequisite(graph: &DepGraph, status: &LineStatus) -> Prerequisite {
    let failure = status.tests.iter().find(|test| {
        status.reasons.contains(&Reason::TestFailed)
            && test.admissible
            && matches!(test.status, TestStatus::Failed | TestStatus::Errored)
    });
    Prerequisite {
        id: status.id.clone(),
        text: graph.texts.get(&status.id).cloned().unwrap_or_default(),
        badge: status.badge,
        reasons: status.reasons.clone(),
        boundary_reference: status
            .reasons
            .contains(&Reason::BoundaryGap)
            .then(|| format!("annotations.json#/requirements/{}/bounds", status.id)),
        failing_test: failure.map(|test| test.test_id.clone()),
        failing_test_status: failure.map(|test| test.status),
    }
}

fn describe(prerequisite: &Prerequisite) -> String {
    let badge = match prerequisite.badge {
        Badge::Proven => "proven",
        Badge::Broken => "broken",
        Badge::Unproven => "unproven",
        Badge::Qualified => "qualified",
    };
    let mut details = Vec::new();
    // Reasons explain the badge; diagnostic failures cannot supply a cause.
    for reason in &prerequisite.reasons {
        if *reason == Reason::TestFailed {
            if let Some(test) = &prerequisite.failing_test {
                let verb = if prerequisite.failing_test_status == Some(TestStatus::Errored) {
                    "errored"
                } else {
                    "failed"
                };
                details.push(format!("test {test} {verb}"));
                continue;
            }
        }
        let value = serde_json::to_value(reason).expect("reason is serializable");
        details.push(match value {
            serde_json::Value::String(text) => text,
            other => other.to_string(),
        });
    }
    if details.is_empty() {
        badge.to_owned()
    } else {
        format!("{badge}: {}", details.join(", "))
    }
}

fn render_prerequisite(output: &mut String, label: &str, prerequisite: &Prerequisite) {
    output.push_str(&format!(
        "- {label}: {} — {} ({})\n",
        prerequisite.id,
        prerequisite.text,
        describe(prerequisite)
    ));
    if let Some(reference) = &prerequisite.boundary_reference {
        output.push_str(&format!(
            "  Boundary declarations: `{reference}`. Case execution details are not included in this dependency export.\n"
        ));
    }
}
