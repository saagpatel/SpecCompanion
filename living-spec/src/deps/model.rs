use crate::core::{annotations::Annotations, spec::Spec};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug, Clone)]
pub struct DepGraph {
    pub edges: BTreeMap<String, Vec<String>>,
    pub texts: BTreeMap<String, String>,
}
impl DepGraph {
    pub fn new(spec: &Spec, annotations: &Annotations) -> Result<Self, String> {
        let texts: BTreeMap<_, _> = spec
            .lines
            .iter()
            .map(|l| (l.id.clone(), l.text.clone()))
            .collect();
        let mut edges: BTreeMap<String, Vec<String>> =
            texts.keys().map(|id| (id.clone(), Vec::new())).collect();
        for (id, annotation) in &annotations.requirements {
            if !texts.contains_key(id) {
                if !annotation.depends_on.is_empty() {
                    return Err(format!("unknown dependent {id}"));
                }
                continue;
            }
            for dep in &annotation.depends_on {
                if dep.evidence.trim().is_empty() {
                    return Err(format!("missing evidence for {id} -> {}", dep.id));
                }
                if !texts.contains_key(&dep.id) {
                    return Err(format!("unknown prerequisite {}", dep.id));
                }
                if id == &dep.id {
                    return Err(format!("self-loop {id}"));
                }
                edges.get_mut(id).expect("known id").push(dep.id.clone());
            }
        }
        for values in edges.values_mut() {
            values.sort();
            values.dedup();
        }
        Ok(Self { edges, texts })
    }
    /// One deterministic concrete cycle per nontrivial strongly connected component.
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles: Vec<_> = self
            .strongly_connected_components()
            .into_iter()
            .filter(|component| component.len() > 1)
            .map(|component| self.cycle_witness(&component))
            .collect();
        cycles.sort();
        cycles
    }

    /// Each member points to one shared witness for its strongly connected component.
    /// Sharing keeps qualification linear in graph size even when a cycle is long.
    pub fn cycle_members(&self) -> BTreeMap<String, Arc<Vec<String>>> {
        let mut result = BTreeMap::new();
        for component in self.strongly_connected_components() {
            if component.len() < 2 {
                continue;
            }
            let witness = Arc::new(self.cycle_witness(&component));
            for id in component {
                result.insert(id, Arc::clone(&witness));
            }
        }
        result
    }

    fn cycle_witness(&self, component: &[String]) -> Vec<String> {
        let start = &component[0];
        let allowed: BTreeSet<_> = component.iter().map(String::as_str).collect();
        let mut path = vec![start.clone()];
        let mut visited = BTreeSet::from([start.clone()]);
        let mut frames = vec![(start.clone(), 0usize)];
        while let Some((id, next_index)) = frames.last_mut() {
            let neighbors = &self.edges[id];
            if *next_index == neighbors.len() {
                frames.pop();
                path.pop();
                continue;
            }
            let next = &neighbors[*next_index];
            *next_index += 1;
            if !allowed.contains(next.as_str()) {
                continue;
            }
            if next == start {
                path.push(next.clone());
                return path;
            }
            if visited.insert(next.clone()) {
                path.push(next.clone());
                frames.push((next.clone(), 0));
            }
        }
        unreachable!("a nontrivial strongly connected component has a cycle")
    }

    fn strongly_connected_components(&self) -> Vec<Vec<String>> {
        // Iterative Kosaraju traversal keeps graph depth bounded by heap memory,
        // rather than the process stack. Adjacency lists are sorted at load time,
        // and BTreeMap iteration keeps both passes deterministic.
        let mut reverse: BTreeMap<String, Vec<String>> = self
            .edges
            .keys()
            .map(|id| (id.clone(), Vec::new()))
            .collect();
        for (from, targets) in &self.edges {
            for target in targets {
                reverse
                    .entry(target.clone())
                    .or_default()
                    .push(from.clone());
            }
        }
        for sources in reverse.values_mut() {
            sources.sort();
            sources.dedup();
        }

        let mut seen = BTreeSet::new();
        let mut finished = Vec::with_capacity(self.edges.len());
        for root in self.edges.keys() {
            if !seen.insert(root.clone()) {
                continue;
            }
            let mut frames = vec![(root.clone(), 0usize)];
            while let Some((id, next_index)) = frames.last_mut() {
                let neighbors = &self.edges[id];
                if *next_index < neighbors.len() {
                    let next = neighbors[*next_index].clone();
                    *next_index += 1;
                    if seen.insert(next.clone()) {
                        frames.push((next, 0));
                    }
                } else {
                    let (id, _) = frames.pop().expect("frame exists");
                    finished.push(id);
                }
            }
        }

        seen.clear();
        let mut components = Vec::new();
        for root in finished.into_iter().rev() {
            if !seen.insert(root.clone()) {
                continue;
            }
            let mut component = Vec::new();
            let mut todo = vec![root];
            while let Some(id) = todo.pop() {
                component.push(id.clone());
                for next in reverse.get(&id).into_iter().flatten().rev() {
                    if seen.insert(next.clone()) {
                        todo.push(next.clone());
                    }
                }
            }
            if component.len() > 1 {
                component.sort();
                components.push(component);
            }
        }
        components
    }
}
