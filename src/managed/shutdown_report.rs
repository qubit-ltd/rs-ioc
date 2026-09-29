// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shareable shutdown observations retaining original error sources.

use std::sync::Arc;

use crate::key::BindingKey;
use crate::managed::ShutdownFailure;
use crate::managed::ShutdownMode;

/// Final observations from one shutdown attempt.
///
/// Reports share their original, potentially non-cloneable error sources.
/// Completion confirms termination; it does not imply that every action
/// succeeded.
#[derive(Clone, Debug)]
pub struct ShutdownReport {
    /// The caller's original requested mode.
    mode: ShutdownMode,
    /// Observed action errors, sharing their original sources.
    failures: Arc<[ShutdownFailure]>,
    /// Components whose termination remains unconfirmed.
    incomplete: Arc<[BindingKey]>,
    /// Components using abort in place of an unsupported graceful request.
    fallbacks: Arc<[BindingKey]>,
}

impl ShutdownReport {
    /// Retains action-order failures and reverse construction-order bindings.
    pub(crate) fn new(
        mode: ShutdownMode,
        failures: Vec<ShutdownFailure>,
        incomplete: Vec<BindingKey>,
        fallbacks: Vec<BindingKey>,
    ) -> Self {
        Self {
            mode,
            failures: failures.into(),
            incomplete: incomplete.into(),
            fallbacks: fallbacks.into(),
        }
    }

    /// Returns the originally requested mode, including after an abort upgrade.
    #[must_use]
    pub fn mode(&self) -> ShutdownMode {
        self.mode
    }

    /// Returns failures in observation order, retaining their original sources.
    #[must_use]
    pub fn failures(&self) -> &[ShutdownFailure] {
        &self.failures
    }

    /// Returns components whose termination could not be confirmed.
    #[must_use]
    pub fn incomplete(&self) -> &[BindingKey] {
        &self.incomplete
    }

    /// Returns components that used abort because they lacked graceful support.
    #[must_use]
    pub fn fallbacks(&self) -> &[BindingKey] {
        &self.fallbacks
    }

    /// Whether termination was confirmed for every managed component.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.incomplete.is_empty()
    }

    /// Whether termination was confirmed without any observed failure.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.is_complete() && self.failures.is_empty()
    }
}
