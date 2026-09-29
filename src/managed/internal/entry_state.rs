// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Progress for one concrete managed definition.

/// Explicit per-component lifecycle progress retained across cancellation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EntryState {
    /// No lifecycle request has been sent.
    Constructed,
    /// Graceful admission closure was requested successfully.
    GracefulRequested,
    /// Immediate cancellation was requested.
    AbortRequested,
    /// Termination was confirmed by a successful wait or synchronous request.
    Done,
    /// No further observation can establish termination in this attempt.
    Incomplete,
}
