// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component and deadline futures retained between wait calls.

use crate::managed::CleanupFuture;
use crate::managed::DeadlineFuture;
use crate::managed::ShutdownPhase;

/// One active observation, owning the original future and current budget.
pub(super) struct ActiveWait {
    /// Journal entry being observed.
    pub(super) index: usize,
    /// One-shot component wait; never reconstructed on cancellation or upgrade.
    pub(super) future: CleanupFuture,
    /// Current budget, absent only for explicitly unbounded waiting.
    pub(super) deadline: Option<DeadlineFuture>,
    /// Graceful or termination wait budget currently in force.
    pub(super) phase: ShutdownPhase,
}
