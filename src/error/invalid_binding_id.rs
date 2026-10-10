// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Invalid binding identifier errors.

use thiserror::Error;

/// An ID that does not match the binding identifier grammar.
///
/// # Examples
///
/// ```
/// use qubit_ioc::BindingId;
///
/// let error = BindingId::parse("bad-id").expect_err("hyphens are not allowed");
/// assert_eq!(error.value(), "bad-id");
/// ```
#[must_use]
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("invalid binding ID `{value}`; expected dot-separated ASCII segments beginning with a letter")]
pub struct InvalidBindingId {
    /// Original text supplied by the caller.
    value: String,
}

impl InvalidBindingId {
    /// Records the original invalid text for diagnostics.
    ///
    /// The text is copied, so the caller keeps ownership of its input string
    /// and may reuse or free it immediately.
    ///
    /// # Parameters
    ///
    /// * `value` - Rejected identifier text, copied into the error.
    ///
    /// # Returns
    ///
    /// Returns an error owning the rejected text for later reporting.
    #[must_use]
    pub fn new(value: &str) -> Self {
        Self {
            value: value.to_owned(),
        }
    }

    /// Returns the original invalid text.
    ///
    /// The borrow stays valid while `self` is borrowed and performs no
    /// allocation.
    ///
    /// # Returns
    ///
    /// Returns the caller's original spelling of the rejected identifier.
    #[must_use]
    #[inline]
    pub fn value(&self) -> &str {
        &self.value
    }
}
