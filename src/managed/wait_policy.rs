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
#[derive(Clone)]
pub struct WaitPolicy {
    /// Graceful and termination budgets with their runtime's timer factory.
    bounded: Option<(Duration, Duration, DeadlineFactory)>,
}

impl WaitPolicy {
    /// Uses independent per-component grace and termination budgets.
    ///
    /// `timer` creates an owned deadline when waiting begins. It must return
    /// promptly and must be callable from the executor driving shutdown.
    /// Zero budgets expire immediately; a simultaneously ready wait wins.
    #[must_use]
    pub fn bounded<F>(grace: Duration, termination: Duration, timer: F) -> Self
    where
        F: Fn(Duration) -> DeadlineFuture + Send + Sync + 'static,
    {
        Self {
            bounded: Some((grace, termination, Arc::new(timer))),
        }
    }

    /// Explicitly permits waiting forever without creating any timer.
    #[must_use]
    pub fn unbounded() -> Self {
        Self { bounded: None }
    }

    /// Creates the selected budget's timer, or returns `None` when unbounded.
    /// The driver catches a panic from the user-supplied factory.
    pub(crate) fn deadline(&self, graceful: bool) -> Option<DeadlineFuture> {
        self.bounded
            .as_ref()
            .map(|(grace, termination, timer)| timer(if graceful { *grace } else { *termination }))
    }
}
