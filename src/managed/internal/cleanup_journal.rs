// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Construction-order lifecycle journal and cancellation cleanup.

use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;

use crate::key::BindingKey;
use crate::managed::CleanupAction;
use crate::managed::CleanupEntry;
use crate::managed::ShutdownFailure;
use crate::managed::ShutdownPhase;
use crate::managed::internal::wait::panic_cleanup_error;
use crate::managed::internal::wait::poll_wait;
use crate::managed::internal::wait::start_wait;
use crate::options::DefinitionSource;

/// Tracks successfully constructed managed components until publication.
pub(crate) struct CleanupJournal {
    /// Successfully constructed managed values in construction order.
    pub(in crate::managed) entries: Vec<CleanupEntry>,
    /// Whether dropping the journal should run stop actions without awaiting
    /// during build abort.
    abort_on_drop: bool,
}

impl Default for CleanupJournal {
    /// Creates an empty journal with cancellation cleanup armed.
    #[inline]
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            abort_on_drop: true,
        }
    }
}

impl CleanupJournal {
    /// Disables build-cancellation cleanup after ownership moves to a context.
    #[inline]
    pub(crate) fn disarm_abort(&mut self) {
        self.abort_on_drop = false;
    }

    /// Adds one constructed component's cleanup actions in construction order.
    pub(crate) fn push(&mut self, key: BindingKey, definition: DefinitionSource, action: CleanupAction) {
        self.entries.push(CleanupEntry {
            key,
            definition,
            action,
        });
    }

    /// Calls every stop action in reverse construction order.
    pub(crate) fn stop_reverse(&mut self) -> Vec<ShutdownFailure> {
        let mut failures = Vec::new();
        for entry in self.entries.iter_mut().rev() {
            if let Some(stop) = entry.action.stop.take() {
                let error = match catch_unwind(AssertUnwindSafe(stop)) {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error),
                    Err(payload) => Some(panic_cleanup_error("stop callback", payload)),
                };
                if let Some(error) = error {
                    failures.push(ShutdownFailure {
                        key: entry.key.clone(),
                        definition: entry.definition,
                        phase: ShutdownPhase::Stop,
                        error,
                    });
                }
            }
        }
        failures
    }

    /// Waits in reverse construction order after all stop actions have run.
    pub(crate) async fn wait_reverse(&mut self) -> Vec<ShutdownFailure> {
        let mut failures = Vec::new();
        for entry in self.entries.iter_mut().rev() {
            if let Some(wait) = entry.action.wait.take() {
                let result = match start_wait(wait) {
                    Ok(mut future) => poll_wait(&mut future).await,
                    Err(error) => Err(error),
                };
                if let Err(error) = result {
                    failures.push(ShutdownFailure {
                        key: entry.key.clone(),
                        definition: entry.definition,
                        phase: ShutdownPhase::Wait,
                        error,
                    });
                }
            }
        }
        self.entries.clear();
        failures
    }

    /// Runs stop actions during cancellation without blocking or awaiting.
    fn abort(&mut self) {
        for entry in self.entries.iter_mut().rev() {
            if let Some(stop) = entry.action.stop.take() {
                let _ = catch_unwind(AssertUnwindSafe(stop));
            }
        }
        self.entries.clear();
    }
}

impl Drop for CleanupJournal {
    /// Stops constructed resources when build ownership was not published.
    ///
    /// Stop failures and panics are discarded; asynchronous waits are never
    /// polled from `Drop`.
    fn drop(&mut self) {
        if self.abort_on_drop {
            self.abort();
        }
    }
}
