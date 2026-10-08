// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Single-use session lifecycle.

/// The observable lifecycle of a session's one-shot factories.
pub(super) enum SessionState {
    /// The builder has not been consumed.
    Ready,
    /// A borrowing run future is executing factories.
    Running,
    /// The run future was cancelled or unwound.
    Cancelled,
    /// Construction returned its result and transferred resource ownership.
    Finished,
}
