// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Errors from a single-use asynchronous build session.

use crate::error::BuildFailure;

/// Reports a failed asynchronous build session while preserving its outcome.
///
/// A session can produce the original construction failure, be cancelled
/// before completion, or reject a later attempt to poll it after it has
/// finished. The error does not restart the build or take ownership away from
/// the original failure.
///
/// # Examples
///
/// ```
/// use qubit_ioc::BuildError;
/// use qubit_ioc::BuildFailure;
/// use qubit_ioc::BuildSessionError;
///
/// let error = BuildSessionError::from(BuildFailure::from(
///     BuildError::NoRootsSelected,
/// ));
/// assert!(matches!(error, BuildSessionError::Build(_)));
/// ```
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
