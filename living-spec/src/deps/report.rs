use super::{model::DepGraph, propagate::nearest_non_proven};
use crate::core::{
    badge::{Badge, LineStatus, Reason},
    test_identity::TestStatus,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

#[derive(Debug, Clone, Serialize)]
pub struct Prerequisite {
    pub id: String,
    pub text: String,
    pub badge: Badge,
    /// Only intrinsic reasons: dependency paths live on the affected claim.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<Reason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failing_test: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failing_test_status: Option<TestStatus>,
}

#[derive(Debug, Serialize)]
pub struct ImpactReport {
    pub dependent_id: String,
    pub dependent_text: String,
    pub path: Vec<String>,
    /// Reachable non-proven claims, excluding self and the selected prerequisite.
    pub other_nonproven_count: usize,
    /// Start traversal here, excluding the dependent itself from the members.
    pub prerequisite_group: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cycles: Vec<usize>,
}

#[derive(Debug, Serialize)]
pub struct PrerequisiteGroup {
    pub id: usize,
    pub members: Vec<Prerequisite>,
    pub edges: Vec<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct DependencyReport {
    pub claims: Vec<ImpactReport>,
    pub groups: Vec<PrerequisiteGroup>,
    pub cycles: Vec<Vec<String>>,
}

pub fn impacts(graph: &DepGraph, statuses: &[LineStatus]) -> DependencyReport {
    let by_id: BTreeMap<_, _> = statuses.iter().map(|s| (s.id.as_str(), s)).collect();
    let badges = statuses.iter().map(|s| (s.id.clone(), s.badge)).collect();
    let mut components = graph.strongly_connected_components();
    components.sort();
    let group_by_id: BTreeMap<_, _> = components
        .iter()
        .enumerate()
        .flat_map(|(group, members)| members.iter().map(move |id| (id.as_str(), group)))
        .collect();
    let edges: Vec<Vec<usize>> = components
        .iter()
        .enumerate()
        .map(|(group, members)| {
            members
                .iter()
                .flat_map(|id| &graph.edges[id])
                .map(|id| group_by_id[id.as_str()])
                .filter(|target| *target != group)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        })
        .collect();
    let mut included = BTreeSet::new();
    let mut claims = Vec::new();
    for line in statuses.iter().filter(|s| s.badge == Badge::Qualified) {
        let group = group_by_id[line.id.as_str()];
        // Count with a temporary visited set; never retain expanded reachability.
        // Shared edges preserve diamonds without counting members twice.
        let mut reachable = BTreeSet::new();
        let mut todo = vec![group];
        let mut count = 0usize;
        while let Some(next) = todo.pop() {
            if !reachable.insert(next) {
                continue;
            }
            count += components[next]
                .iter()
                .filter(|id| {
                    by_id
                        .get(id.as_str())
                        .is_some_and(|s| s.badge != Badge::Proven)
                })
                .count();
            todo.extend(edges[next].iter().copied());
        }
        included.extend(reachable);
        let path = nearest_non_proven(&line.id, graph, &badges)
            .map(|(path, _)| path)
            .unwrap_or_default();
        claims.push(ImpactReport {
            dependent_id: line.id.clone(),
            dependent_text: graph.texts.get(&line.id).cloned().unwrap_or_default(),
            other_nonproven_count: count.saturating_sub(1 + usize::from(path.len() > 1)),
            path,
            prerequisite_group: group,
            cycles: Vec::new(),
        });
    }
    let mut cycles = Vec::new();
    let groups: Vec<_> = included
        .into_iter()
        .map(|id| {
            let cycle = (components[id].len() > 1).then(|| {
                let index = cycles.len();
                cycles.push(graph.cycle_witness(&components[id]));
                index
            });
            PrerequisiteGroup {
                id,
                members: components[id]
                    .iter()
                    .filter_map(|id| by_id.get(id.as_str()))
                    .filter(|s| s.badge != Badge::Proven)
                    .map(|s| prerequisite(graph, s))
                    .collect(),
                edges: edges[id].clone(),
                cycle,
            }
        })
        .collect();
    let cycle_by_group: BTreeMap<_, _> = groups.iter().map(|g| (g.id, g.cycle)).collect();
    for claim in &mut claims {
        claim
            .cycles
            .extend(cycle_by_group[&claim.prerequisite_group]);
    }
    DependencyReport {
        claims,
        groups,
        cycles,
    }
}

pub fn markdown(report: &DependencyReport) -> String {
    let mut output = Vec::new();
    write_markdown(&mut output, report).expect("writing to a vector cannot fail");
    String::from_utf8(output).expect("markdown is UTF-8")
}

pub fn write_markdown(mut output: impl Write, report: &DependencyReport) -> io::Result<()> {
    write!(output, "# Dependency impact report\n\nNon-proven prerequisites qualify dependents, including unproven and qualified prerequisites. Test outcomes are unchanged.\n\nTraverse group edges for all prerequisites; exclude the affected claim itself. Other counts exclude the selected prerequisite too.\n\n")?;
    let members: BTreeMap<_, _> = report
        .groups
        .iter()
        .flat_map(|g| &g.members)
        .map(|p| (p.id.as_str(), p))
        .collect();
    for r in &report.claims {
        write!(
            output,
            "## {} — {}\n\n{}",
            r.dependent_id,
            r.dependent_text,
            r.path.join(" → ")
        )?;
        if let Some(selected) = r.path.last().and_then(|id| members.get(id.as_str())) {
            write!(output, " ({})", describe(selected))?;
        }
        writeln!(
            output,
            "\nGroup {}; other non-proven: {}.",
            r.prerequisite_group, r.other_nonproven_count
        )?;
        for cycle in &r.cycles {
            writeln!(output, "Cycle {cycle}.")?;
        }
        writeln!(output)?;
    }
    writeln!(output, "# Groups\n")?;
    for group in &report.groups {
        writeln!(output, "## Group {}\n", group.id)?;
        for p in &group.members {
            render_prerequisite(&mut output, p)?;
        }
        if !group.edges.is_empty() {
            writeln!(output, "Prerequisite groups: {:?}.", group.edges)?;
        }
        if let Some(cycle) = group.cycle {
            writeln!(output, "Cycle {cycle}.")?;
        }
        writeln!(output)?;
    }
    writeln!(output, "# Cycles\n")?;
    for (index, cycle) in report.cycles.iter().enumerate() {
        writeln!(output, "- Cycle {index}: {}", cycle.join(" → "))?;
    }
    Ok(())
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
        reasons: status
            .reasons
            .iter()
            .filter(|reason| {
                !matches!(
                    reason,
                    Reason::PrerequisiteBroken { .. }
                        | Reason::PrerequisiteUnproven { .. }
                        | Reason::PrerequisiteQualified { .. }
                        | Reason::DependencyCycle { .. }
                )
            })
            .cloned()
            .collect(),
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

fn render_prerequisite(output: &mut impl Write, prerequisite: &Prerequisite) -> io::Result<()> {
    let label = match prerequisite.badge {
        Badge::Proven => "Proven",
        Badge::Broken => "Broken",
        Badge::Unproven => "Unproven",
        Badge::Qualified => "Qualified",
    };
    writeln!(
        output,
        "- {label}: {} — {} ({})",
        prerequisite.id,
        prerequisite.text,
        describe(prerequisite)
    )?;
    if let Some(reference) = &prerequisite.boundary_reference {
        writeln!(output, "  Boundary declarations: `{reference}`. Case execution details are not included in this dependency export.")?;
    }
    Ok(())
}
