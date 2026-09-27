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
use crate::options::DefinitionSource;

/// The position of one binding in its registered definition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BindingLocation {
    /// Index of the owning definition in the active definition vector.
    pub(crate) definition: usize,
    /// Index of the binding within that definition.
    pub(crate) binding: usize,
}

/// A declared request and its exact selected keys, in injection order.
pub(crate) struct ResolvedDependency {
    /// Original request declared by the definition.
    pub(crate) request: Dependency,
    /// Selected keys in deterministic injection order.
    pub(crate) keys: Vec<BindingKey>,
}

/// Active definitions, resolved requests, and dependency-first binding order.
///
/// Every binding occurs once in `order`; aliases follow their target, and
/// concrete bindings follow every selected request of their definition.
pub(crate) struct ValidatedGraph {
    /// Definitions remaining after profile filtering.
    pub(crate) definitions: Vec<PendingDefinition>,
    /// Dependency-first execution order for selected bindings.
    pub(crate) order: Vec<BindingLocation>,
    /// Resolved request keys, indexed by definition and declaration order.
    pub(crate) resolved: Vec<Vec<ResolvedDependency>>,
}

/// One flattened binding with its stable registration position.
#[derive(Clone)]
struct Node {
    /// Original position within the definition list.
    location: BindingLocation,
    /// Exact typed identity used during matching.
    key: BindingKey,
    /// Definition source retained for diagnostics and stable ordering.
    source: DefinitionSource,
    /// Whether an unnamed request can select this candidate as primary.
    primary: bool,
    /// Collection ordering value.
    order: i32,
}

/// A resolved edge or a failure delayed until its root path is known.
enum Edge {
    /// A dependency or alias target selected by validation.
    Target(usize),
    /// A required request with no matching key.
    MissingDependency(Dependency),
    /// A single-value request with multiple candidates.
    AmbiguousDependency(Dependency, Vec<BindingKey>),
    /// An alias refers to a key absent from active definitions.
    MissingAliasTarget(BindingKey),
}

// BuildError carries public candidate sets and complete dependency paths.
#[allow(clippy::result_large_err)]
impl ValidatedGraph {
    /// Validates definitions as an unrooted test graph.
    ///
    /// This helper is available only to graph unit tests; production callers
    /// use root selection through `ContainerBuilder`.
    #[cfg(test)]
    pub(crate) fn validate(
        definitions: Vec<PendingDefinition>,
        active_profiles: &[String],
    ) -> Result<Self, BuildError> {
        Self::validate_roots(definitions, active_profiles, None)
    }

    /// Filters `definitions` using `active_profiles`, then validates the graph.
    ///
    /// An empty profile slice activates `default`. This never calls a factory
    /// or alias projector. Errors carry the relevant binding path.
    pub(crate) fn validate_roots(
        definitions: Vec<PendingDefinition>,
        active_profiles: &[String],
        roots: Option<&[Dependency]>,
    ) -> Result<Self, BuildError> {
        let definitions: Vec<_> = definitions
            .into_iter()
            .filter(|definition| {
                definition.profile.as_deref().is_none_or(|profile| {
                    if active_profiles.is_empty() {
                        profile == "default"
                    } else {
                        active_profiles.iter().any(|active| active == profile)
                    }
                })
            })
            .collect();
        let nodes = flatten(&definitions);
        let by_key = if roots.is_none() {
            exact_keys(&nodes)?
        } else {
            first_keys(&nodes)
        };
        if roots.is_none() {
            validate_primary(&nodes)?;
        }
        let mut by_type: HashMap<TypeId, Vec<usize>> = HashMap::new();
        for (index, node) in nodes.iter().enumerate() {
            by_type.entry(node.key.type_id()).or_default().push(index);
        }
        let (edges, resolved) = resolve_edges(&definitions, &nodes, &by_key, &by_type);
        let reachable = if let Some(roots) = roots {
            select_roots(roots, &nodes, &by_type)?
        } else {
            vec![true; nodes.len()]
        };
        let reachable = if roots.is_some() {
            close_definitions(&nodes, &edges, reachable)?
        } else {
            reachable
        };
        if roots.is_some() {
            let mut sources = HashMap::new();
            for (i, node) in nodes.iter().enumerate().filter(|(i, _)| reachable[*i]) {
                let _ = i;
                if let Some(first) = sources.insert(node.key.clone(), node.source) {
                    return Err(BuildError::DuplicateBinding {
                        key: node.key.clone(),
                        first,
                        second: node.source,
                    });
                }
            }
            let selected: Vec<_> = nodes
                .iter()
                .enumerate()
                .filter(|(i, _)| reachable[*i])
                .map(|(_, n)| n.clone())
                .collect();
            validate_primary(&selected)?;
        }
        detect_errors_and_cycles(&nodes, &edges, &reachable)?;
        let order = stable_topology(&nodes, &edges, &reachable);
        Ok(Self {
            definitions,
            order,
            resolved,
        })
    }
}

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
) -> Result<Vec<bool>, BuildError> {
    let mut selected = vec![false; nodes.len()];
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
                    selected[index] = true;
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

/// Extends root reachability to every binding that belongs to a selected
/// definition and validates the newly included edges.
#[allow(clippy::result_large_err)]
fn close_definitions(nodes: &[Node], edges: &[Vec<Edge>], mut reachable: Vec<bool>) -> Result<Vec<bool>, BuildError> {
    let definition_count = nodes
        .iter()
        .map(|node| node.location.definition)
        .max()
        .map_or(0, |index| index + 1);
    let mut nodes_by_definition = vec![Vec::new(); definition_count];
    for (index, node) in nodes.iter().enumerate() {
        nodes_by_definition[node.location.definition].push(index);
    }
    let mut discovered = reachable.clone();
    let mut predecessors = vec![None; nodes.len()];
    let mut queue = reachable
        .iter()
        .enumerate()
        .filter_map(|(index, yes)| yes.then_some(index))
        .collect::<VecDeque<_>>();
    let mut visited_definitions = HashSet::new();
    while let Some(index) = queue.pop_front() {
        let definition = nodes[index].location.definition;
        if !visited_definitions.insert(definition) {
            continue;
        }
        for &i in &nodes_by_definition[definition] {
            let node = &nodes[i];
            reachable[i] = true;
            let node_path = binding_path(index, &predecessors, nodes);
            for edge in &edges[i] {
                match edge {
                    Edge::Target(target) => {
                        if !discovered[*target] {
                            discovered[*target] = true;
                            predecessors[*target] = Some(i);
                            queue.push_back(*target);
                        }
                    }
                    Edge::MissingDependency(dependency) => {
                        return Err(BuildError::MissingDependency {
                            dependency: dependency.clone(),
                            definition: node.source,
                            path: node_path,
                        });
                    }
                    Edge::AmbiguousDependency(dependency, candidates) => {
                        return Err(BuildError::AmbiguousBinding {
                            dependency: dependency.clone(),
                            definition: node.source,
                            candidates: candidates.clone(),
                            path: node_path,
                        });
                    }
                    Edge::MissingAliasTarget(target) => {
                        return Err(BuildError::MissingAliasTarget {
                            alias: node.key.clone(),
                            target: target.clone(),
                            definition: node.source,
                            path: node_path,
                        });
                    }
                }
            }
        }
    }
    Ok(reachable)
}

/// Reconstructs the first root path to a binding from predecessor links.
fn binding_path(index: usize, predecessors: &[Option<usize>], nodes: &[Node]) -> Vec<BindingKey> {
    let mut path = Vec::new();
    let mut current = Some(index);
    while let Some(entry) = current {
        path.push(nodes[entry].key.clone());
        current = predecessors[entry];
    }
    path.reverse();
    path
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
fn detect_errors_and_cycles(nodes: &[Node], edges: &[Vec<Edge>], reachable: &[bool]) -> Result<(), BuildError> {
    let mut state = vec![0u8; nodes.len()];
    let mut stack = Vec::new();
    for (root, is_reachable) in reachable.iter().copied().enumerate() {
        if is_reachable && state[root] == 0 {
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
                            let path = stack
                                .iter()
                                .map(|(entry, _)| nodes[*entry].key.clone())
                                .chain(std::iter::once(nodes[*target].key.clone()))
                                .collect();
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
                            path: stack.iter().map(|(entry, _)| nodes[*entry].key.clone()).collect(),
                        });
                    }
                    Edge::AmbiguousDependency(dependency, candidates) => {
                        return Err(BuildError::AmbiguousBinding {
                            dependency: dependency.clone(),
                            definition: nodes[*index].source,
                            candidates: candidates.clone(),
                            path: stack.iter().map(|(entry, _)| nodes[*entry].key.clone()).collect(),
                        });
                    }
                    Edge::MissingAliasTarget(target) => {
                        return Err(BuildError::MissingAliasTarget {
                            alias: nodes[*index].key.clone(),
                            target: target.clone(),
                            definition: nodes[*index].source,
                            path: stack.iter().map(|(entry, _)| nodes[*entry].key.clone()).collect(),
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
