// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Root selection, reachability closure, edge resolution, and cycle-free
//! ordering algorithms over the validated binding graph.

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
use crate::graph::DiagnosticPaths;
use crate::graph::PathOrigin;
use crate::graph::internal::binding_index::BindingIndex;
use crate::graph::internal::binding_location::BindingLocation;
use crate::graph::internal::edge::Edge;
use crate::graph::internal::node::Node;
use crate::graph::internal::resolved_dependency::ResolvedDependency;
use crate::key::BindingId;
use crate::key::BindingKey;

/// Resolves each required root and returns the selected binding positions.
///
/// Candidate positions are deduplicated while preserving the request order of
/// `roots`, so a binding reachable from several roots is selected once at the
/// position of its first request.
///
/// # Errors
///
/// Returns [`BuildError::MissingRoot`] when a root request has no candidate, or
/// [`BuildError::AmbiguousRoot`] when one request resolves to several
/// candidates. Both variants report the failing request and the candidate keys
/// known to the index at that point.
///
/// # Parameters
///
/// * `roots` - the root requests in caller order; the order fixes the position
///   of every selected binding, because the first request that can reach a
///   binding claims it.
/// * `nodes` - the flattened node list whose positions the result indexes.
/// * `index` - the candidate index built by [`BindingIndex::new`] over the same
///   `nodes`.
///
/// # Returns
///
/// The selected node positions in first-request order with duplicates removed.
/// An empty vector is a valid result when `roots` is empty.
#[allow(clippy::result_large_err)]
pub(in crate::graph) fn select_roots(
    roots: &[Dependency],
    nodes: &[Node],
    index: &BindingIndex,
) -> Result<Vec<usize>, BuildError> {
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    for request in roots {
        let candidates = candidates_for(request, index);
        match select(request, candidates, nodes) {
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
                    available: index
                        .by_type(request.type_id())
                        .iter()
                        .map(|&position| nodes[position].key.clone())
                        .collect(),
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
///
/// Each definition is expanded at most once, so the traversal stays linear in
/// the number of nodes and edges even when definitions share members.
///
/// # Errors
///
/// Returns the first [`BuildError`] carried by a reachable edge:
/// [`BuildError::MissingDependency`], [`BuildError::AmbiguousBinding`], or
/// [`BuildError::MissingAliasTarget`]. Each error records the diagnostic path
/// from the root that led to the failing node.
///
/// # Parameters
///
/// * `definitions` - the active definitions that seed the diagnostic paths; a
///   definition absent here contributes no root to the walk.
/// * `nodes` - the flattened node list whose positions the result indexes.
/// * `edges` - per-node outgoing edges, indexed in parallel with `nodes`.
/// * `seeds` - node positions to start from; duplicates and already covered
///   positions are ignored.
///
/// # Returns
///
/// The reachable bitmap, indexed in parallel with `nodes`, together with the
/// [`DiagnosticPaths`] that recorded the one retained predecessor per reached
/// location. Both are empty for an empty `seeds` list.
#[allow(clippy::result_large_err)]
pub(in crate::graph) fn close_definitions(
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
    let mut definition_expanded = vec![false; definition_count];
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
            if let Edge::Target(target) = edge {
                let target_location = nodes[*target].location;
                if diagnostics.record_from(target_location, PathOrigin::Dependency(node.location)) {
                    reachable[*target] = true;
                    queue.push_back(*target);
                }
                continue;
            }
            if let Some(problem) = edge_problem(edge, node, diagnostics.path_to(node.location)) {
                return Err(problem);
            }
        }
        if definition_expanded[node.location.definition] {
            continue;
        }
        definition_expanded[node.location.definition] = true;
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

/// Maps one deferred edge failure to the build error reported for its node.
///
/// [`Edge::Target`] is a resolved dependency and yields `None`; the three
/// failure variants become [`BuildError::MissingDependency`],
/// [`BuildError::AmbiguousBinding`], or [`BuildError::MissingAliasTarget`],
/// each carrying `node.source` as the requesting definition and the supplied
/// diagnostic `path`. Sharing one mapping keeps the closure walk and the cycle
/// walk reporting identical errors for identical edges.
fn edge_problem(edge: &Edge, node: &Node, path: Vec<BindingKey>) -> Option<BuildError> {
    match edge {
        Edge::Target(_) => None,
        Edge::MissingDependency(dependency) => Some(BuildError::MissingDependency {
            dependency: dependency.clone(),
            definition: node.source,
            path,
        }),
        Edge::AmbiguousDependency(dependency, candidates) => Some(BuildError::AmbiguousBinding {
            dependency: dependency.clone(),
            definition: node.source,
            candidates: candidates.clone(),
            path,
        }),
        Edge::MissingAliasTarget(target) => Some(BuildError::MissingAliasTarget {
            alias: node.key.clone(),
            target: target.clone(),
            definition: node.source,
            path,
        }),
    }
}

/// Selects unrooted graph components in registration order for `build_all`.
///
/// # Returns
///
/// One node position per graph component that no other node depends on, in
/// registration order. An isolated node is such a component, and the result is
/// empty only when every node has an incoming dependency.
pub(in crate::graph) fn build_all_seeds(nodes: &[Node], edges: &[Vec<Edge>]) -> Vec<usize> {
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
    let mut definition_expanded = vec![false; definition_count];
    for (index, node) in nodes.iter().enumerate() {
        members[node.location.definition].push(index);
    }
    for (index, incoming) in has_incoming.iter().copied().enumerate() {
        if !incoming {
            seeds.push(index);
            mark_build_all_closure(index, nodes, edges, &members, &mut selected, &mut definition_expanded);
        }
    }
    let indices = (0..selected.len()).collect::<Vec<_>>();
    for index in indices {
        if !selected[index] {
            seeds.push(index);
            mark_build_all_closure(index, nodes, edges, &members, &mut selected, &mut definition_expanded);
        }
    }
    seeds
}

/// Marks one build-all component, including its dependency and
/// definition-member closure. The shared `selected` and `definition_expanded`
/// bitmaps ensure each node and definition member list is traversed once
/// across seeds, while preserving every seed's registration position.
///
/// # Parameters
///
/// * `seed` - the component root to mark; a position already marked by an
///   earlier seed is left untouched.
/// * `nodes` - the flattened node list whose positions `members` indexes.
/// * `edges` - per-node outgoing edges, indexed in parallel with `nodes`.
/// * `members` - node positions grouped by their owning definition.
/// * `selected` - the reachability bitmap updated in place for the component.
/// * `definition_expanded` - the bitmap that records which definitions already
///   had their member list walked.
fn mark_build_all_closure(
    seed: usize,
    nodes: &[Node],
    edges: &[Vec<Edge>],
    members: &[Vec<usize>],
    selected: &mut [bool],
    definition_expanded: &mut [bool],
) {
    if selected[seed] {
        return;
    }
    selected[seed] = true;
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
        let definition = nodes[index].location.definition;
        if definition_expanded[definition] {
            continue;
        }
        definition_expanded[definition] = true;
        for &member in &members[definition] {
            if !selected[member] {
                selected[member] = true;
                queue.push_back(member);
            }
        }
    }
}

/// Flattens definitions in builder entry order and binding declaration order.
pub(in crate::graph) fn flatten(definitions: &[PendingDefinition]) -> Vec<Node> {
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
///
/// Returns a map from every exact binding key to its single node position.
///
/// # Errors
///
/// Returns [`BuildError::DuplicateBinding`] for the first key that appears
/// twice, reporting the key together with the sources of the first and second
/// definitions that declare it.
#[allow(clippy::result_large_err)]
pub(in crate::graph) fn exact_keys(nodes: &[Node]) -> Result<HashMap<BindingKey, usize>, BuildError> {
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
///
/// Only nodes marked as primary take part in this check, and the first
/// duplicate type identifier decides the reported candidate set.
///
/// # Errors
///
/// Returns [`BuildError::MultiplePrimaryBindings`] listing the key and source
/// of every primary binding that shares the offending type identifier.
#[allow(clippy::result_large_err)]
pub(in crate::graph) fn validate_primary(nodes: &[Node]) -> Result<(), BuildError> {
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
///
/// Alias nodes resolve their concrete target directly and contribute exactly
/// one edge, while ordinary nodes contribute one edge per dependency request of
/// their definition, so both the edge list and the resolved list stay aligned
/// with their inputs.
///
/// # Returns
///
/// The outgoing edges, indexed in parallel with `nodes`, together with the
/// resolved requests, indexed by definition and then by that definition's
/// dependency order. Requests that a node cannot reach keep an empty
/// `ResolvedDependency::keys`.
pub(in crate::graph) fn resolve_edges(
    definitions: &[PendingDefinition],
    nodes: &[Node],
    index: &BindingIndex,
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
            edges.push(vec![match index.by_key(target).first() {
                Some(&target_index) => Edge::Target(target_index),
                None => Edge::MissingAliasTarget(target.clone()),
            }]);
            continue;
        }
        let mut node_edges = Vec::new();
        for (request_index, request) in definitions[node.location.definition].dependencies.iter().enumerate() {
            let candidates = candidates_for(request, index);
            match select(request, candidates, nodes) {
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
///
/// A request carrying an id string is matched on the full exact key, and an id
/// that [`BindingId::parse`] rejects yields no candidates at all rather than
/// falling back to the type namespace, so a malformed id surfaces as a missing
/// dependency instead of a wrong match.
fn candidates_for<'index>(request: &Dependency, index: &'index BindingIndex) -> &'index [usize] {
    match request.id() {
        Some(id) => {
            let Ok(id) = BindingId::parse(id) else {
                return &[];
            };
            let key = BindingKey::from_parts(request.type_id(), request.type_name(), Some(id));
            index.by_key(&key)
        }
        None => index.by_type(request.type_id()),
    }
}

/// Applies cardinality, primary, and collection ordering to candidates.
///
/// Returns node positions in registration order for `All` cardinality, and a
/// single position or an empty vector otherwise; the sort key is the binding
/// order, then the binding id, then the definition source, then the position.
///
/// # Errors
///
/// Returns [`Edge::MissingDependency`] for an empty candidate list of required
/// cardinality, and [`Edge::AmbiguousDependency`] with the competing keys when
/// an id request has several candidates or a non-id request has no unique
/// primary candidate.
///
/// # Parameters
///
/// * `request` - supplies the cardinality and id rules that decide how many
///   candidates may be selected.
/// * `candidates` - node positions to choose from, indexed into `nodes`.
/// * `nodes` - the flattened node list the positions refer to.
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
///
/// The depth-first walk uses an explicit frame stack so deep graphs cannot
/// overflow the call stack, and only reachable nodes are visited.
///
/// # Errors
///
/// Returns [`BuildError::DependencyCycle`] with the cycle path closed back to
/// its entry node, or the first edge problem encountered on a root path:
/// [`BuildError::MissingDependency`], [`BuildError::AmbiguousBinding`], and
/// [`BuildError::MissingAliasTarget`].
///
/// # Parameters
///
/// * `nodes` - the flattened node list that positions in `reachable` and in
///   `edges` index into.
/// * `edges` - per-node outgoing edges, indexed in parallel with `nodes`.
/// * `reachable` - restricts the walk to nodes the closure reached; nodes left
///   `false` are never used as walk roots.
/// * `diagnostics` - supplies the registration-root order that makes the
///   reported failure deterministic, and the path prefix of each error.
#[allow(clippy::result_large_err)]
pub(in crate::graph) fn detect_errors_and_cycles(
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
                if let Edge::Target(target) = edge {
                    match state[*target] {
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
                    }
                } else if let Some(problem) =
                    edge_problem(edge, &nodes[*index], diagnostics.path_to(nodes[*index].location))
                {
                    return Err(problem);
                }
            }
        }
    }
    Ok(())
}

/// Uses registration position to break ties among currently ready bindings.
///
/// # Returns
///
/// One [`BindingLocation`] per reachable node, in an order where every node
/// appears after all nodes it depends on. Ties between simultaneously ready
/// nodes are settled by registration position, so the result is deterministic
/// and contains no duplicates; a reachable cycle would leave nodes out, which
/// the caller must have ruled out beforehand.
pub(in crate::graph) fn stable_topology(
    nodes: &[Node],
    edges: &[Vec<Edge>],
    reachable: &[bool],
) -> Vec<BindingLocation> {
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
