// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Retains one compact, deterministic source for each selected binding.

use super::BindingLocation;
use crate::binding::PendingDefinition;
use crate::key::BindingKey;

/// Explains how one selected binding entered the graph closure.
#[derive(Clone, Copy)]
pub(super) enum PathOrigin {
    /// The binding was selected directly by a root request or synthetic seed.
    Root,
    /// The binding was selected through a declared dependency or alias target.
    Dependency(BindingLocation),
    /// The binding was included because another binding selected its
    /// definition.
    DefinitionMember(BindingLocation),
}

/// Stores keys and one predecessor per selected binding instead of full paths.
pub(crate) struct DiagnosticPaths {
    /// Binding keys grouped by their definition and declaration positions.
    keys: Vec<Vec<BindingKey>>,
    /// First deterministic source for each selected key.
    origins: Vec<Vec<Option<PathOrigin>>>,
    /// Flattened node indices, used to start iterative graph walks at roots.
    indices: Vec<Vec<usize>>,
    /// Roots in root-request order, followed by synthetic build-all roots.
    roots: Vec<BindingLocation>,
}

impl DiagnosticPaths {
    /// Creates empty provenance slots matching all active binding locations.
    pub(super) fn new(definitions: &[PendingDefinition]) -> Self {
        let mut keys = Vec::with_capacity(definitions.len());
        let mut origins = Vec::with_capacity(definitions.len());
        let mut indices = Vec::with_capacity(definitions.len());
        let mut next_index = 0;
        for definition in definitions {
            let mut definition_keys = Vec::with_capacity(definition.bindings.len());
            let mut definition_origins = Vec::with_capacity(definition.bindings.len());
            let mut definition_indices = Vec::with_capacity(definition.bindings.len());
            for binding in &definition.bindings {
                definition_keys.push(binding.key.clone());
                definition_origins.push(None);
                definition_indices.push(next_index);
                next_index += 1;
            }
            keys.push(definition_keys);
            origins.push(definition_origins);
            indices.push(definition_indices);
        }
        Self {
            keys,
            origins,
            indices,
            roots: Vec::new(),
        }
    }

    /// Records `location` as a root only if it has no earlier provenance.
    pub(super) fn record_root(&mut self, location: BindingLocation) -> bool {
        let origin = &mut self.origins[location.definition][location.binding];
        if origin.is_some() {
            return false;
        }
        *origin = Some(PathOrigin::Root);
        self.roots.push(location);
        true
    }

    /// Records the first predecessor that selected `location`.
    pub(super) fn record_from(&mut self, location: BindingLocation, origin: PathOrigin) -> bool {
        let slot = &mut self.origins[location.definition][location.binding];
        if slot.is_some() {
            return false;
        }
        *slot = Some(origin);
        true
    }

    /// Returns the flattened index used by iterative validation walks.
    pub(super) fn node_index(&self, location: BindingLocation) -> usize {
        self.indices[location.definition][location.binding]
    }

    /// Returns diagnostic roots in the order they were selected.
    pub(super) fn roots(&self) -> &[BindingLocation] {
        &self.roots
    }

    /// Reconstructs the first root path to a selected binding.
    pub(crate) fn path_to(&self, location: BindingLocation) -> Vec<BindingKey> {
        let mut path = Vec::new();
        let mut current = location;
        loop {
            path.push(self.keys[current.definition][current.binding].clone());
            match self.origins[current.definition][current.binding]
                .expect("selected binding must have a diagnostic origin")
            {
                PathOrigin::Root => break,
                PathOrigin::Dependency(parent) | PathOrigin::DefinitionMember(parent) => current = parent,
            }
        }
        path.reverse();
        path
    }
}
