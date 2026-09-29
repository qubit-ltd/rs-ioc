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
use crate::managed::ShutdownHandle;

/// The original build error and optional ownership of already requested
/// rollback.
///
/// All managed aborts have been requested before this value is returned. Waits
/// begin only when the caller takes the cleanup handle and drives `wait()`.
/// Dropping the failure never starts waits or repeats completed abort requests.
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
    pub(crate) fn new(cause: BuildError, cleanup: Option<ShutdownHandle>) -> Self {
        Self {
            cause: Box::new(cause),
            cleanup: Mutex::new(cleanup),
        }
    }

    /// Returns the original construction error, including paths and sources.
    pub fn cause(&self) -> &BuildError {
        &self.cause
    }

    /// Takes rollback ownership once; returns `None` after it has been taken or
    /// when construction failed before creating any managed resources.
    pub fn take_cleanup(&mut self) -> Option<ShutdownHandle> {
        self.cleanup
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    /// Consumes the failure into its original error and remaining cleanup
    /// owner. No waits are started; `None` means no managed cleanup remains
    /// to take.
    pub fn into_parts(self) -> (BuildError, Option<ShutdownHandle>) {
        (
            *self.cause,
            self.cleanup
                .into_inner()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
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
