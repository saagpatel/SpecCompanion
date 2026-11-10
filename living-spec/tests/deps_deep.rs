use living_spec::{
    core::{
        annotations::{Annotations, Dependency, RequirementAnnotations},
        badge::{Badge, LineStatus, Reason},
        spec::Spec,
    },
    deps::{model::DepGraph, propagate::apply_qualified},
};
use std::collections::BTreeMap;

fn graph_with_chain(size: usize, close_cycle: bool) -> DepGraph {
    let mut markdown = String::new();
    let mut requirements = BTreeMap::new();
    for index in 0..size {
        let id = format!("R-{index:05}");
        markdown.push_str(&format!("- [{id}] Synthetic item.\n"));
        let mut annotation = RequirementAnnotations::default();
        if index + 1 < size {
            annotation.depends_on.push(Dependency {
                id: format!("R-{:05}", index + 1),
                evidence: "authored".into(),
                extra: Default::default(),
            });
        } else if close_cycle {
            annotation.depends_on.push(Dependency {
                id: "R-00000".into(),
                evidence: "authored".into(),
                extra: Default::default(),
            });
        }
        requirements.insert(id, annotation);
    }
    let spec = Spec::parse(&markdown).expect("synthetic requirements parse");
    let annotations = Annotations {
        schema: "living-spec.annotations/v1".into(),
        requirements,
        extra: BTreeMap::new(),
    };
    DepGraph::new(&spec, &annotations).expect("authored dependency graph loads")
}

fn assert_concrete_cycle(graph: &DepGraph, cycle: &[String]) {
    assert!(cycle.len() > 1);
    assert_eq!(cycle.first(), cycle.last());
    assert!(cycle
        .windows(2)
        .all(|edge| graph.edges[&edge[0]].contains(&edge[1])));
}

#[test]
fn finding_2_deep_dependency_graph_does_not_overflow() {
    let chain = graph_with_chain(50_000, false);
    assert!(chain.detect_cycles().is_empty());
    assert!(chain.cycle_members().is_empty());

    // A long cycle also exercises iterative concrete cycle-path construction.
    let cycle_graph = graph_with_chain(50_000, true);
    let cycle = cycle_graph.detect_cycles();
    assert_eq!(cycle.len(), 1);
    assert_eq!(cycle[0].len(), 50_001);
    assert_eq!(cycle[0].first().map(String::as_str), Some("R-00000"));
    assert_eq!(cycle[0].last().map(String::as_str), Some("R-00000"));
    assert_concrete_cycle(&cycle_graph, &cycle[0]);
}

#[test]
fn finding_07_deps_2_dense_scc_cycle_witness_search_is_bounded() {
    let mut edges = BTreeMap::new();
    let root = "R-00000".to_owned();
    let entry = "R-00001".to_owned();
    let sink = "R-99999".to_owned();
    edges.insert(root.clone(), vec![entry.clone()]);
    let mut entry_edges = vec![sink.clone()];
    let layers: Vec<Vec<String>> = (0..8)
        .map(|layer| {
            (0..6)
                .map(|member| format!("R-{:02}{member:02}", layer + 1))
                .collect()
        })
        .collect();
    entry_edges.extend(layers[0].iter().cloned());
    entry_edges.sort();
    edges.insert(entry.clone(), entry_edges);
    for (layer_index, layer) in layers.iter().enumerate() {
        for id in layer {
            let mut neighbors = vec![entry.clone()];
            if let Some(next_layer) = layers.get(layer_index + 1) {
                neighbors.extend(next_layer.iter().cloned());
            }
            neighbors.sort();
            edges.insert(id.clone(), neighbors);
        }
    }
    edges.insert(sink.clone(), vec![root.clone()]);
    let texts = edges
        .keys()
        .map(|id| (id.clone(), "Synthetic".into()))
        .collect();
    let graph = DepGraph { edges, texts };

    let cycles = graph.detect_cycles();
    assert_eq!(cycles.len(), 1);
    assert_concrete_cycle(&graph, &cycles[0]);
    assert_eq!(cycles[0][0], root);
}

#[test]
fn finding_07_deps_3_large_cycle_qualification_shares_its_witness() {
    let graph = graph_with_chain(50_000, true);
    let members = graph.cycle_members();
    assert_eq!(members.len(), 50_000);
    assert!(std::sync::Arc::ptr_eq(
        &members["R-00000"],
        &members["R-49999"]
    ));

    let mut statuses: Vec<_> = graph
        .edges
        .keys()
        .map(|id| LineStatus {
            id: id.clone(),
            badge: Badge::Proven,
            reasons: vec![],
            tests: vec![],
            mutation: None,
        })
        .collect();
    apply_qualified(&mut statuses, &graph);
    assert!(statuses
        .iter()
        .all(|status| status.badge == Badge::Qualified));
    let Reason::DependencyCycle { path } = &statuses[0].reasons[0] else {
        panic!("cycle member has a concrete cycle reason");
    };
    assert_eq!(path.len(), 50_001);
    assert_concrete_cycle(&graph, path);
    assert!(statuses.iter().all(|status| matches!(
        &status.reasons[0],
        Reason::DependencyCycle { path: other } if std::sync::Arc::ptr_eq(path, other)
    )));
}
