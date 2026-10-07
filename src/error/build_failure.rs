// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Immediate construction failure and uniquely owned deferred rollback.

use std::error::Error;
use std::fmt;
use std::sync::Mutex;

use crate::error::BuildError;
use crate::error::SettledBuildFailure;
use crate::managed::ShutdownHandle;
use crate::managed::ShutdownReport;

/// The original build error and optional ownership of already requested
/// rollback.
///
/// All managed aborts have been requested before this value is returned. Waits
/// begin only when the caller drives `wait_cleanup()` or takes the cleanup
/// handle and drives `wait()`.
/// Dropping the failure never starts waits or repeats completed abort requests.
///
/// # Examples
///
/// ```
/// use qubit_ioc::BuildError;
/// use qubit_ioc::BuildFailure;
///
/// let mut failure = BuildFailure::from(BuildError::NoRootsSelected);
/// assert!(matches!(failure.cause(), BuildError::NoRootsSelected));
/// assert!(failure.take_cleanup().is_none());
/// ```
#[must_use = "inspect the cause and explicitly handle any pending cleanup"]
pub struct BuildFailure {
    /// Original graph, configuration, or factory error with its complete
    /// source.
    cause: Box<BuildError>,
    /// Mutex makes Send-only cleanup callbacks safe to own in a Sync error.
    cleanup: Mutex<Option<ShutdownHandle>>,
}

impl BuildFailure {
    /// Packages the cause with unique rollback ownership, if resources exist.
    ///
    /// # Parameters
    ///
    /// * `cause` - The structured construction error reported to the caller.
    /// * `cleanup` - `Some` only when managed resources still need aborts and
    ///   waits; `None` when construction failed before creating any.
    ///
    /// # Returns
    ///
    /// Returns a failure that owns the boxed cause and the optional cleanup
    /// handle behind its mutex.
    pub(crate) fn new(cause: BuildError, cleanup: Option<ShutdownHandle>) -> Self {
        Self {
            cause: Box::new(cause),
            cleanup: Mutex::new(cleanup),
        }
    }

    /// Returns the original construction error, including paths and sources.
    ///
    /// The borrow stays valid while `self` is borrowed and performs no
    /// allocation and no lock acquisition.
    ///
    /// # Returns
    ///
    /// Returns a shared reference to the boxed [`BuildError`].
    #[inline]
    pub fn cause(&self) -> &BuildError {
        &self.cause
    }

    /// Takes rollback ownership once; no waits start until the caller drives
    /// the returned handle.
    ///
    /// # Returns
    ///
    /// Returns `Some(handle)` the first time it is called on a failure that
    /// still owns managed rollback work. Returns `None` when ownership was
    /// already taken by an earlier call, or when construction failed before
    /// creating any managed resource.
    pub fn take_cleanup(&mut self) -> Option<ShutdownHandle> {
        self.cleanup
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    /// Consumes the failure into its original error and remaining cleanup
    /// owner. No waits are started.
    ///
    /// # Returns
    ///
    /// Returns the original [`BuildError`] together with the still-owned
    /// cleanup handle, or `None` in that second position when no managed
    /// cleanup remains to take.
    pub fn into_parts(self) -> (BuildError, Option<ShutdownHandle>) {
        (
            *self.cause,
            self.cleanup
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Waits for owned rollback while preserving the original build error.
    ///
    /// Returns `Some(report)` after cleanup succeeds or fails, and `None` when
    /// this failure owns no cleanup handle. Cancelling the returned future
    /// leaves the handle and its active wait in this failure for a later call.
    /// Repeated calls after completion return the same final observations.
    pub async fn wait_cleanup(&mut self) -> Option<ShutdownReport> {
        let handle = self
            .cleanup
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_mut()?;
        Some(match handle.wait().await {
            Ok(report) => report,
            Err(error) => error.report().clone(),
        })
    }

    /// Waits for owned rollback and consumes this failure into its final
    /// cause and optional cleanup report.
    ///
    /// A report can describe unsuccessful cleanup; the original build cause
    /// remains available independently. If this future is cancelled while
    /// waiting, already requested aborts remain in effect, but rollback wait
    /// completion is not guaranteed. Use [`Self::wait_cleanup`] when the caller
    /// needs to retain the failure and resume a cancelled wait.
    #[must_use]
    pub async fn settle(mut self) -> SettledBuildFailure {
        let cleanup_report = self.wait_cleanup().await;
        let (cause, _completed_cleanup) = self.into_parts();
        SettledBuildFailure::new(cause, cleanup_report)
    }
}

impl From<BuildError> for BuildFailure {
    /// Wraps a construction error that has no managed cleanup ownership.
    fn from(cause: BuildError) -> Self {
        Self::new(cause, None)
    }
}

impl fmt::Debug for BuildFailure {
    /// Reports the cause and ownership presence without exposing callbacks.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let has_cleanup = self
            .cleanup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some();
        formatter
            .debug_struct("BuildFailure")
            .field("cause", &self.cause)
            .field("has_cleanup", &has_cleanup)
            .finish()
    }
}

impl fmt::Display for BuildFailure {
    /// Describes the original failure and whether cleanup ownership is present.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let has_cleanup = self
            .cleanup
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some();
        write!(
            formatter,
            "build failed: {}; cleanup available: {has_cleanup}",
            self.cause
        )
    }
}

impl Error for BuildFailure {
    /// Keeps the original structured build error at the start of the source
    /// chain.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
