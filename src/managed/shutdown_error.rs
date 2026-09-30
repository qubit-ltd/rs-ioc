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

/// Failed or incomplete shutdown outcome that keeps the final observations.
///
/// The managed container reports a shutdown problem by returning this error
/// instead of a plain success value, so a caller that only checks for `Ok`
/// silently drops the diagnosis. The error owns the [`ShutdownReport`] produced
/// when the attempt ended, which lets the caller inspect the failing actions
/// and the components that never reached a terminated state instead of
/// re-running the shutdown to obtain them.
///
/// The value is a plain owned snapshot: constructing it performs no I/O and
/// does not block, and reading the retained report through
/// [`ShutdownError::report`] borrows it without copying.
///
/// This type cannot be built through the public API. It is produced by the
/// managed shutdown path and is only ever observed by callers, so its
/// type-level documentation carries no runnable construction example.
///
/// [`ShutdownReport`]: crate::managed::ShutdownReport
#[derive(Clone, Debug, Error)]
#[error("shutdown observed {} failure(s) and {} incomplete component(s)", report.failures().len(), report.incomplete().len())]
pub struct ShutdownError {
    /// Observations captured when the attempt ended.
    ///
    /// The report is stored by value so the error keeps the failing actions and
    /// the components that stayed alive even after every other error source has
    /// been dropped. The derived `Display` message counts this field, so the
    /// retained report is also what that message describes.
    report: ShutdownReport,
}

impl ShutdownError {
    /// Wraps a completed report while retaining its shared error sources.
    ///
    /// This is the only way to build the error. The managed shutdown path calls
    /// it once an attempt has produced a report recording at least one failure
    /// or at least one component that never terminated.
    ///
    /// # Parameters
    ///
    /// * `report` - the observations produced by the finished attempt, taken by
    ///   value so the error owns them and stays valid after the caller drops
    ///   its own handle to that shutdown.
    ///
    /// # Returns
    ///
    /// An error that exposes the given observations through its `Display`
    /// implementation and through [`ShutdownError::report`].
    #[must_use]
    #[inline]
    pub(crate) fn new(report: ShutdownReport) -> Self {
        Self { report }
    }

    /// Returns the complete observations for the failed shutdown attempt.
    ///
    /// The returned reference stays valid for as long as the error is borrowed,
    /// and reading it performs no I/O, no locking, and no allocation.
    ///
    /// # Returns
    ///
    /// A borrowed view of the retained [`ShutdownReport`], covering both the
    /// failing actions and the components that did not terminate.
    #[must_use]
    #[inline]
    pub fn report(&self) -> &ShutdownReport {
        &self.report
    }
}
