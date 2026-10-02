// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Managed component value with explicit stop and wait actions.

use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::sync::Arc;

use crate::managed::CleanupAction;
use crate::managed::CleanupError;
use crate::managed::CleanupFuture;
use crate::managed::ErasedWait;
use crate::managed::internal::cleanup_action::ErasedStop;
use crate::store::ErasedInstance;

/// One-shot synchronous stop request for a managed component.
type StopCallback<T> = Box<dyn FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static>;

/// A component value paired with explicit application shutdown actions.
///
/// The abort action is synchronous so an in-progress asynchronous build can
/// request shutdown if its future is cancelled. `wait` is optional and runs
/// during an explicit asynchronous shutdown or a returned asynchronous build
/// failure. Return this value from a managed factory so its actions can be
/// tracked. Dropping a managed value before handing it to the container
/// requests abort once, contains any abort panic, and never starts its wait
/// callback.
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
/// let _worker = Managed::synchronous(Arc::new("worker"), |_| Ok(()));
/// ```
#[must_use = "return or register this managed value so its cleanup actions can be tracked"]
pub struct Managed<T: ?Sized + Send + Sync + 'static> {
    /// Shared component transferred to container storage alongside its actions.
    value: Option<Arc<T>>,
    /// One-shot abort callback consumed by ownership transfer or drop.
    stop: Option<StopCallback<T>>,
    /// Optional one-shot request to stop accepting work and drain this
    /// component.
    graceful: Option<StopCallback<T>>,
    /// Optional wait callback invoked during explicit asynchronous cleanup;
    /// dropping an untransferred managed value does not call it.
    wait: Option<Box<dyn FnOnce(Arc<T>) -> CleanupFuture + Send + 'static>>,
}

impl<T: ?Sized + Send + Sync + 'static> Managed<T> {
    /// Creates a managed value whose stop callback completes termination.
    ///
    /// The stop callback runs at most once during shutdown, rollback,
    /// cancellation, or drop before ownership transfer. A successful return
    /// means the component has terminated; there is no later wait action.
    /// During graceful shutdown, an optional graceful callback runs first as
    /// a request, then this stop callback confirms termination. Avoid blocking
    /// indefinitely because both callbacks run synchronously.
    ///
    /// # Type Parameters
    ///
    /// `F` is a sendable one-shot stop action that consumes a shared handle.
    ///
    /// # Parameters
    ///
    /// `value` is the shared component exposed to lookups. `stop` completes
    /// termination and may return a [`CleanupError`].
    ///
    /// # Returns
    ///
    /// A managed component whose stop action confirms termination.
    pub fn synchronous<F>(value: Arc<T>, stop: F) -> Self
    where
        F: FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static,
    {
        Self {
            value: Some(value),
            stop: Some(Box::new(stop)),
            graceful: None,
            wait: None,
        }
    }

    /// Creates a managed value with a stop request and termination wait.
    ///
    /// The abort callback must request termination without blocking. During
    /// explicit asynchronous shutdown, `wait` confirms termination before
    /// dependencies are stopped. Dropping an untransferred value only calls
    /// abort and never starts `wait`.
    ///
    /// # Type Parameters
    ///
    /// `F` is a sendable one-shot abort request.
    /// `W` is a sendable one-shot callback returning the termination future.
    ///
    /// # Parameters
    ///
    /// `value` is the shared component exposed to lookups. `abort` requests
    /// immediate termination and may return a [`CleanupError`]. `wait` returns
    /// a future that resolves when termination has completed.
    ///
    /// # Returns
    ///
    /// A managed component with an explicit termination wait action.
    pub fn asynchronous<F, W>(value: Arc<T>, abort: F, wait: W) -> Self
    where
        F: FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static,
        W: FnOnce(Arc<T>) -> CleanupFuture + Send + 'static,
    {
        Self {
            value: Some(value),
            stop: Some(Box::new(abort)),
            graceful: None,
            wait: Some(Box::new(wait)),
        }
    }

    /// Creates a managed value with a synchronous abort action.
    ///
    /// The abort closure runs at most once during immediate shutdown, rollback,
    /// cancellation, or drop before ownership transfer. It must only request
    /// cancellation: it must not join, block on a future, wait on a condition
    /// variable, invoke business handlers, or perform unbounded I/O.
    ///
    /// # Type Parameters
    ///
    /// `F` is a sendable one-shot cleanup action that consumes a shared
    /// component handle.
    ///
    /// # Parameters
    ///
    /// `value` is the shared component exposed to lookups. `abort` requests its
    /// immediate termination and may return a [`CleanupError`]. Without a wait
    /// callback, returning success means resource termination is complete.
    ///
    /// # Returns
    ///
    /// A managed component with no asynchronous wait action.
    pub fn new<F>(value: Arc<T>, abort: F) -> Self
    where
        F: FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static,
    {
        Self {
            value: Some(value),
            stop: Some(Box::new(abort)),
            graceful: None,
            wait: None,
        }
    }

    /// Adds a synchronous request for this component to stop accepting work
    /// and drain its existing work during graceful shutdown.
    ///
    /// # Type Parameters
    ///
    /// `F` is a sendable one-shot callback receiving this component's handle.
    ///
    /// # Parameters
    ///
    /// `request` must only request draining of this component, without closing
    /// its dependencies or waiting for termination. It may return a
    /// [`CleanupError`]; shutdown then falls back to abort. Without this
    /// callback, graceful shutdown uses abort for this component. For a
    /// synchronous component, a successful request is followed by its stop
    /// callback, which must confirm termination before shutdown completes.
    ///
    /// # Returns
    ///
    /// This managed component with its graceful request replaced. Drop before
    /// container transfer still invokes only abort and never this callback.
    pub fn with_graceful_stop<F>(mut self, request: F) -> Self
    where
        F: FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static,
    {
        self.graceful = Some(Box::new(request));
        self
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
    pub(crate) fn into_parts(mut self) -> (ErasedInstance, CleanupAction) {
        let value = self.value.take().expect("managed value has not been transferred");
        let stop_value = Arc::clone(&value);
        let stop = self.stop.take().expect("managed abort has not been transferred");
        let erased_stop = Box::new(move || stop(stop_value));
        let graceful = self.graceful.take().map(|request| {
            let graceful_value = Arc::clone(&value);
            Box::new(move || request(graceful_value)) as ErasedStop
        });
        let wait = self.wait.take().map(|wait| {
            let wait_value = Arc::clone(&value);
            Box::new(move || wait(wait_value)) as ErasedWait
        });
        (
            Arc::new(value) as Arc<dyn Any + Send + Sync>,
            CleanupAction {
                stop: Some(erased_stop),
                graceful,
                wait,
            },
        )
    }
}

impl<T: ?Sized + Send + Sync + 'static> Drop for Managed<T> {
    /// Requests abort once before ownership transfer, suppressing callback
    /// errors and panics because drop cannot report them. Never starts waiting.
    fn drop(&mut self) {
        if let (Some(value), Some(abort)) = (self.value.take(), self.stop.take()) {
            let _ = catch_unwind(AssertUnwindSafe(|| abort(value)));
        }
    }
}
