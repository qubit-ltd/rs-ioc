// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Adapts typed shutdown request tickets to the existing cleanup callbacks.

use std::sync::Arc;
use std::sync::Mutex;

use super::Managed;
use crate::managed::CleanupError;
use crate::managed::CleanupFuture;

/// A request callback returning one ticket for the later wait callback.
pub(super) type TicketRequest<T, K> = Box<dyn FnOnce(Arc<T>) -> Result<K, CleanupError> + Send + 'static>;

/// The ticket waiting for consumption, plus whether observation has started.
struct TicketState<K> {
    pending: Option<K>,
    wait_started: bool,
}

/// Stores a request ticket or returns it for disposal when waiting already
/// began. Any replaced ticket is dropped by the caller after the lock is
/// released.
fn store<K>(state: &Mutex<TicketState<K>>, ticket: K) {
    let discarded = {
        let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.wait_started {
            Some(ticket)
        } else {
            state.pending.replace(ticket)
        }
    };
    drop(discarded);
}

/// Takes the pending ticket and marks the wait callback as started.
/// Returns `None` when every request failed to produce a ticket.
fn take<K>(state: &Mutex<TicketState<K>>) -> Option<K> {
    let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.wait_started = true;
    state.pending.take()
}

/// Converts typed requests and wait into ordinary managed cleanup actions.
/// Request user code runs before locking; the wait callback runs after taking
/// its ticket and releasing the lock.
pub(super) fn adapt<T, K, A, W>(value: Arc<T>, abort: A, graceful: Option<TicketRequest<T, K>>, wait: W) -> Managed<T>
where
    T: ?Sized + Send + Sync + 'static,
    K: Send + 'static,
    A: FnOnce(Arc<T>) -> Result<K, CleanupError> + Send + 'static,
    W: FnOnce(Arc<T>, K) -> CleanupFuture + Send + 'static,
{
    let state = Arc::new(Mutex::new(TicketState {
        pending: None,
        wait_started: false,
    }));
    let abort_state = Arc::clone(&state);
    let wait_state = Arc::clone(&state);
    let managed = Managed::asynchronous(
        value,
        move |value| {
            let ticket = abort(value)?;
            store(&abort_state, ticket);
            Ok(())
        },
        move |value| match take(&wait_state) {
            Some(ticket) => wait(value, ticket),
            None => Box::pin(async {
                Err(CleanupError::new(std::io::Error::other(
                    "shutdown wait has no request ticket",
                )))
            }),
        },
    );
    if let Some(graceful) = graceful {
        let graceful_state = state;
        managed.with_graceful_stop(move |value| {
            let ticket = graceful(value)?;
            store(&graceful_state, ticket);
            Ok(())
        })
    } else {
        managed
    }
}
