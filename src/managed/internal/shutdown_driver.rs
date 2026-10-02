// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Persistent, sequential shutdown state machine.

use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;

use super::active_wait::ActiveWait;
use super::cleanup_action::ErasedStop;
use super::entry_state::EntryState;
use super::wait::panic_cleanup_error;
use super::wait::poll_wait_once;
use super::wait::start_wait;
use crate::ApplicationContext;
use crate::ApplicationState;
use crate::BindingKey;
use crate::managed::CleanupEntry;
use crate::managed::CleanupError;
use crate::managed::CleanupJournal;
use crate::managed::DeadlineFuture;
use crate::managed::ShutdownFailure;
use crate::managed::ShutdownMode;
use crate::managed::ShutdownPhase;
use crate::managed::ShutdownReport;
use crate::managed::WaitPolicy;

/// Owns every action, active future, deadline, and observation for one
/// shutdown.
pub(crate) struct ShutdownDriver {
    /// Shared query storage and lifecycle state, keeping values alive.
    context: ApplicationContext,
    /// Concrete entries in successful construction order.
    entries: Vec<CleanupEntry>,
    /// Explicit budgets selected before construction.
    policy: WaitPolicy,
    /// Original requested mode, retained in the report after upgrades.
    requested: ShutdownMode,
    /// Effective mode for entries whose request has not started.
    mode: ShutdownMode,
    /// Exclusive reverse cursor for entries not yet visited by wait.
    cursor: usize,
    /// Current observation survives cancellation of the borrowing wait future.
    active: Option<ActiveWait>,
    /// Failures in observation order.
    failures: Vec<ShutdownFailure>,
    /// Concrete entries lacking graceful support that used abort instead.
    fallbacks: Vec<BindingKey>,
    /// Whether the first wait poll has initialized the total shutdown timer.
    overall_started: bool,
    /// Timer and failure for the complete shutdown budget.
    overall_deadline: Option<DeadlineFuture>,
    overall_failure: Option<Arc<CleanupError>>,
    /// Immutable result once every entry is finalized or abandoned.
    report: Option<ShutdownReport>,
}

impl ShutdownDriver {
    /// Takes the journal and publishes shutdown before optionally aborting all.
    /// Immediate mode sends every request before this constructor returns.
    pub(crate) fn new(
        context: ApplicationContext,
        mut cleanup: CleanupJournal,
        policy: WaitPolicy,
        mode: ShutdownMode,
    ) -> Self {
        context.publish_state(ApplicationState::ShuttingDown);
        let entries = cleanup.take_entries();
        let cursor = entries.len();
        let mut driver = Self {
            context,
            entries,
            policy,
            requested: mode,
            mode,
            cursor,
            active: None,
            failures: Vec::new(),
            fallbacks: Vec::new(),
            overall_started: false,
            overall_deadline: None,
            overall_failure: None,
            report: None,
        };
        if mode == ShutdownMode::Immediate {
            driver.abort();
        }
        driver
    }

    /// Requests abort on unfinished entries without polling any wait.
    /// A graceful active wait keeps its original future and receives exactly
    /// one termination budget. Repeated calls never restart that budget.
    pub(crate) fn abort(&mut self) {
        if self.report.is_some() {
            return;
        }
        self.mode = ShutdownMode::Immediate;
        for index in (0..self.entries.len()).rev() {
            self.abort_entry(index);
        }
        if let Some(active) = self.active.take() {
            self.restore_active(active);
        }
    }

    /// Returns all bindings whose termination has not been confirmed.
    ///
    /// The keys are collected in reverse construction order on every call, so
    /// the result owns its own storage and stays valid after the driver is
    /// dropped.
    #[must_use]
    pub(crate) fn pending(&self) -> Vec<BindingKey> {
        self.entries
            .iter()
            .rev()
            .filter(|entry| entry.state != EntryState::Done)
            .map(|entry| entry.key.clone())
            .collect()
    }

    /// Aborts and finalizes without starting or polling any pending wait.
    pub(crate) fn abandon(&mut self) -> ShutdownReport {
        if let Some(report) = &self.report {
            return report.clone();
        }
        // Abandonment must not create an upgrade timer; no more waits will run.
        for index in (0..self.entries.len()).rev() {
            self.abort_entry(index);
        }
        self.active = None;
        for entry in &mut self.entries {
            if entry.state != EntryState::Done {
                entry.state = EntryState::Incomplete;
            }
        }
        self.finish()
    }

    /// Drives requests and waits until pending or the final report is ready.
    /// Panics from user callbacks and future polls are retained as failures.
    /// The loop only takes an observation it installed itself in the same
    /// iteration, so its internal invariant never fails.
    pub(crate) fn poll(&mut self, context: &mut Context<'_>) -> Poll<ShutdownReport> {
        if let Some(report) = &self.report {
            return Poll::Ready(report.clone());
        }
        if self.active.is_none() && self.cursor == 0 {
            return Poll::Ready(self.finish());
        }
        if !self.overall_started {
            self.overall_started = true;
            match catch_unwind(AssertUnwindSafe(|| self.policy.overall_deadline())) {
                Ok(deadline) => self.overall_deadline = deadline,
                Err(payload) => {
                    self.overall_failure = Some(Arc::new(panic_cleanup_error("overall deadline factory", payload)));
                    return Poll::Ready(self.expire_overall());
                }
            }
        }
        loop {
            if self.active.is_none() {
                if self.cursor == 0 {
                    return Poll::Ready(self.finish());
                }
                if self.poll_overall_deadline(context).is_ready() {
                    return Poll::Ready(self.expire_overall());
                }
                if !self.start_next() {
                    continue;
                }
            }

            let mut active = self.active.take().expect("active wait installed before polling");
            match poll_wait_once(&mut active.future, context) {
                Poll::Ready(Ok(())) => {
                    self.complete_entry(active.index);
                    continue;
                }
                Poll::Ready(Err(error)) => {
                    self.fail_incomplete(active.index, ShutdownPhase::Wait, error);
                    continue;
                }
                Poll::Pending => {}
            }

            // Component completion wins when both futures are ready in this poll.
            let deadline = active
                .deadline
                .as_mut()
                .map(|deadline| catch_unwind(AssertUnwindSafe(|| deadline.as_mut().poll(context))));
            match deadline {
                None | Some(Ok(Poll::Pending)) => {
                    self.active = Some(active);
                }
                Some(Err(payload)) => self.fail_incomplete(
                    active.index,
                    ShutdownPhase::Deadline,
                    panic_cleanup_error("deadline future", payload),
                ),
                Some(Ok(Poll::Ready(()))) => self.expire(active),
            }
            if let Some(active) = self.active.take() {
                match self.poll_overall_deadline(context) {
                    Poll::Ready(()) => {
                        self.active = Some(active);
                        return Poll::Ready(self.expire_overall());
                    }
                    Poll::Pending => {
                        self.active = Some(active);
                        return Poll::Pending;
                    }
                }
            }
        }
    }

    /// Polls the one retained whole-shutdown timer and stores timer failures.
    fn poll_overall_deadline(&mut self, context: &mut Context<'_>) -> Poll<()> {
        let Some(deadline) = self.overall_deadline.as_mut() else {
            return Poll::Pending;
        };
        match catch_unwind(AssertUnwindSafe(|| deadline.as_mut().poll(context))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(())) => {
                self.overall_failure = Some(Arc::new(CleanupError::new(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "overall shutdown deadline expired",
                ))));
                Poll::Ready(())
            }
            Err(payload) => {
                self.overall_failure = Some(Arc::new(panic_cleanup_error("overall deadline future", payload)));
                Poll::Ready(())
            }
        }
    }

    /// Aborts all unfinished entries without creating any further wait timer.
    fn expire_overall(&mut self) -> ShutdownReport {
        if self.overall_failure.is_none() {
            self.overall_failure = Some(Arc::new(CleanupError::new(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "overall shutdown deadline expired",
            ))));
        }
        for index in (0..self.entries.len()).rev() {
            self.abort_entry(index);
        }
        self.active = None;
        for entry in &mut self.entries {
            if entry.state != EntryState::Done {
                entry.state = EntryState::Incomplete;
            }
        }
        self.finish()
    }

    /// Claims the next entry from the reverse cursor and installs its wait.
    /// Returns `false` when the entry was already finalized, terminated
    /// synchronously, or failed before any wait started, so the caller should
    /// advance to the next entry without polling. Requires `cursor > 0`.
    fn start_next(&mut self) -> bool {
        self.cursor -= 1;
        let index = self.cursor;
        if matches!(self.entries[index].state, EntryState::Done | EntryState::Incomplete) {
            return false;
        }
        if self.mode == ShutdownMode::Graceful {
            self.request_graceful(index);
        }
        if matches!(self.entries[index].state, EntryState::Done | EntryState::Incomplete) {
            return false;
        }
        let Some(wait) = self.entries[index].action.wait.take() else {
            // A successful request with no wait promises synchronous termination.
            self.complete_entry(index);
            return false;
        };
        let future = match start_wait(wait) {
            Ok(future) => future,
            Err(error) => {
                self.fail_incomplete(index, ShutdownPhase::Wait, error);
                return false;
            }
        };
        let graceful = self.entries[index].state == EntryState::GracefulRequested;
        let deadline = match self.deadline(index, graceful) {
            Ok(deadline) => deadline,
            Err(()) => {
                self.abort_entry(index);
                self.entries[index].state = EntryState::Incomplete;
                return false;
            }
        };
        self.active = Some(ActiveWait {
            index,
            future,
            deadline,
            phase: if graceful {
                ShutdownPhase::GracefulWait
            } else {
                ShutdownPhase::TerminationWait
            },
        });
        true
    }

    /// Restores a taken observation, giving a graceful wait one fresh
    /// termination budget and leaving a termination wait unchanged. A budget
    /// factory panic marks the entry incomplete instead of reinstalling the
    /// observation.
    fn restore_active(&mut self, mut active: ActiveWait) {
        if active.phase != ShutdownPhase::GracefulWait {
            self.active = Some(active);
            return;
        }
        active.phase = ShutdownPhase::TerminationWait;
        match self.deadline(active.index, false) {
            Ok(deadline) => {
                active.deadline = deadline;
                self.active = Some(active);
            }
            Err(()) => self.entries[active.index].state = EntryState::Incomplete,
        }
    }

    /// Records one failure, sends abort, and finalizes the entry as
    /// incomplete. The three steps always follow a failed wait or budget, so
    /// no active observation is left installed.
    fn fail_incomplete(&mut self, index: usize, phase: ShutdownPhase, error: CleanupError) {
        self.fail(index, phase, error);
        self.abort_entry(index);
        self.entries[index].state = EntryState::Incomplete;
    }

    /// Handles an expired budget: the failure is recorded in the phase that
    /// owned the budget, abort is sent, and a graceful entry receives one
    /// termination budget instead of being finalized immediately.
    fn expire(&mut self, active: ActiveWait) {
        let index = active.index;
        let phase = active.phase;
        self.fail(
            index,
            phase,
            CleanupError::new(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "component shutdown deadline expired",
            )),
        );
        self.abort_entry(index);
        if phase == ShutdownPhase::GracefulWait {
            self.restore_active(active);
        } else {
            self.entries[index].state = EntryState::Incomplete;
        }
    }

    /// Requests graceful admission closure once, falling back to abort when
    /// unsupported or failed. No wait is started here.
    fn request_graceful(&mut self, index: usize) {
        if self.entries[index].state != EntryState::Constructed {
            return;
        }
        let Some(request) = self.entries[index].action.graceful.take() else {
            self.fallbacks.push(self.entries[index].key.clone());
            self.abort_entry(index);
            return;
        };
        if self.request(index, ShutdownPhase::RequestGraceful, request) {
            if self.entries[index].action.wait.is_none() {
                // The graceful callback only requests termination. Without a
                // wait, the synchronous stop must confirm it before completion.
                self.abort_entry(index);
            } else {
                self.entries[index].state = EntryState::GracefulRequested;
            }
        } else {
            self.abort_entry(index);
        }
    }

    /// Sends one abort, preserving active waits and skipping finalized entries.
    fn abort_entry(&mut self, index: usize) {
        if matches!(
            self.entries[index].state,
            EntryState::Done | EntryState::Incomplete | EntryState::AbortRequested
        ) {
            return;
        }
        let Some(abort) = self.entries[index].action.stop.take() else {
            return;
        };
        let succeeded = self.request(index, ShutdownPhase::Abort, abort);
        // A consumed wait may still be active or may have just failed. Only
        // Constructed entries can prove synchronous termination from no wait.
        let had_wait =
            self.entries[index].action.wait.is_some() || self.entries[index].state == EntryState::GracefulRequested;
        self.entries[index].state = if had_wait {
            EntryState::AbortRequested
        } else if succeeded {
            EntryState::Done
        } else {
            EntryState::Incomplete
        };
    }

    /// Calls a nonblocking request and captures its returned error or unwind.
    /// Returns whether the request completed successfully.
    fn request(&mut self, index: usize, phase: ShutdownPhase, action: ErasedStop) -> bool {
        let result = catch_unwind(AssertUnwindSafe(action))
            .unwrap_or_else(|payload| Err(panic_cleanup_error("shutdown request", payload)));
        match result {
            Ok(()) => true,
            Err(error) => {
                self.fail(index, phase, error);
                false
            }
        }
    }

    /// Creates one budget and captures timer factory panics as Deadline errors.
    ///
    /// A budget is created at most once per phase of a single entry; the policy
    /// decides whether the phase is budgeted at all.
    ///
    /// # Parameters
    ///
    /// * `index` - Position of the entry in construction order, used to
    ///   attribute a factory panic to the right binding.
    /// * `graceful` - Whether the budget covers the graceful phase instead of
    ///   the termination phase.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(future))` - The phase is budgeted and `future` must be polled
    ///   alongside the component wait.
    /// * `Ok(None)` - The policy budgets this phase with no timer at all, so
    ///   the caller keeps waiting on the component alone.
    /// * `Err(())` - The timer factory panicked. The panic is already recorded
    ///   as a `ShutdownPhase::Deadline` failure for `index`, and the entry must
    ///   not wait any further.
    fn deadline(&mut self, index: usize, graceful: bool) -> Result<Option<DeadlineFuture>, ()> {
        match catch_unwind(AssertUnwindSafe(|| self.policy.deadline(graceful))) {
            Ok(deadline) => Ok(deadline),
            Err(payload) => {
                self.fail(
                    index,
                    ShutdownPhase::Deadline,
                    panic_cleanup_error("deadline factory", payload),
                );
                Err(())
            }
        }
    }

    /// Records one error with the concrete binding's original definition
    /// source.
    fn fail(&mut self, index: usize, phase: ShutdownPhase, error: CleanupError) {
        let entry = &self.entries[index];
        self.failures.push(ShutdownFailure {
            key: entry.key.clone(),
            definition: entry.definition,
            phase,
            error,
        });
    }

    /// Disarms unused callbacks after confirmed termination, including abort.
    fn complete_entry(&mut self, index: usize) {
        let entry = &mut self.entries[index];
        entry.state = EntryState::Done;
        entry.action.stop = None;
        entry.action.graceful = None;
        entry.action.wait = None;
    }

    /// Publishes final completeness once and shares the resulting observations.
    fn finish(&mut self) -> ShutdownReport {
        let report = ShutdownReport::new(
            self.requested,
            std::mem::take(&mut self.failures),
            self.pending(),
            std::mem::take(&mut self.fallbacks),
            self.overall_failure.clone(),
        );
        self.context.publish_state(if report.is_complete() {
            ApplicationState::Closed
        } else {
            ApplicationState::Incomplete
        });
        self.report = Some(report.clone());
        report
    }
}
