// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Construction failure after explicitly awaiting its optional rollback.

use std::error::Error;
use std::fmt;

use crate::error::BuildError;
use crate::managed::ShutdownReport;

/// The original build error and the observed rollback report, when rollback
/// was available and awaited.
///
/// A failed rollback remains in `cleanup_report`; it never replaces the
/// construction error or its source chain.
#[derive(Debug)]
pub struct SettledBuildFailure {
    cause: BuildError,
    cleanup_report: Option<ShutdownReport>,
}

impl SettledBuildFailure {
    /// Retains the original cause and optional completed rollback observation.
    pub(crate) fn new(cause: BuildError, cleanup_report: Option<ShutdownReport>) -> Self {
        Self { cause, cleanup_report }
    }

    /// Returns the original construction error with its source chain intact.
    pub fn cause(&self) -> &BuildError {
        &self.cause
    }

    /// Returns `Some` for an awaited rollback, including one that failed, or
    /// `None` when construction had no cleanup handle.
    pub fn cleanup_report(&self) -> Option<&ShutdownReport> {
        self.cleanup_report.as_ref()
    }

    /// Consumes the settled failure into its original error and optional
    /// completed rollback report.
    pub fn into_parts(self) -> (BuildError, Option<ShutdownReport>) {
        (self.cause, self.cleanup_report)
    }
}

impl fmt::Display for SettledBuildFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "build failed: {}", self.cause)
    }
}

impl Error for SettledBuildFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.cause)
    }
}
