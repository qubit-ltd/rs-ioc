// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Aggregate of failures observed during application shutdown.

use std::sync::Arc;

use thiserror::Error;

use crate::managed::ShutdownFailure;

/// All failures observed while stopping and waiting for managed components.
///
/// # Examples
///
/// ```
/// use qubit_ioc::ShutdownError;
///
/// let error = ShutdownError::new(Vec::new());
/// assert!(error.failures().is_empty());
/// ```
#[derive(Clone, Debug, Error)]
#[error("{} component cleanup action(s) failed", failures.len())]
pub struct ShutdownError {
    /// Cleanup failures in the order their actions were attempted.
    failures: Arc<[ShutdownFailure]>,
}

impl ShutdownError {
    /// Creates a shutdown error from all observed stop and wait failures.
    ///
    /// # Parameters
    ///
    /// `failures` contains cleanup failures in the order their actions ran.
    ///
    /// # Returns
    ///
    /// An error that can be cloned without cloning its original error sources.
    pub fn new(failures: Vec<ShutdownFailure>) -> Self {
        Self {
            failures: Arc::from(failures),
        }
    }

    /// Returns every recorded cleanup failure in action order.
    #[must_use]
    pub fn failures(&self) -> &[ShutdownFailure] {
        &self.failures
    }
}
