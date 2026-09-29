// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Panic isolation and cancellation-safe wait polling.

use std::any::Any;
use std::future::poll_fn;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
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

/// Polls a retained wait future and converts an unwind panic into a cleanup
/// error.
///
/// The future is borrowed so a `Pending` result preserves its state if the
/// caller cancels the surrounding wait operation.
///
/// # Parameters
///
/// * `future` - Retained future for one component's wait action.
///
/// # Returns
///
/// `Ok(())` when the wait future succeeds.
///
/// # Errors
///
/// Returns its cleanup error, or a cleanup error containing a caught panic.
pub(in crate::managed) async fn poll_wait(future: &mut CleanupFuture) -> Result<(), CleanupError> {
    poll_fn(
        |context| match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context))) {
            Ok(Poll::Ready(result)) => Poll::Ready(result),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => Poll::Ready(Err(panic_cleanup_error("wait future", payload))),
        },
    )
    .await
}
