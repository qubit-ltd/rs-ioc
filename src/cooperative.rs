// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Private cooperative scheduling helpers for long-running operations.

/// Maximum number of completed work items before an asynchronous operation
/// yields.
pub(crate) const BATCH_SIZE: usize = 64;

/// Yields once to the executor, then completes when polled again.
pub(crate) async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(move |context| {
        if yielded {
            std::task::Poll::Ready(())
        } else {
            yielded = true;
            context.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await
}
