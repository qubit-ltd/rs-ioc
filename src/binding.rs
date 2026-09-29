// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal binding definitions and erased factories.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::build_context::BuildContext;
use crate::error::FactoryError;
use crate::managed::CleanupAction;
use crate::managed::Managed;
use crate::store::ErasedInstance;

mod internal;

pub(crate) use internal::pending_binding::PendingBinding;
pub(crate) use internal::pending_binding_kind::PendingBindingKind;
pub(crate) use internal::pending_definition::PendingDefinition;
pub(crate) use internal::profile::profile_is_active;

/// A sendable future that produces one shared component or a retained factory
/// error.
///
/// # Type Parameters
///
/// `T` is the concrete or trait-object value returned by the factory.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::FactoryFuture;
///
/// fn make_value() -> FactoryFuture<u32> {
///     Box::pin(async { Ok(Arc::new(42)) })
/// }
/// ```
pub type FactoryFuture<T> = Pin<Box<dyn Future<Output = Result<Arc<T>, FactoryError>> + Send + 'static>>;

/// A sendable factory future returning one erased complete `Arc<T>`.
pub(crate) type ErasedFactoryFuture =
    Pin<Box<dyn Future<Output = Result<ErasedInstance, FactoryError>> + Send + 'static>>;

/// The one-shot synchronous factory signature used during construction.
pub(crate) type SyncFactory = Box<dyn FnOnce(BuildContext) -> Result<ErasedInstance, FactoryError> + Send + 'static>;

/// The one-shot asynchronous factory signature used during construction.
pub(crate) type AsyncFactory = Box<dyn FnOnce(BuildContext) -> ErasedFactoryFuture + Send + 'static>;

/// A type-erased managed value and its lifecycle actions.
pub(crate) type ManagedProduct = (ErasedInstance, CleanupAction);

/// A one-shot synchronous factory returning a managed component.
pub(crate) type ManagedSyncFactory =
    Box<dyn FnOnce(BuildContext) -> Result<ManagedProduct, FactoryError> + Send + 'static>;

/// A sendable future returning a managed component.
pub(crate) type ErasedManagedFactoryFuture =
    Pin<Box<dyn Future<Output = Result<ManagedProduct, FactoryError>> + Send + 'static>>;

/// A one-shot asynchronous factory returning a managed component.
pub(crate) type ManagedAsyncFactory = Box<dyn FnOnce(BuildContext) -> ErasedManagedFactoryFuture + Send + 'static>;

/// Projects an already built concrete `Arc` to an interface `Arc`.
pub(crate) type AliasProjector = Box<dyn Fn(&ErasedInstance) -> Option<ErasedInstance> + Send + Sync + 'static>;
