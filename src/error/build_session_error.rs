// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Errors from a single-use asynchronous build session.

use crate::error::BuildFailure;

/// A construction failure or an attempt to reuse a consumed session.
// The public variant preserves BuildFailure and its unique cleanup owner by value.
#[allow(clippy::large_enum_variant)]
#[must_use]
#[derive(Debug, thiserror::Error)]
pub enum BuildSessionError {
    /// Preserves the original cause, source chain, and rollback ownership.
    #[error(transparent)]
    Build(#[from] BuildFailure),
    /// The previously polled build future was dropped before completion.
    #[error("build session was cancelled")]
    Cancelled,
    /// The session already returned an application or construction failure.
    #[error("build session already finished")]
    AlreadyFinished,
}
