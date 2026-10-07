// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Construction failure after owned rollback has been awaited.

use std::error::Error;
use std::fmt;

use crate::error::BuildError;
use crate::managed::ShutdownReport;

/// The original build error and the optional final rollback observation.
///
/// A completed wait can still report cleanup failures. The original build
/// error remains the first source in this error's source chain.
#[derive(Debug)]
#[must_use = "inspect the build cause and any cleanup report"]
pub struct SettledBuildFailure {
    cause: BuildError,
    cleanup_report: Option<ShutdownReport>,
}

impl SettledBuildFailure {
    /// Stores the original build cause and the completed cleanup observation.
    pub(crate) fn new(cause: BuildError, cleanup_report: Option<ShutdownReport>) -> Self {
        Self { cause, cleanup_report }
    }

    /// Returns the original graph, configuration, or factory error.
    #[must_use = "inspect the original build error"]
    #[inline]
    pub fn cause(&self) -> &BuildError {
        &self.cause
    }

    /// Returns `Some` for an awaited rollback and `None` when no managed
    /// cleanup was owned by the failed build.
    #[must_use]
    #[inline]
    pub fn cleanup_report(&self) -> Option<&ShutdownReport> {
        self.cleanup_report.as_ref()
    }
}

impl fmt::Display for SettledBuildFailure {
    /// Shows the original cause and whether cleanup was absent, successful,
    /// or unsuccessful.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cleanup = match &self.cleanup_report {
            None => "no cleanup",
            Some(report) if report.is_success() => "cleanup succeeded",
            Some(_) => "cleanup failed",
        };
        write!(formatter, "build failed: {}; {cleanup}", self.cause)
    }
}

impl Error for SettledBuildFailure {
    /// Keeps the original structured build error first in the source chain.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}
