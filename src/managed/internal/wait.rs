// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Panic isolation and cancellation-safe wait polling.

use std::any::Any;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::task::Context;
use std::task::Poll;

use crate::managed::CleanupError;
use crate::managed::CleanupFuture;
use crate::managed::ErasedWait;

/// Converts a cleanup callback or future panic into a structured cleanup error.
///
/// # Parameters
///
/// * `action` - Lifecycle operation whose unwind was caught.
/// * `payload` - Panic payload returned by `catch_unwind`.
///
/// # Returns
///
/// A cleanup error retaining the panic message in its source.
pub(in crate::managed) fn panic_cleanup_error(action: &'static str, payload: Box<dyn Any + Send>) -> CleanupError {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic payload");
    CleanupError::new(std::io::Error::other(format!("{action} panicked: {message}")))
}

/// Starts one wait callback while converting any callback panic to an error.
///
/// The returned future is still polled separately so panics during polling are
/// attributed to the wait future.
///
/// # Parameters
///
/// * `wait` - One-shot callback that creates the component's wait future.
///
/// # Returns
///
/// The created cleanup future.
///
/// # Errors
///
/// Returns a cleanup error if the callback panics while creating the future.
pub(in crate::managed) fn start_wait(wait: ErasedWait) -> Result<CleanupFuture, CleanupError> {
    catch_unwind(AssertUnwindSafe(wait)).map_err(|payload| panic_cleanup_error("wait callback", payload))
}

/// Polls one retained component future behind the shared unwind boundary.
/// Returns its Pending/Ready state, translating polling panics into Wait
/// errors.
pub(in crate::managed) fn poll_wait_once(
    future: &mut CleanupFuture,
    context: &mut Context<'_>,
) -> Poll<Result<(), CleanupError>> {
    match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context))) {
        Ok(result) => result,
        Err(payload) => Poll::Ready(Err(panic_cleanup_error("wait future", payload))),
    }
}
