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
use crate::store::ErasedInstance;

mod internal;

pub(crate) use internal::pending_binding::PendingBinding;
pub(crate) use internal::pending_binding_kind::PendingBindingKind;
pub(crate) use internal::pending_definition::PendingDefinition;
pub(crate) use internal::profile::profile_is_active;

/// A sendable future that produces one shared component or a retained factory
/// error.
///
/// The future owns the whole factory body, so the container can move it to
/// whichever thread builds the component and poll it there. Resolving to `Ok`
/// hands back the shared value; resolving to `Err` reports a [`FactoryError`]
/// that keeps the original user error as its source, and the container records
/// the binding as failed without retrying it.
///
/// The produced `Arc<T>` stays shared between the requesting component and the
/// container, and the future itself must be `Send`, so `T` is in practice
/// required to be `Send + Sync` and to outlive `'static`.
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

/// A sendable factory future whose product is already type-erased.
///
/// This is the [`FactoryFuture`] counterpart used once a binding is registered:
/// the concrete `T` is replaced by [`ErasedInstance`], so one erased future
/// type can hold factories of unrelated component types. The `'static` and
/// `Send` bounds keep that erased future movable between threads, and it
/// resolves to either the erased component or a [`FactoryError`].
pub(crate) type ErasedFactoryFuture =
    Pin<Box<dyn Future<Output = Result<ErasedInstance, FactoryError>> + Send + 'static>>;

/// A one-shot synchronous factory invoked during construction.
///
/// The boxed `FnOnce` receives the [`BuildContext`] prepared for the requesting
/// component and returns the erased product immediately, so the calling thread
/// stays blocked for the duration of the call. `FnOnce` lets the closure
/// consume captured state such as configuration read once per build; boxing
/// keeps the signature object-safe and cheap to move between threads, and
/// `'static` lets the container hold the factory past the registration call.
/// Returning `Ok` yields the erased component, returning `Err` a
/// [`FactoryError`].
pub(crate) type SyncFactory = Box<dyn FnOnce(BuildContext) -> Result<ErasedInstance, FactoryError> + Send + 'static>;

/// A one-shot asynchronous factory invoked during construction.
///
/// Instead of awaiting inside the closure, this returns an
/// [`ErasedFactoryFuture`] so the caller starts the work and the runtime drives
/// it on the async executor, keeping a slow factory off the building thread.
/// The `FnOnce` and `'static` bounds mirror [`SyncFactory`], so configuration
/// may still be consumed once per build while the future stays movable.
pub(crate) type AsyncFactory = Box<dyn FnOnce(BuildContext) -> ErasedFactoryFuture + Send + 'static>;

/// The erased component value paired with the cleanup action it registered.
///
/// A managed factory must hand back both halves together, because the container
/// schedules the [`CleanupAction`] exactly once when the application context
/// shuts down and keeps the value reachable until then. Keeping the pair in one
/// alias records that the action belongs to that specific value: it must not be
/// split off, dropped early, or reused for another component.
pub(crate) type ManagedProduct = (ErasedInstance, CleanupAction);

/// A one-shot synchronous factory returning a managed component.
///
/// This is the [`SyncFactory`] counterpart for factories that must run
/// destruction logic. The closure still blocks the building thread and keeps
/// the `FnOnce`, `Send` and `'static` bounds, but its product carries the
/// registered [`CleanupAction`] alongside the erased value.
pub(crate) type ManagedSyncFactory =
    Box<dyn FnOnce(BuildContext) -> Result<ManagedProduct, FactoryError> + Send + 'static>;

/// A sendable future whose product is a managed component.
///
/// This is the [`ErasedFactoryFuture`] counterpart for managed factories: the
/// future must be `Send` and `'static` so the runtime can move it between
/// threads, and it resolves to either the managed product or a
/// [`FactoryError`].
pub(crate) type ErasedManagedFactoryFuture =
    Pin<Box<dyn Future<Output = Result<ManagedProduct, FactoryError>> + Send + 'static>>;

/// A one-shot asynchronous factory returning a managed component.
///
/// This is the [`AsyncFactory`] counterpart for managed factories, keeping the
/// same `FnOnce`, `Send` and `'static` bounds. The returned
/// [`ErasedManagedFactoryFuture`] carries the value together with its
/// [`CleanupAction`], so shutdown is scheduled for the same component that the
/// future built.
pub(crate) type ManagedAsyncFactory = Box<dyn FnOnce(BuildContext) -> ErasedManagedFactoryFuture + Send + 'static>;

/// Projects an already built concrete `Arc` to an interface `Arc`.
///
/// The projector borrows the erased product instead of consuming it, so the
/// container can still hand the original value to the component that requested
/// it. Returning `Some` yields the erased interface view; returning `None`
/// reports that the stored value does not implement the requested interface,
/// which the container surfaces as a binding failure rather than a panic. It is
/// `Send + Sync` because one projector is shared by every resolution of that
/// binding, so implementations must stay cheap and must not block.
pub(crate) type AliasProjector = Box<dyn Fn(&ErasedInstance) -> Option<ErasedInstance> + Send + Sync + 'static>;
