// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Validated graph state and root-selection entry points.

use std::collections::HashMap;

use crate::binding::PendingBindingKind;
use crate::binding::PendingDefinition;
use crate::binding::profile_is_active;
use crate::dependency::Dependency;
use crate::error::BuildError;
use crate::graph::DiagnosticPaths;
use crate::graph::internal::binding_index::BindingIndex;
use crate::graph::internal::binding_location::BindingLocation;
use crate::graph::internal::node::Node;
use crate::graph::internal::resolved_dependency::ResolvedDependency;
use crate::graph::internal::selection::build_all_seeds;
use crate::graph::internal::selection::close_definitions;
use crate::graph::internal::selection::detect_errors_and_cycles;
use crate::graph::internal::selection::exact_keys;
use crate::graph::internal::selection::flatten;
use crate::graph::internal::selection::resolve_edges;
use crate::graph::internal::selection::select_roots;
use crate::graph::internal::selection::stable_topology;
use crate::graph::internal::selection::validate_primary;

/// Active definitions, resolved requests, and dependency-first binding order.
///
/// Every selected binding occurs once in `order`; aliases follow their target,
/// and concrete bindings follow every selected request of their definition.
pub(crate) struct ValidatedGraph {
    /// Definitions remaining after profile filtering.
    pub(crate) definitions: Vec<PendingDefinition>,
    /// Dependency-first execution order for selected bindings.
    pub(crate) order: Vec<BindingLocation>,
    /// Resolved request keys, indexed by definition and declaration order.
    pub(crate) resolved: Vec<Vec<ResolvedDependency>>,
    /// Compact root provenance shared by validation and construction.
    pub(crate) diagnostics: DiagnosticPaths,
}
// BuildError carries public candidate sets and complete dependency paths.
#[allow(clippy::result_large_err)]
impl ValidatedGraph {
    /// Validates definitions as an unrooted test graph.
    ///
    /// This helper is available only to graph unit tests; production callers
    /// use root selection through `ContainerBuilder`.
    ///
    /// # Parameters
    ///
    /// * `definitions` - Definitions to validate, consumed by this call.
    /// * `active_profiles` - Profiles to keep; an empty slice activates
    ///   `default`.
    ///
    /// # Returns
    ///
    /// A validated graph over every remaining definition, with no requested
    /// roots and therefore a dependency-first order over all reachable nodes.
    ///
    /// # Errors
    ///
    /// Returns the same variants as
    /// [`ValidatedGraph::validate_roots`] with `roots` set to `None`.
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
    ///
    /// Key uniqueness and primary uniqueness are checked before graph expansion
    /// only when `roots` is `None`; requested roots defer both checks until the
    /// reachable subset is known, so bindings outside it cannot fail
    /// validation.
    ///
    /// # Parameters
    ///
    /// * `definitions` - Definitions to validate, consumed by this call and
    ///   filtered in place by profile activity.
    /// * `active_profiles` - Profiles to keep; an empty slice activates
    ///   `default`.
    /// * `roots` - Requested root dependencies. `Some` selects only the nodes
    ///   reachable from those roots; `None` selects every component in
    ///   registration order for `build_all`.
    ///
    /// # Returns
    ///
    /// A validated graph holding the profile-filtered `definitions`, the
    /// dependency-first `order` over the selected bindings, the `resolved`
    /// request keys per definition, and the `diagnostics` root provenance.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::DuplicateBinding`] for the first repeated exact
    /// key, [`BuildError::MultiplePrimaryBindings`] listing every primary
    /// source that shares one Rust type namespace,
    /// [`BuildError::MissingRoot`] for an unsatisfiable requested root,
    /// [`BuildError::MissingDependency`], [`BuildError::AmbiguousBinding`]
    /// or [`BuildError::MissingAliasTarget`] for the first unresolvable
    /// request, and [`BuildError::DependencyCycle`] for the first cycle
    /// reachable from a root.
    pub(crate) fn validate_roots(
        definitions: Vec<PendingDefinition>,
        active_profiles: &[String],
        roots: Option<&[Dependency]>,
    ) -> Result<Self, BuildError> {
        let definitions: Vec<_> = definitions
            .into_iter()
            .filter(|definition| profile_is_active(definition.profile.as_deref(), active_profiles))
            .collect();
        let nodes = flatten(&definitions);
        if roots.is_none() {
            exact_keys(&nodes)?;
            validate_primary(&nodes)?;
        }
        let index = BindingIndex::new(&nodes);
        let (edges, resolved) = resolve_edges(&definitions, &nodes, &index);
        let seeds = if let Some(roots) = roots {
            select_roots(roots, &nodes, &index)?
        } else {
            build_all_seeds(&nodes, &edges)
        };
        let (reachable, diagnostics) = close_definitions(&definitions, &nodes, &edges, &seeds)?;
        if roots.is_some() {
            validate_reachable_bindings(&nodes, &reachable)?;
        }
        detect_errors_and_cycles(&nodes, &edges, &reachable, &diagnostics)?;
        let order = stable_topology(&nodes, &edges, &reachable);
        Ok(Self {
            definitions,
            order,
            resolved,
            diagnostics,
        })
    }

    /// Requires every selected binding to support synchronous construction.
    ///
    /// Checks the complete selected order without invoking a factory or alias
    /// projector, so callers can reject asynchronous construction before any
    /// component starts.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` when all selected bindings are synchronous.
    ///
    /// # Errors
    ///
    /// Returns [`BuildError::AsyncRequired`] for the first ordinary or managed
    /// asynchronous factory in dependency-first order, retaining its definition
    /// source and binding key.
    pub(crate) fn require_sync(&self) -> Result<(), BuildError> {
        for location in &self.order {
            let definition = &self.definitions[location.definition];
            let binding = &definition.bindings[location.binding];
            if matches!(
                binding.kind,
                PendingBindingKind::AsyncFactory(_) | PendingBindingKind::ManagedAsyncFactory(_)
            ) {
                return Err(BuildError::AsyncRequired {
                    definition: definition.source,
                    key: binding.key.clone(),
                });
            }
        }
        Ok(())
    }
}

/// Rejects duplicate keys and ambiguous primaries among reachable bindings.
///
/// Requested-root validation defers both checks until reachability is known, so
/// only bindings selected from those roots are inspected. The first duplicate
/// in registration order is reported, and the primary check runs over the same
/// reachable subset.
///
/// # Parameters
///
/// * `nodes` - Flattened graph nodes in registration order.
/// * `reachable` - One flag per `nodes` entry marking selection from the roots.
///
/// # Returns
///
/// `Ok(())` when the reachable bindings have unique keys and at most one
/// primary binding per Rust type namespace.
///
/// # Errors
///
/// Returns [`BuildError::DuplicateBinding`] for the first repeated exact key,
/// naming both definition sources, or [`BuildError::MultiplePrimaryBindings`]
/// with every conflicting primary source in a shared type namespace.
// BuildError carries public candidate sets and complete dependency paths.
#[allow(clippy::result_large_err)]
fn validate_reachable_bindings(nodes: &[Node], reachable: &[bool]) -> Result<(), BuildError> {
    let mut sources = HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        if !reachable[index] {
            continue;
        }
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
        .filter(|(index, _)| reachable[*index])
        .map(|(_, node)| node.clone())
        .collect();
    validate_primary(&selected)
}
