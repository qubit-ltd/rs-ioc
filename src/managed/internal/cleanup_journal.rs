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

use super::entry_state::EntryState;
use crate::key::BindingKey;
use crate::managed::CleanupAction;
use crate::managed::CleanupEntry;
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

    /// Transfers all entries to the unique shutdown driver and disarms Drop.
    #[must_use]
    pub(crate) fn take_entries(&mut self) -> Vec<CleanupEntry> {
        self.disarm_abort();
        std::mem::take(&mut self.entries)
    }

    /// Adds one constructed component's cleanup actions in construction order.
    pub(crate) fn push(&mut self, key: BindingKey, definition: DefinitionSource, action: CleanupAction) {
        self.entries.push(CleanupEntry {
            key,
            definition,
            action,
            state: EntryState::Constructed,
        });
    }

    /// Returns whether construction has produced no managed cleanup entries.
    #[must_use]
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
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
