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

/// Resolves each root request and returns the selected binding positions.
///
/// Candidate positions are deduplicated while preserving the request order of
/// `roots`, so a binding reachable from several roots is selected once at the
/// position of its first request.
///
/// # Errors
///
/// Returns [`BuildError::MissingRoot`] when a required root request has no
/// candidate, or [`BuildError::AmbiguousRoot`] when one request resolves to
/// several candidates. Both variants report the failing request and the
/// candidate keys known to the index at that point. An optional root without
/// candidates contributes no selected binding and is not an error.
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
/// location. The bitmap has the same length as `nodes`; when `seeds` is empty,
/// every bit is false and no reachable paths are recorded.
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
            if diagnostics.record_from(member_location, PathOrigin::DefinitionMember(node.location))
            {
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
///
/// # Parameters
///
/// - `edge`: The resolved target or deferred failure to convert.
/// - `node`: The node whose dependency or alias edge was resolved.
/// - `path`: The diagnostic path from a selected root to `node`.
///
/// # Returns
///
/// The corresponding build error for a failure edge, or `None` for a target.
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
/// Seeds are chosen in registration order. Nodes with no incoming dependency
/// edge are considered first; then the first still-uncovered node of each
/// remaining region is added so cycles are also reached by validation.
/// Every node is covered by the resulting seed closures. An empty graph
/// produces an empty vector.
///
/// # Parameters
///
/// - `nodes`: The flattened bindings whose components are being selected.
/// - `edges`: The resolved outgoing edges indexed in parallel with `nodes`.
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
            mark_build_all_closure(
                index,
                nodes,
                edges,
                &members,
                &mut selected,
                &mut definition_expanded,
            );
        }
    }
    for index in 0..selected.len() {
        if !selected[index] {
            seeds.push(index);
            mark_build_all_closure(
                index,
                nodes,
                edges,
                &members,
                &mut selected,
                &mut definition_expanded,
            );
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
///
/// # Parameters
///
/// - `definitions`: The active definitions whose bindings are flattened.
///
/// # Returns
///
/// One graph node per declared binding, preserving definition and binding
/// order.
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
/// # Errors
///
/// Returns [`BuildError::DuplicateBinding`] for the first key that appears
/// twice, reporting the key together with the sources of the first and second
/// definitions that declare it.
///
/// # Parameters
///
/// - `nodes`: Active graph nodes in registration order.
///
/// # Returns
///
/// A map from each unique exact binding key to its node position.
#[allow(clippy::result_large_err)]
pub(in crate::graph) fn exact_keys(
    nodes: &[Node],
) -> Result<HashMap<BindingKey, usize>, BuildError> {
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
///
/// # Parameters
///
/// - `nodes`: Active graph nodes in registration order.
///
/// # Returns
///
/// `Ok(())` when no type has multiple primary bindings.
#[allow(clippy::result_large_err)]
pub(in crate::graph) fn validate_primary(nodes: &[Node]) -> Result<(), BuildError> {
    let mut seen = HashSet::new();
    for node in nodes {
        if node.primary && !seen.insert(node.key.type_id()) {
            let candidates = nodes
                .iter()
                .filter(|candidate| {
                    candidate.primary && candidate.key.type_id() == node.key.type_id()
                })
                .map(|candidate| (candidate.key.clone(), candidate.source))
                .collect();
            return Err(BuildError::MultiplePrimaryBindings { candidates });
        }
    }
    Ok(())
}

/// Resolves every dependency request declared by one definition.
///
/// Successful selections contribute their keys to the resolved requests and
/// target edges in request order. Failed selections remain as error edges for
/// the graph validation pass to report with its traversal-specific path.
///
/// # Parameters
///
/// - `definition`: The definition whose dependency requests are resolved.
/// - `nodes`: The flattened graph nodes indexed by the candidate index.
/// - `index`: The candidate index built over `nodes`.
///
/// # Returns
///
/// The dependency edges in request and candidate order, together with one
/// resolved request per declared dependency in declaration order.
fn resolve_definition_dependencies(
    definition: &PendingDefinition,
    nodes: &[Node],
    index: &BindingIndex,
) -> (Vec<Edge>, Vec<ResolvedDependency>) {
    let mut edges = Vec::new();
    let mut resolved = Vec::with_capacity(definition.dependencies.len());
    for request in &definition.dependencies {
        let mut dependency = ResolvedDependency {
            request: request.clone(),
            keys: Vec::new(),
        };
        match select(request, candidates_for(request, index), nodes) {
            Ok(selected) => {
                dependency.keys = selected
                    .iter()
                    .map(|&position| nodes[position].key.clone())
                    .collect();
                edges.extend(selected.into_iter().map(Edge::Target));
            }
            Err(problem) => edges.push(problem),
        }
        resolved.push(dependency);
    }
    (edges, resolved)
}

/// Resolves each request, then creates binding and alias edges.
///
/// Alias nodes resolve their concrete target directly and contribute exactly
/// one edge, while ordinary nodes contribute one edge per dependency request of
/// their definition. Definitions without ordinary nodes keep their initialized
/// request records with empty keys.
///
/// # Parameters
///
/// - `definitions`: Definitions whose declared dependency requests are read.
/// - `nodes`: Flattened binding nodes indexed by the returned edges.
/// - `index`: The binding candidate index built over `nodes`.
///
/// # Returns
///
/// The outgoing edges, indexed in parallel with `nodes`, together with
/// resolved requests indexed by definition and declaration order.
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
        let (node_edges, requests) =
            resolve_definition_dependencies(&definitions[node.location.definition], nodes, index);
        resolved[node.location.definition] = requests;
        edges.push(node_edges);
    }
    (edges, resolved)
}

/// Resolves only nodes in the closure of `seeds`, leaving full-shape outputs.
///
/// Definitions may contain several bindings, so reaching one member queues
/// every member. Each node's alias edge is resolved independently, while only
/// concrete nodes resolve the definition's dependency declarations.
///
/// # Parameters
///
/// - `definitions`: Active definitions whose members and requests form the
///   graph.
/// - `nodes`: Flattened binding nodes indexed by the returned edges.
/// - `index`: The binding candidate index built over every active node.
/// - `seeds`: Root node positions from which dependency resolution begins.
///
/// # Returns
///
/// Full-shape per-node outgoing edges and per-definition resolved requests;
/// definitions without a visited ordinary node retain empty request lists.
pub(in crate::graph) fn resolve_reachable_edges(
    definitions: &[PendingDefinition],
    nodes: &[Node],
    index: &BindingIndex,
    seeds: &[usize],
) -> (Vec<Vec<Edge>>, Vec<Vec<ResolvedDependency>>) {
    let mut edges = vec![Vec::new(); nodes.len()];
    let mut resolved = (0..definitions.len())
        .map(|_| Vec::new())
        .collect::<Vec<_>>();
    let mut visited = vec![false; nodes.len()];
    let mut expanded = vec![false; definitions.len()];
    let mut resolved_initialized = vec![false; definitions.len()];
    let mut dependency_edges = (0..definitions.len())
        .map(|_| None)
        .collect::<Vec<Option<Vec<Edge>>>>();
    // `flatten` lays each definition's bindings out contiguously, so retain
    // just its range instead of allocating one node-index vector per
    // definition.
    let mut nodes_by_definition = Vec::with_capacity(definitions.len());
    let mut node_start = 0;
    for definition in definitions {
        let node_end = node_start + definition.bindings.len();
        nodes_by_definition.push(node_start..node_end);
        node_start = node_end;
    }
    let mut queue = seeds.to_vec();
    let mut cursor = 0;
    while cursor < queue.len() {
        let ni = queue[cursor];
        cursor += 1;
        if visited[ni] {
            continue;
        }
        visited[ni] = true;
        let node = &nodes[ni];
        let di = node.location.definition;
        if !expanded[di] {
            expanded[di] = true;
            queue.extend(nodes_by_definition[di].clone());
        }
        let binding = &definitions[di].bindings[node.location.binding];
        if let PendingBindingKind::Alias { target, .. } = &binding.kind {
            edges[ni].push(match index.by_key(target).first() {
                Some(&t) => Edge::Target(t),
                None => Edge::MissingAliasTarget(target.clone()),
            });
        } else {
            if !resolved_initialized[di] {
                let (definition_edges, requests) =
                    resolve_definition_dependencies(&definitions[di], nodes, index);
                resolved[di] = requests;
                dependency_edges[di] = Some(definition_edges);
                resolved_initialized[di] = true;
            }
            for edge in dependency_edges[di]
                .as_ref()
                .expect("concrete definition dependencies were resolved")
                .iter()
                .cloned()
            {
                if let Edge::Target(target) = &edge {
                    queue.push(*target);
                }
                edges[ni].push(edge);
            }
        }
    }
    (edges, resolved)
}

/// Gets candidate indices for an exact ID or for a Rust type namespace.
///
/// A request carrying an id string is matched on the full exact key, and an id
/// that [`BindingId::parse`] rejects yields no candidates at all rather than
/// falling back to the type namespace, so a malformed id surfaces as a missing
/// dependency instead of a wrong match.
///
/// # Parameters
///
/// - `request`: The exact-ID or type-wide dependency request to match.
/// - `index`: The binding index containing active exact-key and type positions.
///
/// # Returns
///
/// Candidate node positions for the request, or an empty slice when none
/// match.
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
/// Returns node positions sorted by binding order, binding ID, definition
/// source, and registration position for `All` cardinality; returns a single
/// position or an empty vector otherwise.
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
            let primary: Vec<_> = many
                .iter()
                .copied()
                .filter(|&index| nodes[index].primary)
                .collect();
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
                } else if let Some(problem) = edge_problem(
                    edge,
                    &nodes[*index],
                    diagnostics.path_to(nodes[*index].location),
                ) {
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
///
/// # Parameters
///
/// - `nodes`: The flattened graph nodes.
/// - `edges`: Outgoing edges indexed in parallel with `nodes`.
/// - `reachable`: Flags selecting the nodes to place in dependency-first order.
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
