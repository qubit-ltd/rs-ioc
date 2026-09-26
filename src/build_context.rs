// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Restricted access to dependencies selected before factory execution.

use std::sync::Arc;
use std::sync::RwLock;

use crate::dependency::Dependency;
use crate::error::BuildAccessError;
use crate::graph::ResolvedDependency;
use crate::key::BindingKey;
use crate::options::DefinitionSource;
use crate::store::InstanceStore;

/// Gives one factory access only to the requests it declared at registration.
pub struct BuildContext {
    store: Arc<RwLock<InstanceStore>>,
    source: DefinitionSource,
    resolved: Vec<ResolvedDependency>,
}

// BuildAccessError preserves the rejected request and its definition source.
#[allow(clippy::result_large_err)]
impl BuildContext {
    /// Creates a factory view from the validated requests for one definition.
    pub(crate) fn new(
        store: Arc<RwLock<InstanceStore>>,
        source: DefinitionSource,
        resolved: Vec<ResolvedDependency>,
    ) -> Self {
        Self {
            store,
            source,
            resolved,
        }
    }

    /// Clones the `Arc<T>` selected for a declared required request.
    ///
    /// Returns [`BuildAccessError`] if the factory did not declare this exact
    /// type and cardinality. Graph validation guarantees a built target.
    pub fn get<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Arc<T>, BuildAccessError> {
        let request = Dependency::of::<T>();
        let keys = self.keys_for(&request)?;
        Ok(self.read::<T>(&keys[0]))
    }

    /// Clones the `Arc<T>` selected for a declared required request with `id`.
    ///
    /// An undeclared request returns [`BuildAccessError`]; registration
    /// validates ID syntax before a factory can run.
    pub fn get_by_id<T: ?Sized + Send + Sync + 'static>(&self, id: &str) -> Result<Arc<T>, BuildAccessError> {
        let request = Dependency::with_id::<T>(id);
        let keys = self.keys_for(&request)?;
        Ok(self.read::<T>(&keys[0]))
    }

    /// Returns `Some` for a selected optional request and `None` when it had no
    /// candidate; returns an error if the optional request was not declared.
    pub fn try_get<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Option<Arc<T>>, BuildAccessError> {
        let request = Dependency::optional::<T>();
        let keys = self.keys_for(&request)?;
        Ok(keys.first().map(|key| self.read::<T>(key)))
    }

    /// Returns a selected optional `T` by exact `id`, or `None` if absent.
    ///
    /// An undeclared optional request returns [`BuildAccessError`].
    pub fn try_get_by_id<T: ?Sized + Send + Sync + 'static>(
        &self,
        id: &str,
    ) -> Result<Option<Arc<T>>, BuildAccessError> {
        let request = Dependency::optional_with_id::<T>(id);
        let keys = self.keys_for(&request)?;
        Ok(keys.first().map(|key| self.read::<T>(key)))
    }

    /// Clones every selected `Arc<T>` in graph order for a declared collection.
    ///
    /// An empty declared collection returns an empty vector; an undeclared
    /// collection returns [`BuildAccessError`].
    pub fn get_all<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Vec<Arc<T>>, BuildAccessError> {
        let request = Dependency::all::<T>();
        let keys = self.keys_for(&request)?;
        Ok(keys.iter().map(|key| self.read::<T>(key)).collect())
    }

    /// Finds only the exact request whose keys were fixed during graph
    /// validation.
    fn keys_for(&self, request: &Dependency) -> Result<&[BindingKey], BuildAccessError> {
        self.resolved
            .iter()
            .find(|entry| &entry.request == request)
            .map(|entry| entry.keys.as_slice())
            .ok_or_else(|| BuildAccessError::UndeclaredDependency {
                definition: self.source,
                dependency: request.clone(),
            })
    }

    /// Clones a graph-guaranteed value without exposing the mutable store.
    fn read<T: ?Sized + Send + Sync + 'static>(&self, key: &BindingKey) -> Arc<T> {
        self.store
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get::<T>(key)
            .expect("validated dependency must be constructed before its factory")
    }
}
