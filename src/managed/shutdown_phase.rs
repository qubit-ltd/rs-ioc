// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Lifecycle operation associated with a cleanup failure.

/// Distinguishes request, wait, timeout, and timer failures.
///
/// Every reported failure carries the phase of the lifecycle action that
/// produced it, so callers can tell a failed stop request from an exhausted
/// waiting budget.
///
/// # Examples
///
/// ```
/// use qubit_ioc::Application;
/// use qubit_ioc::CleanupError;
/// use qubit_ioc::Managed;
/// use qubit_ioc::ShutdownMode;
/// use qubit_ioc::ShutdownPhase;
/// use qubit_ioc::WaitPolicy;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut builder = Application::builder().wait_policy(WaitPolicy::unbounded());
/// builder.register_managed_factory::<u32, _>(&[], |_| {
///     Ok(Managed::synchronous(std::sync::Arc::new(1_u32), |_| {
///         Err(CleanupError::new(std::io::Error::other("stop failed")))
///     }))
/// })?;
///
/// let application = builder.build_all()?;
/// let report = application.begin_shutdown(ShutdownMode::Immediate).abandon();
/// assert_eq!(report.failures()[0].phase, ShutdownPhase::Abort);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownPhase {
    /// A graceful request returned an error or panicked.
    RequestGraceful,
    /// An immediate cancellation request failed.
    Abort,
    /// Creating or polling the component wait failed.
    Wait,
    /// The graceful waiting budget expired.
    GracefulWait,
    /// The termination waiting budget expired.
    TerminationWait,
    /// Creating or polling a deadline panicked.
    Deadline,
}
