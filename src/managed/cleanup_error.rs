// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Error wrapper for a component cleanup callback.

use std::error::Error;

use thiserror::Error;

/// An error returned by a component stop or wait action.
///
/// # Examples
///
/// ```
/// use qubit_ioc::CleanupError;
///
/// let error = CleanupError::new(std::io::Error::other("stop failed"));
/// assert!(std::error::Error::source(&error).is_some());
/// ```
#[derive(Debug, Error)]
#[error("component cleanup failed: {source}")]
pub struct CleanupError {
    /// The original cleanup error.
    #[source]
    source: Box<dyn Error + Send + Sync + 'static>,
}

impl CleanupError {
    /// Wraps a cleanup error while retaining its source chain.
    ///
    /// # Parameters
    ///
    /// `source` is the original error produced by a stop or wait action.
    ///
    /// # Returns
    ///
    /// A cleanup error whose source chain contains `source`.
    pub fn new<E: Error + Send + Sync + 'static>(source: E) -> Self {
        Self {
            source: Box::new(source),
        }
    }
}
