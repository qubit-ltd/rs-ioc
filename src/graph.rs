// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// qubit-style: allow multiple-public-types
//! Resolves active bindings and validates their dependency graph before
//! construction.

use std::any::TypeId;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

use crate::binding::PendingBindingKind;
use crate::binding::PendingDefinition;
use crate::dependency::Dependency;
use crate::dependency::DependencyCardinality;
use crate::error::BuildError;
use crate::key::BindingKey;

mod diagnostics;

pub(crate) use diagnostics::DiagnosticPaths;
use diagnostics::PathOrigin;

mod internal;

pub(crate) use internal::binding_location::BindingLocation;
use internal::edge::Edge;
use internal::node::Node;
pub(crate) use internal::resolved_dependency::ResolvedDependency;
pub(crate) use internal::validated_graph::ValidatedGraph;

/// Keeps the first registered index for each key while root reachability is
/// still being established.
fn first_keys(nodes: &[Node]) -> HashMap<BindingKey, usize> {
    let mut result = HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        result.entry(node.key.clone()).or_insert(index);
    }
    result
}

/// Resolves each required root and returns the selected binding positions.
#[allow(clippy::result_large_err)]
fn select_roots(
    roots: &[Dependency],
    nodes: &[Node],
    by_type: &HashMap<TypeId, Vec<usize>>,
) -> Result<Vec<usize>, BuildError> {
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    for request in roots {
        let candidates = candidates_for(request, nodes, by_type);
        let available = by_type
            .get(&request.type_id())
            .into_iter()
            .flatten()
            .map(|&i| nodes[i].key.clone())
            .collect::<Vec<_>>();
        match select(request, &candidates, nodes) {
            Ok(indices) => {
                for index in indices {
                    if seen.insert(index) {
                        selected.push(index);
                    }
                }
            }
            Err(Edge::MissingDependency(_)) => {
                return Err(BuildError::MissingRoot {
                    request: request.clone(),
                    available,
                });
            }
            Err(Edge::AmbiguousDependency(_, candidates)) => {
                return Err(BuildError::AmbiguousRoot {
                    request: request.clone(),
                    candidates,
                });
            }
            _ => unreachable!(),
        }
    }
    Ok(selected)
}

/// Starts from ordered roots, closes dependencies and definition members, and
/// retains one deterministic predecessor for each selected binding.
#[allow(clippy::result_large_err)]
fn close_definitions(
    definitions: &[PendingDefinition],
    nodes: &[Node],
    edges: &[Vec<Edge>],
    seeds: &[usize],
) -> Result<(Vec<bool>, DiagnosticPaths), BuildError> {
    let definition_count = nodes
        .iter()
        .map(|node| node.location.definition)
        .max()
        .map_or(0, |index| index + 1);
    let mut nodes_by_definition = vec![Vec::new(); definition_count];
    for (index, node) in nodes.iter().enumerate() {
        nodes_by_definition[node.location.definition].push(index);
    }
    let mut diagnostics = DiagnosticPaths::new(definitions);
    let mut reachable = vec![false; nodes.len()];
    let mut queue = VecDeque::new();
    for &seed in seeds {
        if diagnostics.record_root(nodes[seed].location) {
            reachable[seed] = true;
            queue.push_back(seed);
        }
    }
    while let Some(index) = queue.pop_front() {
        let node = &nodes[index];
        for edge in &edges[index] {
            match edge {
                Edge::Target(target) => {
                    let target_location = nodes[*target].location;
                    if diagnostics.record_from(target_location, PathOrigin::Dependency(node.location)) {
                        reachable[*target] = true;
                        queue.push_back(*target);
                    }
                }
                Edge::MissingDependency(dependency) => {
                    return Err(BuildError::MissingDependency {
                        dependency: dependency.clone(),
                        definition: node.source,
                        path: diagnostics.path_to(node.location),
                    });
                }
                Edge::AmbiguousDependency(dependency, candidates) => {
                    return Err(BuildError::AmbiguousBinding {
                        dependency: dependency.clone(),
                        definition: node.source,
                        candidates: candidates.clone(),
                        path: diagnostics.path_to(node.location),
                    });
                }
                Edge::MissingAliasTarget(target) => {
                    return Err(BuildError::MissingAliasTarget {
                        alias: node.key.clone(),
                        target: target.clone(),
                        definition: node.source,
                        path: diagnostics.path_to(node.location),
                    });
                }
            }
        }
        for &member in &nodes_by_definition[node.location.definition] {
            let member_location = nodes[member].location;
            if diagnostics.record_from(member_location, PathOrigin::DefinitionMember(node.location)) {
                reachable[member] = true;
                queue.push_back(member);
            }
        }
    }
    Ok((reachable, diagnostics))
}

/// Selects unrooted graph components in registration order for `build_all`.
fn build_all_seeds(nodes: &[Node], edges: &[Vec<Edge>]) -> Vec<usize> {
    let mut has_incoming = vec![false; nodes.len()];
    for requests in edges {
        for edge in requests {
            if let Edge::Target(target) = edge {
                has_incoming[*target] = true;
            }
        }
    }
    let mut seeds = Vec::new();
    let mut selected = vec![false; nodes.len()];
    let definition_count = nodes
        .iter()
        .map(|node| node.location.definition)
        .max()
        .map_or(0, |value| value + 1);
    let mut members = vec![Vec::new(); definition_count];
    for (index, node) in nodes.iter().enumerate() {
        members[node.location.definition].push(index);
    }
    for (index, incoming) in has_incoming.iter().copied().enumerate() {
        if !incoming {
            seeds.push(index);
            mark_build_all_closure(index, nodes, edges, &members, &mut selected);
        }
    }
    let indices = (0..selected.len()).collect::<Vec<_>>();
    for index in indices {
        if !selected[index] {
            selected[index] = true;
            seeds.push(index);
            mark_build_all_closure(index, nodes, edges, &members, &mut selected);
        }
    }
    seeds
}

/// Marks one build-all component, including its dependency and
/// definition-member closure.
fn mark_build_all_closure(
    seed: usize,
    nodes: &[Node],
    edges: &[Vec<Edge>],
    members: &[Vec<usize>],
    selected: &mut [bool],
) {
    let mut queue = VecDeque::from([seed]);
    while let Some(index) = queue.pop_front() {
        for edge in &edges[index] {
            if let Edge::Target(target) = edge
                && !selected[*target]
            {
                selected[*target] = true;
                queue.push_back(*target);
            }
        }
        for &member in &members[nodes[index].location.definition] {
            if !selected[member] {
                selected[member] = true;
                queue.push_back(member);
            }
        }
    }
}

/// Flattens definitions in builder entry order and binding declaration order.
fn flatten(definitions: &[PendingDefinition]) -> Vec<Node> {
    let mut nodes = Vec::new();
    for (definition_index, definition) in definitions.iter().enumerate() {
        for (binding_index, binding) in definition.bindings.iter().enumerate() {
            nodes.push(Node {
                location: BindingLocation {
                    definition: definition_index,
                    binding: binding_index,
                },
                key: binding.key.clone(),
                source: definition.source,
                primary: binding.primary,
                order: binding.order,
            });
        }
    }
    nodes
}

/// Rejects the first exact key collision among active definitions.
// BuildError preserves both conflicting definitions for callers.
#[allow(clippy::result_large_err)]
fn exact_keys(nodes: &[Node]) -> Result<HashMap<BindingKey, usize>, BuildError> {
    let mut by_key = HashMap::with_capacity(nodes.len());
    for (index, node) in nodes.iter().enumerate() {
        if let Some(first_index) = by_key.insert(node.key.clone(), index) {
            return Err(BuildError::DuplicateBinding {
                key: node.key.clone(),
                first: nodes[first_index].source,
                second: node.source,
            });
        }
    }
    Ok(by_key)
}

/// Rejects multiple primaries in the same active Rust type namespace.
// BuildError preserves all conflicting primary sources for callers.
#[allow(clippy::result_large_err)]
fn validate_primary(nodes: &[Node]) -> Result<(), BuildError> {
    let mut seen = HashSet::new();
    for node in nodes {
        if node.primary && !seen.insert(node.key.type_id()) {
            let candidates = nodes
                .iter()
                .filter(|candidate| candidate.primary && candidate.key.type_id() == node.key.type_id())
                .map(|candidate| (candidate.key.clone(), candidate.source))
                .collect();
            return Err(BuildError::MultiplePrimaryBindings { candidates });
        }
    }
    Ok(())
}

/// Resolves each request, then creates binding and alias edges.
fn resolve_edges(
    definitions: &[PendingDefinition],
    nodes: &[Node],
    by_key: &HashMap<BindingKey, usize>,
    by_type: &HashMap<TypeId, Vec<usize>>,
) -> (Vec<Vec<Edge>>, Vec<Vec<ResolvedDependency>>) {
    let mut edges = Vec::with_capacity(nodes.len());
    let mut resolved = definitions
        .iter()
        .map(|definition| {
            definition
                .dependencies
                .iter()
                .cloned()
                .map(|request| ResolvedDependency {
                    request,
                    keys: Vec::new(),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for node in nodes {
        let binding = &definitions[node.location.definition].bindings[node.location.binding];
        if let PendingBindingKind::Alias { target, .. } = &binding.kind {
            edges.push(vec![match by_key.get(target) {
                Some(&target_index) => Edge::Target(target_index),
                None => Edge::MissingAliasTarget(target.clone()),
            }]);
            continue;
        }
        let mut node_edges = Vec::new();
        for (request_index, request) in definitions[node.location.definition].dependencies.iter().enumerate() {
            let candidates = candidates_for(request, nodes, by_type);
            match select(request, &candidates, nodes) {
                Ok(selected) => {
                    resolved[node.location.definition][request_index].keys =
                        selected.iter().map(|&index| nodes[index].key.clone()).collect();
                    node_edges.extend(selected.into_iter().map(Edge::Target));
                }
                Err(problem) => node_edges.push(problem),
            }
        }
        edges.push(node_edges);
    }
    (edges, resolved)
}

/// Gets candidate indices for an exact ID or for a Rust type namespace.
fn candidates_for(request: &Dependency, nodes: &[Node], by_type: &HashMap<TypeId, Vec<usize>>) -> Vec<usize> {
    let candidates = by_type.get(&request.type_id()).cloned().unwrap_or_default();
    match request.id() {
        Some(id) => candidates
            .into_iter()
            .filter(|&index| {
                nodes[index]
                    .key
                    .id()
                    .is_some_and(|candidate_id| candidate_id.as_str() == id)
            })
            .collect(),
        None => candidates,
    }
}

/// Applies cardinality, primary, and collection ordering to candidates.
#[allow(clippy::result_large_err)]
fn select(request: &Dependency, candidates: &[usize], nodes: &[Node]) -> Result<Vec<usize>, Edge> {
    if request.cardinality() == DependencyCardinality::All {
        let mut selected = candidates.to_vec();
        selected.sort_by(|&left, &right| {
            nodes[left]
                .order
                .cmp(&nodes[right].order)
                .then_with(|| nodes[left].key.id().cmp(&nodes[right].key.id()))
                .then_with(|| nodes[left].source.cmp(&nodes[right].source))
                .then_with(|| left.cmp(&right))
        });
        return Ok(selected);
    }
    match candidates {
        [] if request.cardinality() == DependencyCardinality::Optional => Ok(Vec::new()),
        [] => Err(Edge::MissingDependency(request.clone())),
        [one] => Ok(vec![*one]),
        many if request.id().is_some() => Err(Edge::AmbiguousDependency(
            request.clone(),
            many.iter().map(|&index| nodes[index].key.clone()).collect(),
        )),
        many => {
            let primary: Vec<_> = many.iter().copied().filter(|&index| nodes[index].primary).collect();
            match primary.as_slice() {
                [one] => Ok(vec![*one]),
                _ => Err(Edge::AmbiguousDependency(
                    request.clone(),
                    many.iter().map(|&index| nodes[index].key.clone()).collect(),
                )),
            }
        }
    }
}

/// Visits registration roots to report the first failure with its full path.
// BuildError preserves a complete path from a root to the graph failure.
#[allow(clippy::result_large_err)]
fn detect_errors_and_cycles(
    nodes: &[Node],
    edges: &[Vec<Edge>],
    reachable: &[bool],
    diagnostics: &DiagnosticPaths,
) -> Result<(), BuildError> {
    let mut state = vec![0u8; nodes.len()];
    let mut stack = Vec::new();
    let ordered_roots = diagnostics
        .roots()
        .iter()
        .copied()
        .map(|location| diagnostics.node_index(location));
    for root in ordered_roots.chain(0..nodes.len()) {
        if reachable[root] && state[root] == 0 {
            state[root] = 1;
            stack.push((root, 0));
            while let Some((index, next_edge)) = stack.last_mut() {
                if *next_edge == edges[*index].len() {
                    let (finished, _) = stack.pop().expect("DFS frame exists");
                    state[finished] = 2;
                    continue;
                }
                let edge = &edges[*index][*next_edge];
                *next_edge += 1;
                match edge {
                    Edge::Target(target) => match state[*target] {
                        2 => {}
                        1 => {
                            let cycle_start = stack
                                .iter()
                                .position(|(entry, _)| entry == target)
                                .expect("active target must be present in the DFS stack");
                            let mut path = diagnostics.path_to(nodes[*target].location);
                            path.extend(
                                stack[cycle_start + 1..]
                                    .iter()
                                    .map(|(entry, _)| nodes[*entry].key.clone()),
                            );
                            path.push(nodes[*target].key.clone());
                            return Err(BuildError::DependencyCycle { path });
                        }
                        _ => {
                            state[*target] = 1;
                            stack.push((*target, 0));
                        }
                    },
                    Edge::MissingDependency(dependency) => {
                        return Err(BuildError::MissingDependency {
                            dependency: dependency.clone(),
                            definition: nodes[*index].source,
                            path: diagnostics.path_to(nodes[*index].location),
                        });
                    }
                    Edge::AmbiguousDependency(dependency, candidates) => {
                        return Err(BuildError::AmbiguousBinding {
                            dependency: dependency.clone(),
                            definition: nodes[*index].source,
                            candidates: candidates.clone(),
                            path: diagnostics.path_to(nodes[*index].location),
                        });
                    }
                    Edge::MissingAliasTarget(target) => {
                        return Err(BuildError::MissingAliasTarget {
                            alias: nodes[*index].key.clone(),
                            target: target.clone(),
                            definition: nodes[*index].source,
                            path: diagnostics.path_to(nodes[*index].location),
                        });
                    }
                }
            }
        }
    }
    Ok(())
}

/// Uses registration position to break ties among currently ready bindings.
fn stable_topology(nodes: &[Node], edges: &[Vec<Edge>], reachable: &[bool]) -> Vec<BindingLocation> {
    let mut indegree = vec![0usize; nodes.len()];
    let mut dependents = vec![Vec::new(); nodes.len()];
    for (index, requests) in edges.iter().enumerate() {
        let mut unique = HashSet::new();
        if !reachable[index] {
            continue;
        }
        for edge in requests {
            if let Edge::Target(target) = edge
                && reachable[*target]
                && unique.insert(target)
            {
                indegree[index] += 1;
                dependents[*target].push(index);
            }
        }
    }
    let mut ready = BinaryHeap::new();
    for (index, &count) in indegree.iter().enumerate() {
        if reachable[index] && count == 0 {
            ready.push(Reverse(index));
        }
    }
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(Reverse(index)) = ready.pop() {
        order.push(nodes[index].location);
        for &dependent in &dependents[index] {
            indegree[dependent] -= 1;
            if indegree[dependent] == 0 {
                ready.push(Reverse(dependent));
            }
        }
    }
    debug_assert_eq!(
        order.len(),
        reachable.iter().filter(|yes| **yes).count(),
        "cycles were checked before sorting"
    );
    order
}
