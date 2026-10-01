// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runtime-neutral, explicitly selected shutdown deadlines.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// An owned timer polled by the application's executor.
pub type DeadlineFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Shared runtime-neutral factory used only when a bounded wait starts.
type DeadlineFactory = Arc<dyn Fn(Duration) -> DeadlineFuture + Send + Sync>;

/// Explicit shutdown budgets, or an explicit choice to wait indefinitely.
///
/// Bounded policies depend on the supplied runtime driving its timers. They
/// cannot interrupt blocking callbacks or blocking future polls.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
///
/// use qubit_ioc::Application;
/// use qubit_ioc::ShutdownMode;
/// use qubit_ioc::WaitPolicy;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // A real runtime supplies its own timer factory; this one only shows the
/// // contract that the returned future must be owned and polled by the caller.
/// let policy = WaitPolicy::bounded(
///     Duration::from_millis(500),
///     Duration::from_millis(200),
///     |budget| Box::pin(async move { std::thread::sleep(budget) }),
/// );
///
/// let mut builder = Application::builder().wait_policy(policy);
/// builder.register_instance(std::sync::Arc::new(String::from("hello")))?;
/// builder.root::<String>();
///
/// let application = builder.build()?;
/// let report = application.begin_shutdown(ShutdownMode::Graceful).abandon();
/// assert!(report.is_complete());
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct WaitPolicy {
    /// Graceful and termination budgets with their runtime's timer factory.
    bounded: Option<(Duration, Duration, DeadlineFactory)>,
    /// Optional whole-shutdown budget, started by the first `wait` poll.
    overall: Option<Duration>,
}

impl WaitPolicy {
    /// Uses independent per-component grace and termination budgets.
    ///
    /// `timer` creates an owned deadline when waiting begins. It must return
    /// promptly and must be callable from the executor driving shutdown.
    /// Zero budgets expire immediately; a simultaneously ready wait wins.
    ///
    /// # Type Parameters
    ///
    /// `F` is the supplied timer factory. It is shared behind an `Arc` so a
    /// cloned policy keeps the same factory without re-boxing it.
    ///
    /// # Parameters
    ///
    /// * `grace` - budget granted to a graceful stop request before the wait is
    ///   abandoned in favour of termination.
    /// * `termination` - budget granted to the termination wait.
    /// * `timer` - creates the owned deadline future for the selected budget.
    ///
    /// # Returns
    ///
    /// A policy that creates one deadline per waiting phase through `timer`.
    #[must_use]
    pub fn bounded<F>(grace: Duration, termination: Duration, timer: F) -> Self
    where
        F: Fn(Duration) -> DeadlineFuture + Send + Sync + 'static,
    {
        Self {
            bounded: Some((grace, termination, Arc::new(timer))),
            overall: None,
        }
    }

    /// Uses per-component graceful and termination budgets plus one total
    /// shutdown budget.
    ///
    /// The total budget starts when `ShutdownHandle::wait` is first polled and
    /// continues across cancellation and abort upgrades. On expiry, the driver
    /// requests abort for every unfinished component and reports any component
    /// whose termination remains unconfirmed. A timer cannot interrupt a
    /// blocking callback or future poll.
    ///
    /// # Parameters
    ///
    /// * `grace` - budget for each component's graceful wait.
    /// * `termination` - budget for each component's termination wait.
    /// * `total` - budget for the complete shutdown attempt.
    /// * `timer` - creates owned deadline futures on the executor polling
    ///   shutdown.
    ///
    /// # Returns
    ///
    /// A bounded policy with per-component and whole-shutdown deadlines.
    #[must_use]
    pub fn bounded_with_total<F>(grace: Duration, termination: Duration, total: Duration, timer: F) -> Self
    where
        F: Fn(Duration) -> DeadlineFuture + Send + Sync + 'static,
    {
        Self {
            bounded: Some((grace, termination, Arc::new(timer))),
            overall: Some(total),
        }
    }

    /// Explicitly permits waiting forever without creating any timer.
    ///
    /// # Returns
    ///
    /// A policy that never starts a timer and never expires a wait budget.
    #[must_use]
    #[inline]
    pub fn unbounded() -> Self {
        Self {
            bounded: None,
            overall: None,
        }
    }

    /// Creates the selected budget's timer, or returns `None` when unbounded.
    /// The driver catches a panic from the user-supplied factory.
    ///
    /// # Parameters
    ///
    /// `graceful` selects the graceful budget when `true` and the termination
    /// budget otherwise.
    ///
    /// # Returns
    ///
    /// * `Some(future)` - the owned deadline for the selected budget, which the
    ///   caller must poll to observe the expiry.
    /// * `None` - the policy is unbounded, so no timer is created and the
    ///   caller keeps waiting on the component alone.
    #[must_use]
    pub(crate) fn deadline(&self, graceful: bool) -> Option<DeadlineFuture> {
        self.bounded
            .as_ref()
            .map(|(grace, termination, timer)| timer(if graceful { *grace } else { *termination }))
    }

    /// Creates the total shutdown timer if this policy has one.
    /// The driver catches a panic from the user-supplied factory.
    pub(crate) fn overall_deadline(&self) -> Option<DeadlineFuture> {
        let duration = self.overall?;
        let (_, _, timer) = self.bounded.as_ref()?;
        Some(timer(duration))
    }
}
