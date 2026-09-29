// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Managed component value with explicit stop and wait actions.

use std::any::Any;
use std::sync::Arc;

use crate::managed::CleanupAction;
use crate::managed::CleanupError;
use crate::managed::CleanupFuture;
use crate::managed::ErasedWait;
use crate::store::ErasedInstance;

/// A component value paired with explicit application shutdown actions.
///
/// `stop` is synchronous so an in-progress asynchronous build can request
/// shutdown if its future is cancelled. `wait` is optional and runs during an
/// explicit asynchronous shutdown or a returned asynchronous build failure.
/// Dropping a context does not invoke either callback; applications begin
/// shutdown explicitly through [`crate::ApplicationContext::begin_shutdown`].
/// Return this value from a managed factory so its actions can be tracked.
/// Dropping a managed value before handing it to the container drops its
/// callbacks without invoking either cleanup action.
///
/// # Type Parameters
///
/// `T` is the thread-safe component value managed by the cleanup actions.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use qubit_ioc::Managed;
///
/// let _worker = Managed::new(Arc::new("worker"), |_| Ok(()));
/// ```
#[must_use = "return or register this managed value so its cleanup actions can be tracked"]
pub struct Managed<T: ?Sized + Send + Sync + 'static> {
    /// Shared component transferred to container storage alongside its actions.
    value: Arc<T>,
    /// One-shot stop callback invoked only after container ownership transfer;
    /// dropping an untransferred managed value does not call it.
    stop: Box<dyn FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static>,
    /// Optional wait callback invoked during explicit asynchronous cleanup;
    /// dropping an untransferred managed value does not call it.
    wait: Option<Box<dyn FnOnce(Arc<T>) -> CleanupFuture + Send + 'static>>,
}

impl<T: ?Sized + Send + Sync + 'static> Managed<T> {
    /// Creates a managed value with a synchronous stop action.
    ///
    /// The stop closure runs once during explicit shutdown or when an
    /// asynchronous build future is cancelled after the factory returns this
    /// value to the container. Dropping it before that transfer does not call
    /// the stop action.
    ///
    /// # Type Parameters
    ///
    /// `F` is a sendable one-shot cleanup action that consumes a shared
    /// component handle.
    ///
    /// # Parameters
    ///
    /// `value` is the shared component exposed to lookups. `stop` releases its
    /// external resources and may return a [`CleanupError`].
    ///
    /// # Returns
    ///
    /// A managed component with no asynchronous wait action.
    pub fn new<F>(value: Arc<T>, stop: F) -> Self
    where
        F: FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static,
    {
        Self {
            value,
            stop: Box::new(stop),
            wait: None,
        }
    }

    /// Adds an asynchronous action that waits for this component to terminate.
    ///
    /// # Type Parameters
    ///
    /// `F` is a sendable one-shot callback that returns the owned wait future.
    ///
    /// # Parameters
    ///
    /// `wait` runs after stop actions during asynchronous shutdown. Dropping
    /// this value before transfer to the container does not call it; dropping
    /// the shutdown handle abandons waiting without starting pending callbacks.
    ///
    /// # Returns
    ///
    /// This managed component with the wait action installed, replacing any
    /// previously configured wait action.
    pub fn with_wait<F>(mut self, wait: F) -> Self
    where
        F: FnOnce(Arc<T>) -> CleanupFuture + Send + 'static,
    {
        self.wait = Some(Box::new(wait));
        self
    }

    /// Converts the value and cleanup actions to type-erased internal storage.
    pub(crate) fn into_parts(self) -> (ErasedInstance, CleanupAction) {
        let value = Arc::clone(&self.value);
        let stop_value = Arc::clone(&value);
        let stop = self.stop;
        let erased_stop = Box::new(move || stop(stop_value));
        let wait = self.wait.map(|wait| {
            let wait_value = value;
            Box::new(move || wait(wait_value)) as ErasedWait
        });
        (
            Arc::new(self.value) as Arc<dyn Any + Send + Sync>,
            CleanupAction {
                stop: Some(erased_stop),
                wait,
            },
        )
    }
}
