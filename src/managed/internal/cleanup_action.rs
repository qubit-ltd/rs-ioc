// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Erased abort, graceful request, and wait callbacks retained by the cleanup
//! journal.

use crate::managed::CleanupError;
use crate::managed::CleanupFuture;

/// One-shot type-erased stop action, consumed when shutdown starts.
pub(crate) type ErasedStop = Box<dyn FnOnce() -> Result<(), CleanupError> + Send + 'static>;
/// One-shot type-erased wait callback, consumed when its future is created.
pub(crate) type ErasedWait = Box<dyn FnOnce() -> CleanupFuture + Send + 'static>;

/// Type-erased lifecycle actions for one concrete binding.
pub(crate) struct CleanupAction {
    /// Immediate abort callback, consumed at most once.
    pub(crate) stop: Option<ErasedStop>,
    /// Optional graceful request callback, consumed at most once.
    pub(crate) graceful: Option<ErasedStop>,
    /// Optional wait callback consumed after stop has been attempted.
    pub(crate) wait: Option<ErasedWait>,
}
