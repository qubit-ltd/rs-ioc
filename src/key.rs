// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Validated binding identifiers and typed binding keys.

use std::any::TypeId;
use std::any::type_name;
use std::hash::Hash;
use std::hash::Hasher;

use crate::error::InvalidBindingId;

/// An owned, case-sensitive binding identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BindingId(String);

impl BindingId {
    /// Validates `value` and returns an owned identifier.
    ///
    /// An empty value or any segment outside `[A-Za-z][A-Za-z0-9_]*`
    /// returns [`InvalidBindingId`] containing the original input.
    pub fn parse(value: &str) -> Result<Self, InvalidBindingId> {
        if value.split('.').all(valid_segment) {
            Ok(Self(value.to_owned()))
        } else {
            Err(InvalidBindingId::new(value))
        }
    }

    /// Returns the original, validated identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BindingId {
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

/// A binding identity in one Rust type namespace.
#[derive(Clone, Debug)]
pub struct BindingKey {
    type_id: TypeId,
    type_name: &'static str,
    id: Option<BindingId>,
}

impl PartialEq for BindingKey {
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id && self.id == other.id
    }
}

impl Eq for BindingKey {}

impl Hash for BindingKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
        self.id.hash(state);
    }
}

impl BindingKey {
    /// Creates a key for `T` and an optional validated identifier.
    pub fn of<T: ?Sized + 'static>(id: Option<BindingId>) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            id,
        }
    }

    /// Returns the Rust type identity used for equality.
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Returns the Rust type name used for diagnostics.
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Returns the validated ID, or `None` for an unnamed binding.
    pub fn id(&self) -> Option<&BindingId> {
        self.id.as_ref()
    }
}
