// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Read-only queries over a successfully built component graph.

use std::any::TypeId;
use std::sync::Arc;
use std::sync::RwLock;

use crate::builder::ContainerBuilder;
use crate::error::ResolveError;
use crate::key::BindingId;
use crate::key::BindingKey;
use crate::options::DefinitionSource;
use crate::store::InstanceStore;

/// Lookup metadata retained for candidate selection after construction.
pub(crate) struct BuiltBinding {
    pub(crate) key: BindingKey,
    pub(crate) primary: bool,
    pub(crate) order: i32,
    pub(crate) source: DefinitionSource,
    pub(crate) replaced_sources: Vec<DefinitionSource>,
}

/// Shared, read-only component context after successful construction.
///
/// Every successful query clones an existing `Arc`; factories never run again.
pub struct ApplicationContext {
    store: Arc<RwLock<InstanceStore>>,
    bindings: Vec<BuiltBinding>,
}

impl ApplicationContext {
    /// Creates a builder for this context.
    pub fn builder() -> ContainerBuilder {
        ContainerBuilder::new()
    }

    /// Returns the active source and earlier sources replaced for one exact
    /// key.
    ///
    /// `None` means that `key` is absent. The returned slice borrows this
    /// immutable context and lists removed active bindings in registration
    /// order.
    pub fn binding_sources(&self, key: &BindingKey) -> Option<(DefinitionSource, &[DefinitionSource])> {
        self.bindings
            .iter()
            .find(|binding| &binding.key == key)
            .map(|binding| (binding.source, binding.replaced_sources.as_slice()))
    }

    /// Publishes a fully constructed store and its active lookup metadata.
    pub(crate) fn new(store: Arc<RwLock<InstanceStore>>, bindings: Vec<BuiltBinding>) -> Self {
        Self { store, bindings }
    }

    /// Returns the only `T`, or the unique primary when several exist.
    ///
    /// Missing and ambiguous selections return structured [`ResolveError`].
    pub fn get<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Arc<T>, ResolveError> {
        let request = BindingKey::of::<T>(None);
        let candidates = self.candidates(TypeId::of::<T>());
        let selected = self.select(&request, &candidates)?;
        Ok(self.read::<T>(&selected.key))
    }

    /// Returns the `T` bound to the exact, case-sensitive `id`.
    ///
    /// Invalid IDs, missing bindings and ambiguous bindings return
    /// [`ResolveError`] with the original request and candidates.
    pub fn get_by_id<T: ?Sized + Send + Sync + 'static>(&self, id: &str) -> Result<Arc<T>, ResolveError> {
        let request = BindingKey::of::<T>(Some(BindingId::parse(id)?));
        let candidates = self.candidates(TypeId::of::<T>());
        let selected = self.select(&request, &candidates)?;
        Ok(self.read::<T>(&selected.key))
    }

    /// Returns `Some` for a unique selected `T`, `None` when absent, or an
    /// ambiguity error when multiple non-primary candidates exist.
    pub fn try_get<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Option<Arc<T>>, ResolveError> {
        let request = BindingKey::of::<T>(None);
        let candidates = self.candidates(TypeId::of::<T>());
        if candidates.is_empty() {
            return Ok(None);
        }
        self.select(&request, &candidates)
            .map(|binding| Some(self.read::<T>(&binding.key)))
    }

    /// Returns all built `T` values sorted by order, ID and source location.
    ///
    /// A type with no bindings yields an empty vector.
    pub fn get_all<T: ?Sized + Send + Sync + 'static>(&self) -> Vec<Arc<T>> {
        let mut candidates = self.candidates(TypeId::of::<T>());
        candidates.sort_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| left.key.id().cmp(&right.key.id()))
                .then_with(|| left.source.cmp(&right.source))
        });
        candidates.iter().map(|binding| self.read::<T>(&binding.key)).collect()
    }

    /// Lists active bindings for one Rust type in registration order.
    fn candidates(&self, type_id: TypeId) -> Vec<&BuiltBinding> {
        self.bindings
            .iter()
            .filter(|binding| binding.key.type_id() == type_id)
            .collect()
    }

    /// Resolves one named or unnamed query from candidates already in this
    /// context.
    fn select<'binding>(
        &self,
        request: &BindingKey,
        candidates: &[&'binding BuiltBinding],
    ) -> Result<&'binding BuiltBinding, ResolveError> {
        let matching: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|binding| request.id().is_none() || binding.key.id() == request.id())
            .collect();
        match matching.as_slice() {
            [] => Err(ResolveError::MissingComponent {
                request: request.clone(),
                available: candidates.iter().map(|binding| binding.key.clone()).collect(),
            }),
            [only] => Ok(only),
            many if request.id().is_none() => {
                let primaries: Vec<_> = many.iter().copied().filter(|binding| binding.primary).collect();
                match primaries.as_slice() {
                    [only] => Ok(only),
                    _ => Err(ResolveError::AmbiguousBinding {
                        request: request.clone(),
                        candidates: many.iter().map(|binding| binding.key.clone()).collect(),
                    }),
                }
            }
            many => Err(ResolveError::AmbiguousBinding {
                request: request.clone(),
                candidates: many.iter().map(|binding| binding.key.clone()).collect(),
            }),
        }
    }

    /// Clones an `Arc<T>` that must exist after successful construction.
    fn read<T: ?Sized + Send + Sync + 'static>(&self, key: &BindingKey) -> Arc<T> {
        self.store
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get::<T>(key)
            .expect("published binding must exist with its registered type")
    }
}
