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
use crate::managed::CleanupError;
use crate::managed::ShutdownFailure;
use crate::managed::ShutdownMode;

/// Final observations from one shutdown attempt.
///
/// Reports share their original, potentially non-cloneable error sources.
/// Completion confirms termination; it does not imply that every action
/// succeeded.
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
/// // A report is obtained from the shutdown handle, never built directly.
/// let report = application.begin_shutdown(ShutdownMode::Immediate).abandon();
///
/// assert_eq!(report.mode(), ShutdownMode::Immediate);
/// assert!(report.is_complete());
/// assert!(report.is_success());
/// assert!(report.failures().is_empty());
/// assert!(report.incomplete().is_empty());
/// assert!(report.fallbacks().is_empty());
/// # Ok(())
/// # }
/// ```
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
    /// Failure from creating, polling, or expiring the overall deadline.
    overall_failure: Option<Arc<CleanupError>>,
}

impl ShutdownReport {
    /// Retains action-order failures and reverse construction-order bindings.
    ///
    /// This is the only way to build a report. The three collections are moved
    /// into shared storage, so every later clone of the report shares the same
    /// observations and the same original error sources. A built report is
    /// immutable: no method appends to or clears any collection, so the only
    /// way to observe new shutdown activity is to finish another attempt.
    ///
    /// # Parameters
    ///
    /// * `mode` - the mode the caller originally requested, not the mode the
    ///   shutdown may have upgraded to. An abort upgrade therefore does not
    ///   change what [`ShutdownReport::mode`] reports.
    /// * `failures` - the observed action errors in the order they were
    ///   observed. The vector is taken by value and its elements are shared
    ///   rather than re-wrapped, so a failure keeps its own error source.
    /// * `incomplete` - the components whose termination could not be
    ///   confirmed, in reverse construction order. Taken by value; the caller
    ///   must not keep observing it afterwards.
    /// * `fallbacks` - the components that used abort because they could not
    ///   honor a graceful request, in the same reverse construction order.
    /// * `overall_failure` - an optional failure from creating, polling, or
    ///   expiring the overall deadline. Its original error source is shared.
    ///
    /// # Returns
    ///
    /// An immutable report that shares the four observations with every clone
    /// made from it.
    pub(crate) fn new(
        mode: ShutdownMode,
        failures: Vec<ShutdownFailure>,
        incomplete: Vec<BindingKey>,
        fallbacks: Vec<BindingKey>,
        overall_failure: Option<Arc<CleanupError>>,
    ) -> Self {
        Self {
            mode,
            failures: failures.into(),
            incomplete: incomplete.into(),
            fallbacks: fallbacks.into(),
            overall_failure,
        }
    }

    /// Returns the originally requested mode, including after an abort upgrade.
    #[must_use]
    #[inline]
    pub fn mode(&self) -> ShutdownMode {
        self.mode
    }

    /// Returns failures in observation order, retaining their original sources.
    #[must_use]
    #[inline]
    pub fn failures(&self) -> &[ShutdownFailure] {
        &self.failures
    }

    /// Returns components whose termination could not be confirmed.
    #[must_use]
    #[inline]
    pub fn incomplete(&self) -> &[BindingKey] {
        &self.incomplete
    }

    /// Returns components that used abort because they lacked graceful support.
    #[must_use]
    #[inline]
    pub fn fallbacks(&self) -> &[BindingKey] {
        &self.fallbacks
    }

    /// Returns an overall deadline failure, if the total shutdown budget
    /// expired or its timer failed.
    #[must_use]
    #[inline]
    pub fn overall_failure(&self) -> Option<&CleanupError> {
        self.overall_failure.as_deref()
    }

    /// Whether termination was confirmed for every managed component.
    #[must_use]
    #[inline]
    pub fn is_complete(&self) -> bool {
        self.incomplete.is_empty()
    }

    /// Whether termination was confirmed without any observed failure.
    #[must_use]
    #[inline]
    pub fn is_success(&self) -> bool {
        self.is_complete() && self.failures.is_empty() && self.overall_failure.is_none()
    }
}
