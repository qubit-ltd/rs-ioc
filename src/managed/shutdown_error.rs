// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Failed or incomplete shutdown with retained final observations.

use thiserror::Error;

use crate::managed::ShutdownReport;

/// Final shutdown observations when an action failed or termination is
/// incomplete.
#[derive(Clone, Debug, Error)]
#[error("shutdown observed {} failure(s) and {} incomplete component(s)", report.failures().len(), report.incomplete().len())]
pub struct ShutdownError {
    /// Shared observations including incomplete termination.
    report: ShutdownReport,
}

impl ShutdownError {
    /// Wraps a completed report while retaining its shared error sources.
    pub(crate) fn new(report: ShutdownReport) -> Self {
        Self { report }
    }

    /// Returns the complete observations for the failed shutdown attempt.
    #[must_use]
    pub fn report(&self) -> &ShutdownReport {
        &self.report
    }
}
