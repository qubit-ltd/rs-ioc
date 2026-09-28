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
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("invalid binding ID `{value}`; expected dot-separated ASCII segments beginning with a letter")]
pub struct InvalidBindingId {
    /// Original text supplied by the caller.
    value: String,
}

impl InvalidBindingId {
    /// Records the original invalid text for diagnostics.
    pub fn new(value: &str) -> Self {
        Self {
            value: value.to_owned(),
        }
    }

    /// Returns the original invalid text.
    #[must_use]
    #[inline]
    pub fn value(&self) -> &str {
        &self.value
    }
}
