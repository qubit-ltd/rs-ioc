// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stores a shutdown ticket until its matching wait callback consumes it.

/// Tracks the latest shutdown request ticket and whether waiting has begun.
///
/// # Type Parameters
///
/// `K` is the value produced by a successful shutdown request and consumed by
/// the matching wait callback.
pub(super) struct TicketState<K> {
    /// Latest ticket produced by a successful request and not yet taken by the
    /// wait callback.
    ///
    /// A later successful request replaces this value until waiting begins.
    pub(super) pending: Option<K>,
    /// Whether the wait callback has begun taking the ticket.
    ///
    /// Once set, subsequent request tickets are discarded instead of stored.
    pub(super) wait_started: bool,
}
