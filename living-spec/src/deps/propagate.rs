use super::model::DepGraph;
use crate::core::badge::{Badge, LineStatus, Reason};
use std::collections::{BTreeMap, HashMap, VecDeque};

pub struct DependencyQualification;
impl crate::core::providers::QualifiedProvider for DependencyQualification {
    fn qualify(
        &self,
        spec: &crate::core::spec::Spec,
        annotations: &crate::core::annotations::Annotations,
        statuses: &mut [LineStatus],
    ) -> crate::core::providers::Result<()> {
        let graph = DepGraph::new(spec, annotations)?;
        apply_qualified(statuses, &graph);
        Ok(())
    }
}

/// For every node, records its nearest non-proven reachable prerequisite.
/// Distances are shared across all sources; the parent pointers are selected
/// afterward in sorted-neighbor order to preserve the lexicographic path tie-break.
struct NearestPrerequisites {
    distance: HashMap<String, usize>,
    next: HashMap<String, String>,
    badge: HashMap<String, Badge>,
}

impl NearestPrerequisites {
    fn new(
        graph: &DepGraph,
        statuses: &BTreeMap<String, Badge>,
        excluded_terminal: Option<&str>,
    ) -> Self {
        let forward: HashMap<&str, &[String]> = graph
            .edges
            .iter()
            .map(|(id, targets)| (id.as_str(), targets.as_slice()))
            .collect();
        let mut reverse: HashMap<&str, Vec<&str>> =
            forward.keys().map(|&id| (id, Vec::new())).collect();
        for (&source, targets) in &forward {
            for target in *targets {
                reverse.entry(target.as_str()).or_default().push(source);
            }
        }
        let mut distance = HashMap::new();
        let mut badge = HashMap::new();
        let mut queue = VecDeque::new();
        for (id, &state) in statuses {
            if state != Badge::Proven
                && Some(id.as_str()) != excluded_terminal
                && forward.contains_key(id.as_str())
            {
                distance.insert(id.clone(), 0);
                badge.insert(id.clone(), state);
                queue.push_back(id.as_str());
            }
        }
        while let Some(target) = queue.pop_front() {
            let next_distance = distance[target] + 1;
            for &source in reverse.get(target).into_iter().flatten() {
                if !distance.contains_key(source) {
                    distance.insert(source.to_owned(), next_distance);
                    queue.push_back(source);
                }
            }
        }

        let mut next = HashMap::new();
        for (id, &depth) in &distance {
            if depth == 0 {
                continue;
            }
            if let Some(parent) = forward[id.as_str()]
                .iter()
                .find(|candidate| distance.get(candidate.as_str()) == Some(&(depth - 1)))
            {
                next.insert(id.clone(), parent.clone());
            }
        }

        Self {
            distance,
            next,
            badge,
        }
    }

    fn from(&self, id: &str) -> Option<(Vec<String>, Badge)> {
        self.distance.get(id)?;
        let mut path = vec![id.to_owned()];
        let mut current = id;
        while let Some(parent) = self.next.get(current) {
            path.push(parent.clone());
            current = parent;
        }
        let badge = *self.badge.get(current)?;
        Some((path, badge))
    }
}

pub fn nearest_non_proven(
    id: &str,
    graph: &DepGraph,
    statuses: &BTreeMap<String, Badge>,
) -> Option<(Vec<String>, Badge)> {
    NearestPrerequisites::new(graph, statuses, Some(id)).from(id)
}

pub fn apply_qualified(lines: &mut [LineStatus], graph: &DepGraph) {
    let mut statuses: BTreeMap<_, _> = lines.iter().map(|l| (l.id.clone(), l.badge)).collect();
    let cycles = graph.cycle_members();
    for id in cycles.keys() {
        if statuses.get(id) == Some(&Badge::Proven) {
            statuses.insert(id.clone(), Badge::Qualified);
        }
    }
    let nearest = NearestPrerequisites::new(graph, &statuses, None);
    for line in lines.iter_mut().filter(|l| l.badge == Badge::Proven) {
        if let Some(path) = cycles.get(&line.id) {
            line.badge = Badge::Qualified;
            line.reasons.push(Reason::DependencyCycle {
                path: std::sync::Arc::clone(path),
            });
        } else if let Some((path, badge)) = nearest.from(&line.id) {
            line.badge = Badge::Qualified;
            line.reasons.push(match badge {
                Badge::Broken => Reason::PrerequisiteBroken { path },
                Badge::Unproven => Reason::PrerequisiteUnproven { path },
                Badge::Qualified => Reason::PrerequisiteQualified { path },
                Badge::Proven => unreachable!(),
            });
        }
    }
}
