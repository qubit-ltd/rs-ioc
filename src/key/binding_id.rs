// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Validated binding identifiers.

use crate::error::InvalidBindingId;

/// An owned, case-sensitive binding identifier.
///
/// # Examples
///
/// ```
/// use qubit_ioc::BindingId;
///
/// let id = BindingId::parse("database.primary")?;
/// assert_eq!(id.as_str(), "database.primary");
/// # Ok::<(), qubit_ioc::InvalidBindingId>(())
/// ```
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BindingId(String);

impl BindingId {
    /// Validates `value` and returns an owned identifier.
    ///
    /// An empty value or any segment outside `[A-Za-z][A-Za-z0-9_]*`
    /// returns [`InvalidBindingId`] containing the original input.
    ///
    /// # Returns
    ///
    /// The owned identifier with the supplied spelling.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidBindingId`] when the input is empty or any
    /// dot-delimited segment violates the identifier grammar.
    ///
    /// # Parameters
    ///
    /// `value` is the identifier text to validate and own.
    pub fn parse(value: &str) -> Result<Self, InvalidBindingId> {
        if value.split('.').all(valid_segment) {
            Ok(Self(value.to_owned()))
        } else {
            Err(InvalidBindingId::new(value))
        }
    }

    /// Returns the original, validated identifier text.
    ///
    /// # Returns
    ///
    /// The exact identifier spelling borrowed from this value.
    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BindingId {
    /// Formats the validated ID using its original spelling.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Checks one dot-delimited segment against the binding ID grammar.
fn valid_segment(segment: &str) -> bool {
    let mut bytes = segment.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
