// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stores a shutdown ticket until its matching wait callback consumes it.

/// Tracks one request ticket and whether its wait callback has begun.
pub(super) struct TicketState<K> {
    /// Ticket produced by a shutdown request and not yet consumed by waiting.
    pub(super) pending: Option<K>,
    /// Whether the wait callback has started and can accept no further tickets.
    pub(super) wait_started: bool,
}
