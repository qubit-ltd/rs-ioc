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

#[path = "ticket.rs"]
mod ticket;
#[path = "internal/ticket_state.rs"]
mod ticket_state;

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

    /// Creates a synchronous managed value with a graceful shutdown request.
    ///
    /// During graceful shutdown, `graceful` requests termination before
    /// `stop` confirms it. Both callbacks run synchronously and at most once.
    ///
    /// # Type Parameters
    ///
    /// `F` is a sendable one-shot stop action. `G` is a sendable one-shot
    /// graceful request.
    ///
    /// # Parameters
    ///
    /// `value` is the shared component exposed to lookups. `stop` completes
    /// termination. `graceful` requests graceful termination. Each callback
    /// may return a [`CleanupError`].
    ///
    /// # Returns
    ///
    /// A managed component with a graceful request followed by a stop action.
    pub fn synchronous_with_graceful<F, G>(value: Arc<T>, stop: F, graceful: G) -> Self
    where
        F: FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static,
        G: FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static,
    {
        let mut managed = Self::synchronous(value, stop);
        managed.graceful = Some(Box::new(graceful));
        managed
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

    /// Creates an asynchronous managed value with a graceful shutdown request.
    ///
    /// During graceful shutdown, `graceful` requests termination before
    /// `wait` confirms it; dependencies are stopped only after that wait
    /// completes. Dropping an untransferred value only calls `abort`.
    ///
    /// # Type Parameters
    ///
    /// `F` is a sendable one-shot abort request. `G` is a sendable one-shot
    /// graceful request. `W` is a sendable one-shot callback returning the
    /// termination future.
    ///
    /// # Parameters
    ///
    /// `value` is the shared component exposed to lookups. `abort` requests
    /// immediate termination, `graceful` requests graceful termination, and
    /// `wait` returns a future that resolves when termination completes. The
    /// request callbacks may return a [`CleanupError`].
    ///
    /// # Returns
    ///
    /// A managed component with an explicit asynchronous termination wait.
    pub fn asynchronous_with_graceful<F, G, W>(value: Arc<T>, abort: F, graceful: G, wait: W) -> Self
    where
        F: FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static,
        G: FnOnce(Arc<T>) -> Result<(), CleanupError> + Send + 'static,
        W: FnOnce(Arc<T>) -> CleanupFuture + Send + 'static,
    {
        let mut managed = Self::asynchronous(value, abort, wait);
        managed.graceful = Some(Box::new(graceful));
        managed
    }

    /// Creates a managed value whose abort request returns its wait ticket.
    ///
    /// `abort` must request termination without waiting. `wait` consumes the
    /// ticket during explicit shutdown and confirms termination. Dropping an
    /// untransferred value calls only `abort`. The ticket's destructor must not
    /// cancel shutdown, because an unused ticket may be discarded on upgrade.
    /// Use [`Self::asynchronous_with_graceful_ticket`] when a graceful request
    /// is needed so its ticket can be passed to `wait`.
    ///
    /// # Type Parameters
    ///
    /// `K` is the sendable ticket passed from a successful abort request to
    /// `wait`. `A` is a sendable one-shot abort callback, and `W` is a sendable
    /// one-shot callback that waits using the ticket.
    ///
    /// # Parameters
    ///
    /// `value` is the shared component exposed to lookups. `abort` requests
    /// termination and may return a [`CleanupError`]. `wait` observes
    /// termination using the ticket returned by `abort`.
    ///
    /// # Returns
    ///
    /// A managed component whose explicit asynchronous wait uses the abort
    /// request's ticket.
    ///
    /// # Errors
    ///
    /// Request and wait errors are retained in the final shutdown report. If
    /// the request fails, waiting reports the missing ticket without panicking.
    pub fn asynchronous_with_ticket<K, A, W>(value: Arc<T>, abort: A, wait: W) -> Self
    where
        K: Send + 'static,
        A: FnOnce(Arc<T>) -> Result<K, CleanupError> + Send + 'static,
        W: FnOnce(Arc<T>, K) -> CleanupFuture + Send + 'static,
    {
        ticket::adapt(value, abort, None, wait)
    }

    /// Creates a managed value with ticket-producing graceful and abort
    /// requests.
    ///
    /// Graceful shutdown requests its ticket when `wait` is first polled. If
    /// graceful request fails, abort is requested. An abort before the wait
    /// starts replaces any pending graceful ticket; an abort after it starts
    /// preserves the active wait future and discards the new abort ticket.
    /// Neither request may block waiting for termination. Ticket destruction
    /// must not cancel shutdown, including when an unused ticket is discarded.
    /// Once waiting has started, the original graceful ticket must continue to
    /// confirm the resource's **final termination**, even if a later Immediate
    /// request causes that termination. A ticket that only confirms its own
    /// request does not meet this contract. If a borrowed
    /// [`crate::ShutdownHandle::wait`] future is dropped, a later `wait()` call
    /// resumes the same internally retained wait future and deadline. The
    /// adapter's wait future must support that continued observation.
    /// If only the newest request's ticket can confirm termination, keep
    /// upgradeable shared observation state in the resource and use
    /// [`Self::asynchronous_with_graceful`] with a stable wait future instead.
    /// This constructor keeps its graceful callback paired with its ticket;
    /// use [`Self::asynchronous_with_ticket`] when no graceful request is
    /// needed.
    ///
    /// # Type Parameters
    ///
    /// `K` is the sendable ticket passed from a successful request to `wait`.
    /// `A` and `G` are sendable one-shot abort and graceful request callbacks.
    /// `W` is a sendable one-shot callback that waits using the selected
    /// ticket.
    ///
    /// # Parameters
    ///
    /// `value` is the shared component exposed to lookups. `abort` and
    /// `graceful` request termination and may return a [`CleanupError`].
    /// `wait` observes final termination using the ticket from the request
    /// that started the wait.
    ///
    /// # Returns
    ///
    /// A managed component with ticket-producing graceful and abort actions
    /// and a ticket-aware asynchronous wait.
    ///
    /// # Errors
    ///
    /// Request and wait errors are retained in the final shutdown report. If
    /// no request produced a ticket, waiting reports that condition rather than
    /// panicking.
    pub fn asynchronous_with_graceful_ticket<K, A, G, W>(value: Arc<T>, abort: A, graceful: G, wait: W) -> Self
    where
        K: Send + 'static,
        A: FnOnce(Arc<T>) -> Result<K, CleanupError> + Send + 'static,
        G: FnOnce(Arc<T>) -> Result<K, CleanupError> + Send + 'static,
        W: FnOnce(Arc<T>, K) -> CleanupFuture + Send + 'static,
    {
        ticket::adapt(value, abort, Some(Box::new(graceful)), wait)
    }

    /// Converts the value and cleanup actions to type-erased internal storage.
    ///
    /// The component is transferred into erased instance storage, and its
    /// callbacks become a cleanup action that can be invoked by the container.
    ///
    /// # Returns
    ///
    /// The erased component instance and its associated cleanup action.
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
