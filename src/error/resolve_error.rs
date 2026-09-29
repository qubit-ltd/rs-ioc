// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Errors raised when querying built component instances.

use thiserror::Error;

use super::InvalidBindingId;
use crate::key::BindingKey;

/// Errors from looking up built components.
///
/// # Examples
///
/// ```
/// use qubit_ioc::ContainerBuilder;
/// use qubit_ioc::ResolveError;
///
/// let context = ContainerBuilder::new().build_all()?;
/// assert!(matches!(context.get::<String>(), Err(ResolveError::MissingComponent { .. })));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Error)]
#[must_use = "resolution errors must be handled or explicitly discarded"]
pub enum ResolveError {
    /// No built binding satisfies the requested type and ID.
    #[error("missing component {request:?}; available: {available:?}")]
    MissingComponent {
        /// Requested type and optional ID.
        request: BindingKey,
        /// Available keys for that type.
        available: Vec<BindingKey>,
    },
    /// Several built bindings satisfy an unnamed request.
    #[error("ambiguous component {request:?}; candidates: {candidates:?}")]
    AmbiguousBinding {
        /// Requested type.
        request: BindingKey,
        /// Matching keys.
        candidates: Vec<BindingKey>,
    },
    /// The caller supplied an invalid ID at lookup time.
    #[error(transparent)]
    InvalidBindingId(
        /// Parse failure retaining the caller's invalid lookup ID.
        #[from]
        InvalidBindingId,
    ),
}
