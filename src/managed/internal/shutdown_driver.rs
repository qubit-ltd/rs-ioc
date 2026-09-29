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
        if let Some(mut active) = self.active.take() {
            if active.phase == ShutdownPhase::GracefulWait {
                active.phase = ShutdownPhase::TerminationWait;
                match self.deadline(active.index, false) {
                    Ok(deadline) => {
                        active.deadline = deadline;
                        self.active = Some(active);
                    }
                    Err(()) => self.entries[active.index].state = EntryState::Incomplete,
                }
            } else {
                self.active = Some(active);
            }
        }
    }

    /// Returns all bindings whose termination has not been confirmed.
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
    pub(crate) fn poll(&mut self, context: &mut Context<'_>) -> Poll<ShutdownReport> {
        if let Some(report) = &self.report {
            return Poll::Ready(report.clone());
        }
        loop {
            if self.active.is_none() {
                if self.cursor == 0 {
                    return Poll::Ready(self.finish());
                }
                self.cursor -= 1;
                let index = self.cursor;
                if matches!(self.entries[index].state, EntryState::Done | EntryState::Incomplete) {
                    continue;
                }
                if self.mode == ShutdownMode::Graceful {
                    self.request_graceful(index);
                }
                if matches!(self.entries[index].state, EntryState::Done | EntryState::Incomplete) {
                    continue;
                }
                let Some(wait) = self.entries[index].action.wait.take() else {
                    // A successful request with no wait promises synchronous termination.
                    self.complete_entry(index);
                    continue;
                };
                let future = match start_wait(wait) {
                    Ok(future) => future,
                    Err(error) => {
                        self.fail(index, ShutdownPhase::Wait, error);
                        self.abort_entry(index);
                        self.entries[index].state = EntryState::Incomplete;
                        continue;
                    }
                };
                let graceful = self.entries[index].state == EntryState::GracefulRequested;
                let deadline = match self.deadline(index, graceful) {
                    Ok(deadline) => deadline,
                    Err(()) => {
                        self.abort_entry(index);
                        self.entries[index].state = EntryState::Incomplete;
                        continue;
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
            }

            let mut active = self.active.take().expect("active wait initialized before polling");
            let waited = poll_wait_once(&mut active.future, context);
            match waited {
                Poll::Ready(Ok(())) => {
                    self.complete_entry(active.index);
                    continue;
                }
                Poll::Ready(Err(error)) => {
                    self.fail(active.index, ShutdownPhase::Wait, error);
                    self.abort_entry(active.index);
                    self.entries[active.index].state = EntryState::Incomplete;
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
                    return Poll::Pending;
                }
                Some(Err(payload)) => {
                    self.fail(
                        active.index,
                        ShutdownPhase::Deadline,
                        panic_cleanup_error("deadline future", payload),
                    );
                    self.abort_entry(active.index);
                    self.entries[active.index].state = EntryState::Incomplete;
                }
                Some(Ok(Poll::Ready(()))) => {
                    self.fail(
                        active.index,
                        active.phase,
                        CleanupError::new(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "component shutdown deadline expired",
                        )),
                    );
                    self.abort_entry(active.index);
                    if active.phase == ShutdownPhase::GracefulWait {
                        active.phase = ShutdownPhase::TerminationWait;
                        match self.deadline(active.index, false) {
                            Ok(deadline) => {
                                active.deadline = deadline;
                                self.active = Some(active);
                            }
                            Err(()) => self.entries[active.index].state = EntryState::Incomplete,
                        }
                    } else {
                        self.entries[active.index].state = EntryState::Incomplete;
                    }
                }
            }
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
                self.complete_entry(index);
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
