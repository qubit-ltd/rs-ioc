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

/// Sendable, `'static` callback that requests component termination once.
///
/// Its result reports callback failure through [`CleanupError`].
pub(crate) type ErasedStop = Box<dyn FnOnce() -> Result<(), CleanupError> + Send + 'static>;
/// Sendable, `'static` callback that creates the future used to observe
/// component termination.
///
/// The returned future reports observation failure through [`CleanupError`].
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
