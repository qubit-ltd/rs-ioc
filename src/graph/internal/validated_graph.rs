// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Validated graph state and root-selection entry points.

use std::any::TypeId;
use std::collections::HashMap;

use crate::binding::PendingBindingKind;
use crate::binding::PendingDefinition;
use crate::binding::profile_is_active;
use crate::dependency::Dependency;
use crate::error::BuildError;
use crate::graph::DiagnosticPaths;
use crate::graph::build_all_seeds;
use crate::graph::close_definitions;
use crate::graph::detect_errors_and_cycles;
use crate::graph::exact_keys;
use crate::graph::first_keys;
use crate::graph::flatten;
use crate::graph::internal::binding_location::BindingLocation;
use crate::graph::internal::resolved_dependency::ResolvedDependency;
use crate::graph::resolve_edges;
use crate::graph::select_roots;
use crate::graph::stable_topology;
use crate::graph::validate_primary;

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
            .filter(|definition| profile_is_active(definition.profile.as_deref(), active_profiles))
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
        let seeds = if let Some(roots) = roots {
            select_roots(roots, &nodes, &by_type)?
        } else {
            build_all_seeds(&nodes, &edges)
        };
        let (reachable, diagnostics) = close_definitions(&definitions, &nodes, &edges, &seeds)?;
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
