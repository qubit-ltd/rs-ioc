// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! One component binding staged for graph validation.

use std::sync::Arc;

use crate::binding::ErasedFactoryFuture;
use crate::binding::ErasedInstance;
use crate::binding::ErasedManagedFactoryFuture;
use crate::binding::FactoryFuture;
use crate::binding::Managed;
use crate::binding::ManagedProduct;
use crate::binding::PendingBindingKind;
use crate::build_context::BuildContext;
use crate::error::FactoryError;
use crate::key::BindingKey;
use crate::options::DefinitionSource;

/// A single concrete binding or interface alias in a definition.
pub(crate) struct PendingBinding {
    /// Typed identity of this concrete binding or interface alias.
    pub(crate) key: BindingKey,
    /// Whether unnamed dependency requests may prefer this binding.
    pub(crate) primary: bool,
    /// Collection ordering value, compared before ID and source.
    pub(crate) order: i32,
    /// Sources removed by explicit replacement of this key.
    pub(crate) replaced_sources: Vec<DefinitionSource>,
    /// Deferred instance, factory, or interface projection operation.
    pub(crate) kind: PendingBindingKind,
}

impl PendingBinding {
    /// Stages a complete shared instance of `T` under a matching typed `key`.
    pub(crate) fn instance<T: ?Sized + Send + Sync + 'static>(
        key: BindingKey,
        value: Arc<T>,
        primary: bool,
        order: i32,
    ) -> Self {
        Self {
            key,
            primary,
            order,
            replaced_sources: Vec::new(),
            kind: PendingBindingKind::Instance(Arc::new(value)),
        }
    }

    /// Erases the `Arc<T>` produced by a synchronous one-shot `factory`.
    ///
    /// Construction and any factory error occur only when the validated graph
    /// executes this binding.
    pub(crate) fn sync_factory<T, F>(key: BindingKey, primary: bool, order: i32, factory: F) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> Result<Arc<T>, FactoryError> + Send + 'static,
    {
        let erased = move |context: BuildContext| -> Result<ErasedInstance, FactoryError> {
            factory(context).map(|value| Arc::new(value) as ErasedInstance)
        };
        Self {
            key,
            primary,
            order,
            replaced_sources: Vec::new(),
            kind: PendingBindingKind::SyncFactory(Box::new(erased)),
        }
    }

    /// Erases the `Arc<T>` produced by an asynchronous one-shot `factory`.
    ///
    /// The returned future owns its data and is `Send`; construction and errors
    /// occur only when a caller polls it during asynchronous building.
    pub(crate) fn async_factory<T, F>(key: BindingKey, primary: bool, order: i32, factory: F) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> FactoryFuture<T> + Send + 'static,
    {
        let erased = move |context: BuildContext| -> ErasedFactoryFuture {
            let future = factory(context);
            Box::pin(async move { future.await.map(|value| Arc::new(value) as ErasedInstance) })
        };
        Self {
            key,
            primary,
            order,
            replaced_sources: Vec::new(),
            kind: PendingBindingKind::AsyncFactory(Box::new(erased)),
        }
    }

    /// Erases the value and lifecycle actions returned by a synchronous
    /// factory.
    pub(crate) fn managed_sync_factory<T, F>(key: BindingKey, primary: bool, order: i32, factory: F) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> Result<Managed<T>, FactoryError> + Send + 'static,
    {
        let erased = move |context: BuildContext| -> Result<ManagedProduct, FactoryError> {
            factory(context).map(Managed::into_parts)
        };
        Self {
            key,
            primary,
            order,
            replaced_sources: Vec::new(),
            kind: PendingBindingKind::ManagedSyncFactory(Box::new(erased)),
        }
    }

    /// Erases the value and lifecycle actions returned by an asynchronous
    /// factory.
    pub(crate) fn managed_async_factory<T, F>(key: BindingKey, primary: bool, order: i32, factory: F) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        F: FnOnce(BuildContext) -> crate::managed::ManagedFactoryFuture<T> + Send + 'static,
    {
        let erased = move |context: BuildContext| -> ErasedManagedFactoryFuture {
            let future = factory(context);
            Box::pin(async move { future.await.map(Managed::into_parts) })
        };
        Self {
            key,
            primary,
            order,
            replaced_sources: Vec::new(),
            kind: PendingBindingKind::ManagedAsyncFactory(Box::new(erased)),
        }
    }

    /// Stages an alias from the concrete `target` to interface type `U`.
    ///
    /// `project` converts a cloned `Arc<T>` into an `Arc<U>` without rebuilding
    /// the component. A mismatched source type yields `None` during projection.
    pub(crate) fn alias<T, U, F>(key: BindingKey, target: BindingKey, primary: bool, order: i32, project: F) -> Self
    where
        T: ?Sized + Send + Sync + 'static,
        U: ?Sized + Send + Sync + 'static,
        F: Fn(Arc<T>) -> Arc<U> + Send + Sync + 'static,
    {
        let erased = move |value: &ErasedInstance| -> Option<ErasedInstance> {
            let concrete = value.downcast_ref::<Arc<T>>()?;
            Some(Arc::new(project(Arc::clone(concrete))))
        };
        Self {
            key,
            primary,
            order,
            replaced_sources: Vec::new(),
            kind: PendingBindingKind::Alias {
                target,
                project: Box::new(erased),
            },
        }
    }

    /// Projects an alias from `value`, returning `None` for a non-alias or type
    /// mismatch.
    #[cfg(test)]
    pub(crate) fn project_alias(&self, value: &ErasedInstance) -> Option<ErasedInstance> {
        match &self.kind {
            PendingBindingKind::Alias { project, .. } => project(value),
            _ => None,
        }
    }
}
