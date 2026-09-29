// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Unique, resumable application shutdown ownership.

use std::future::poll_fn;

use crate::ApplicationContext;
use crate::BindingKey;
use crate::managed::CleanupJournal;
use crate::managed::ShutdownError;
use crate::managed::ShutdownMode;
use crate::managed::ShutdownReport;
use crate::managed::WaitPolicy;
use crate::managed::internal::shutdown_driver::ShutdownDriver;

/// Owns the actions and observations for one application shutdown.
///
/// Cancelling `wait()` preserves its active component future and deadline.
/// Dropping this handle requests unfinished aborts without starting any wait;
/// use `abandon()` to obtain a report of the resulting incomplete termination.
#[must_use = "await shutdown.wait() or call abandon() to observe shutdown completeness"]
pub struct ShutdownHandle {
    /// Persistent state stays in this owner, never in a borrowing wait future.
    driver: ShutdownDriver,
}

impl ShutdownHandle {
    /// Transfers unique cleanup ownership and sends Immediate requests now.
    pub(crate) fn new(
        context: ApplicationContext,
        cleanup: CleanupJournal,
        policy: WaitPolicy,
        mode: ShutdownMode,
    ) -> Self {
        Self {
            driver: ShutdownDriver::new(context, cleanup, policy, mode),
        }
    }

    /// Waits in reverse construction order, resuming any interrupted wait.
    ///
    /// Graceful mode confirms each consumer before stopping its dependencies.
    /// Returns the stable final report, or a [`ShutdownError`] carrying that
    /// report if callbacks failed or termination could not be confirmed.
    /// Deadlines require the supplied timer runtime to be driven and cannot
    /// interrupt blocking callbacks or future polls.
    pub async fn wait(&mut self) -> Result<ShutdownReport, ShutdownError> {
        let report = poll_fn(|context| self.driver.poll(context)).await;
        if report.is_success() {
            Ok(report)
        } else {
            Err(ShutdownError::new(report))
        }
    }

    /// Requests abort on all unfinished components without waiting.
    /// Repeated calls do not repeat requests or reset termination budgets.
    pub fn abort(&mut self) {
        self.driver.abort();
    }

    /// Lists components whose termination has not yet been confirmed.
    #[must_use]
    pub fn pending(&self) -> Vec<BindingKey> {
        self.driver.pending()
    }

    /// Requests unfinished aborts and reports unconfirmed components.
    /// Does not create or poll any new wait or deadline.
    #[must_use]
    pub fn abandon(mut self) -> ShutdownReport {
        self.driver.abandon()
    }
}

impl Drop for ShutdownHandle {
    /// Requests remaining aborts and publishes completeness without waiting.
    fn drop(&mut self) {
        self.driver.abandon();
    }
}
