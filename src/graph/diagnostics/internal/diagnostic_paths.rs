// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stores compact predecessor data used to reconstruct diagnostic paths.

use super::PathOrigin;
use crate::binding::PendingDefinition;
use crate::graph::BindingLocation;
use crate::key::BindingKey;

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
    ///
    /// Every binding of every definition receives a `None` origin and a
    /// distinct flattened index assigned in definition-then-binding order,
    /// so indices stay dense and the root list starts empty.
    ///
    /// # Parameters
    ///
    /// * `definitions` - Pending definitions whose bindings are laid out.
    #[must_use]
    pub(crate) fn new(definitions: &[PendingDefinition]) -> Self {
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

    /// Returns the flattened index used by iterative validation walks.
    ///
    /// The index is assigned once by [`DiagnosticPaths::new`] and stays stable
    /// for the lifetime of the value; this lookup copies nothing.
    ///
    /// # Parameters
    ///
    /// * `location` - Definition and binding position to translate.
    ///
    /// # Returns
    ///
    /// The dense index assigned to `location` during construction.
    ///
    /// # Panics
    ///
    /// Panics if `location` is outside the definitions passed to
    /// [`DiagnosticPaths::new`], because the layout has no slot for it.
    #[must_use]
    #[inline]
    pub(crate) fn node_index(&self, location: BindingLocation) -> usize {
        self.indices[location.definition][location.binding]
    }

    /// Returns diagnostic roots in the order they were selected.
    ///
    /// The returned slice borrows `self` and stays empty until
    /// [`DiagnosticPaths::record_root`] records the first root.
    #[must_use]
    #[inline]
    pub(crate) fn roots(&self) -> &[BindingLocation] {
        &self.roots
    }

    /// Records `location` as a root only if it has no earlier provenance.
    ///
    /// Provenance is write-once: a location that already has an origin keeps it
    /// and is not appended to the root list a second time.
    ///
    /// # Parameters
    ///
    /// * `location` - Definition and binding position to mark as a root.
    ///
    /// # Returns
    ///
    /// `true` when the location had no provenance and is now a root, `false`
    /// when it was already recorded.
    ///
    /// # Panics
    ///
    /// Panics if `location` is outside the definitions passed to
    /// [`DiagnosticPaths::new`], because the provenance layout has no slot for
    /// it.
    #[must_use]
    pub(crate) fn record_root(&mut self, location: BindingLocation) -> bool {
        let origin = &mut self.origins[location.definition][location.binding];
        if origin.is_some() {
            return false;
        }
        *origin = Some(PathOrigin::Root);
        self.roots.push(location);
        true
    }

    /// Records the first predecessor that selected `location`.
    ///
    /// Later predecessors are ignored so the reconstructed path stays the first
    /// deterministic one, and the root list is left untouched.
    ///
    /// # Parameters
    ///
    /// * `location` - Definition and binding position that was selected.
    /// * `origin` - Predecessor that selected `location`.
    ///
    /// # Returns
    ///
    /// `true` when the origin was stored, `false` when `location` already had
    /// provenance.
    ///
    /// # Panics
    ///
    /// Panics if `location` is outside the definitions passed to
    /// [`DiagnosticPaths::new`], because the provenance layout has no slot for
    /// it.
    #[must_use]
    pub(crate) fn record_from(&mut self, location: BindingLocation, origin: PathOrigin) -> bool {
        let slot = &mut self.origins[location.definition][location.binding];
        if slot.is_some() {
            return false;
        }
        *slot = Some(origin);
        true
    }

    /// Reconstructs the first root path to a selected binding.
    ///
    /// The walk follows predecessor origins until it reaches a root, cloning
    /// each key along the way.
    ///
    /// # Parameters
    ///
    /// * `location` - Selected binding whose provenance path is rebuilt.
    ///
    /// # Returns
    ///
    /// Owned keys ordered from the root down to `location`.
    ///
    /// # Panics
    ///
    /// Panics if `location` has no recorded provenance, because a path only
    /// exists for bindings selected during graph walks.
    #[must_use]
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
