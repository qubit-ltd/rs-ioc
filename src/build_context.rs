// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Restricted access to dependencies selected before factory execution.

use std::collections::HashMap;
use std::sync::Arc;

use crate::dependency::Dependency;
use crate::error::BuildAccessError;
use crate::graph::ResolvedDependency;
use crate::key::BindingKey;
use crate::options::DefinitionSource;
use crate::store::ErasedInstance;

/// Gives one factory access only to the requests it declared at registration.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::ContainerBuilder;
/// use qubit_ioc::Dependency;
/// use qubit_ioc::FactoryError;
///
/// let mut builder = ContainerBuilder::new();
/// builder.register_instance(Arc::new(7_u32))?;
/// builder.register_factory::<usize, _>(&[Dependency::of::<u32>()], |context| {
///     Ok(Arc::new(*context.get::<u32>().map_err(FactoryError::new)? as usize))
/// })?;
/// let context = builder.build_all()?;
/// assert_eq!(*context.get::<usize>()?, 7);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct BuildContext {
    /// Erased values for the exact dependency keys resolved for this factory.
    values: HashMap<BindingKey, ErasedInstance>,
    /// Definition whose factory is currently running.
    source: DefinitionSource,
    /// Requests and keys declared and resolved before construction.
    resolved: Vec<ResolvedDependency>,
}

// BuildAccessError preserves the rejected request and its definition source.
#[allow(clippy::result_large_err)]
impl BuildContext {
    /// Creates a factory view from the validated requests and their value
    /// snapshot.
    pub(crate) fn new(
        values: HashMap<BindingKey, ErasedInstance>,
        source: DefinitionSource,
        resolved: Vec<ResolvedDependency>,
    ) -> Self {
        Self {
            values,
            source,
            resolved,
        }
    }

    /// Clones the `Arc<T>` selected for a declared required request.
    ///
    /// Returns [`BuildAccessError`] if the factory did not declare this exact
    /// type and cardinality. Graph validation guarantees a built target.
    ///
    /// # Type Parameters
    ///
    /// `T` identifies the declared concrete or trait-object dependency and
    /// must be thread-safe and `'static`.
    ///
    /// # Returns
    ///
    /// A shared handle to the component selected for the declared request.
    ///
    /// # Errors
    ///
    /// Returns [`BuildAccessError`] if this exact required request was not
    /// declared.
    #[must_use = "handle the declared dependency lookup result"]
    pub fn get<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Arc<T>, BuildAccessError> {
        let request = Dependency::of::<T>();
        let keys = self.keys_for(&request)?;
        Ok(self.read::<T>(&keys[0]))
    }

    /// Clones the `Arc<T>` selected for a declared required request with `id`.
    ///
    /// An undeclared request returns [`BuildAccessError`]; registration
    /// validates ID syntax before a factory can run.
    ///
    /// # Type Parameters
    ///
    /// `T` identifies the declared concrete or trait-object dependency and
    /// must be thread-safe and `'static`.
    ///
    /// # Parameters
    ///
    /// `id` is the exact identifier of the declared required request.
    ///
    /// # Returns
    ///
    /// A shared handle to the component selected by `id`.
    ///
    /// # Errors
    ///
    /// Returns [`BuildAccessError`] if the exact request was not declared.
    #[must_use = "handle the declared dependency lookup result"]
    pub fn get_by_id<T: ?Sized + Send + Sync + 'static>(&self, id: &str) -> Result<Arc<T>, BuildAccessError> {
        let request = Dependency::with_id::<T>(id);
        let keys = self.keys_for(&request)?;
        Ok(self.read::<T>(&keys[0]))
    }

    /// Returns `Some` for a selected optional request and `None` when it had no
    /// candidate; returns an error if the optional request was not declared.
    ///
    /// # Type Parameters
    ///
    /// `T` identifies the declared concrete or trait-object dependency and
    /// must be thread-safe and `'static`.
    ///
    /// # Returns
    ///
    /// `Some` contains the selected component; `None` means the declared
    /// optional request had no candidate.
    ///
    /// # Errors
    ///
    /// Returns [`BuildAccessError`] if the optional request was not declared.
    #[must_use = "handle the optional dependency lookup result"]
    pub fn try_get<T: ?Sized + Send + Sync + 'static>(&self) -> Result<Option<Arc<T>>, BuildAccessError> {
        let request = Dependency::optional::<T>();
        let keys = self.keys_for(&request)?;
        Ok(keys.first().map(|key| self.read::<T>(key)))
    }

    /// Returns a selected optional `T` by exact `id`, or `None` if absent.
    ///
    /// An undeclared optional request returns [`BuildAccessError`].
    ///
    /// # Type Parameters
    ///
    /// `T` identifies the declared concrete or trait-object dependency and
    /// must be thread-safe and `'static`.
    ///
    /// # Parameters
    ///
    /// `id` is the exact identifier of the declared optional request.
    ///
    /// # Returns
    ///
    /// `Some` contains the selected component; `None` means no candidate has
    /// the exact ID.
    ///
    /// # Errors
    ///
    /// Returns [`BuildAccessError`] if the exact optional request was not
    /// declared.
    #[must_use = "handle the optional dependency lookup result"]
    pub fn try_get_by_id<T: ?Sized + Send + Sync + 'static>(
        &self,
        id: &str,
    ) -> Result<Option<Arc<T>>, BuildAccessError> {
        let request = Dependency::optional_with_id::<T>(id);
        let keys = self.keys_for(&request)?;
        Ok(keys.first().map(|key| self.read::<T>(key)))
    }

    /// Clones every selected `Arc<T>` for a declared collection, sorted by
    /// binding order, ID, and definition source location.
    ///
    /// An empty declared collection returns an empty vector; an undeclared
    /// collection returns [`BuildAccessError`].
    ///
    /// # Type Parameters
    ///
    /// `T` identifies the declared concrete or trait-object dependency and
    /// must be thread-safe and `'static`.
    ///
    /// # Returns
    ///
    /// Every selected component in ascending binding order, ID, and source
    /// location; ties preserve registration order. An empty declaration result
    /// is an empty vector.
    ///
    /// # Errors
    ///
    /// Returns [`BuildAccessError`] if the collection request was not
    /// declared.
    #[must_use = "handle the collection dependency lookup result"]
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

    /// Clones a graph-guaranteed value from this factory's dependency snapshot.
    fn read<T: ?Sized + Send + Sync + 'static>(&self, key: &BindingKey) -> Arc<T> {
        self.values
            .get(key)
            .and_then(|value| value.downcast_ref::<Arc<T>>())
            .cloned()
            .expect("validated dependency must be constructed before its factory")
    }
}
