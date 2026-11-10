// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Transfers cleanup after the active factory future is dropped.

use super::BuildSession;
use super::state::SessionState;
use crate::builder::internal::Construction;

/// Keeps cancellation armed while the nested factory future borrows storage.
/// That future is dropped before this guard, including during unwinding.
pub(super) struct RunGuard<'a> {
    /// The session that receives cancelled cleanup ownership.
    pub(super) session: &'a mut BuildSession,
    /// Partially completed graph, moved out only on normal completion.
    pub(super) construction: Option<Construction>,
}

impl Drop for RunGuard<'_> {
    /// Requests abort only after the active factory future has been dropped;
    /// preserves the unique observer in the caller-owned session.
    fn drop(&mut self) {
        if matches!(self.session.state, SessionState::Running) {
            self.session.state = SessionState::Cancelled;
            self.session.cleanup = self.construction.take().and_then(Construction::into_cleanup);
        }
    }
}
