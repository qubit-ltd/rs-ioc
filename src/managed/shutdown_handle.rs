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
///
/// # Examples
///
/// ```
/// use qubit_ioc::Application;
/// use qubit_ioc::ShutdownMode;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut builder = Application::builder();
/// builder.register_instance(std::sync::Arc::new(String::from("hello")))?;
/// builder.root::<String>();
///
/// let application = builder.build()?;
///
/// // Ownership of the cleanup journal moves into the returned handle.
/// let shutdown = application.begin_shutdown(ShutdownMode::Immediate);
/// assert!(shutdown.pending().is_empty());
///
/// // Abandoning observes the outcome without polling a wait future.
/// let report = shutdown.abandon();
/// assert!(report.is_success());
/// # Ok(())
/// # }
/// ```
#[must_use = "await shutdown.wait() or call abandon() to observe shutdown completeness"]
pub struct ShutdownHandle {
    /// Persistent state stays in this owner, never in a borrowing wait future.
    driver: ShutdownDriver,
}

impl ShutdownHandle {
    /// Transfers unique cleanup ownership and sends Immediate requests now.
    ///
    /// This is the only way to build the handle. It publishes the shutting-down
    /// state, takes the cleanup journal out of its previous owner, and starts
    /// abort requests for every unfinished component before returning. Requests
    /// are sent immediately only for [`ShutdownMode::Immediate`]; under
    /// [`ShutdownMode::Graceful`] the first request is deferred until
    /// [`ShutdownHandle::wait`] is polled.
    ///
    /// # Parameters
    ///
    /// * `context` - the shared query and lifecycle state to publish the
    ///   shutting-down transition on; it is retained for the rest of the
    ///   shutdown.
    /// * `cleanup` - the one-shot cleanup journal whose entries this handle
    ///   takes over by value. The caller must not observe it again, because
    ///   every entry and its registered action move into the new driver.
    /// * `policy` - the wait policy retained for the shutdown; deadlines it
    ///   describes are only armed once a wait is actually polled.
    /// * `mode` - the requested shutdown mode, recorded in the report and used
    ///   to decide whether abort requests start now or on the first poll.
    ///
    /// # Returns
    ///
    /// A handle that uniquely owns the taken journal and every future
    /// observation for this shutdown attempt.
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
    /// Deadlines require the supplied timer runtime to be driven and cannot
    /// interrupt blocking callbacks or future polls.
    ///
    /// # Returns
    ///
    /// The stable final report, also returned by every later read of this
    /// shutdown, once every entry is confirmed or abandoned.
    ///
    /// # Errors
    ///
    /// Returns [`ShutdownError`] carrying that same report when a cleanup
    /// callback failed or a component's termination could not be confirmed.
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
    ///
    /// The result is a fresh snapshot collected on every call, so it owns its
    /// own keys and stays valid after this handle is dropped. Reading it
    /// performs no I/O beyond the internal state it already holds.
    ///
    /// # Returns
    ///
    /// The unconfirmed bindings in reverse construction order, which is the
    /// order this shutdown stops them. The vector is empty once every entry is
    /// confirmed or was abandoned, and it never reports the same binding twice.
    #[must_use]
    pub fn pending(&self) -> Vec<BindingKey> {
        self.driver.pending()
    }

    /// Requests unfinished aborts and reports unconfirmed components.
    /// Does not create or poll any new wait or deadline.
    ///
    /// The handle is consumed, so no further observation is possible through
    /// it; dropping the returned report afterwards repeats the same abort
    /// requests without effect.
    ///
    /// # Returns
    ///
    /// The final [`ShutdownReport`] for this attempt, in which every component
    /// that could not be confirmed is listed as incomplete. When this shutdown
    /// already produced a report, for example after a completed
    /// [`ShutdownHandle::wait`], a clone of that retained report is returned
    /// unchanged and no new request is sent, so the caller always observes the
    /// first finalized outcome.
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
