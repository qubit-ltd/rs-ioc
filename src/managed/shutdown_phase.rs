// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Lifecycle operation associated with a cleanup failure.

/// Distinguishes request, wait, timeout, and timer failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownPhase {
    /// A graceful request returned an error or panicked.
    RequestGraceful,
    /// An immediate cancellation request failed.
    Abort,
    /// Creating or polling the component wait failed.
    Wait,
    /// The graceful waiting budget expired.
    GracefulWait,
    /// The termination waiting budget expired.
    TerminationWait,
    /// Creating or polling a deadline panicked.
    Deadline,
}
