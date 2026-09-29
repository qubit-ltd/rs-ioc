// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Typed identity for one binding.

use std::any::TypeId;
use std::any::type_name;
use std::hash::Hash;
use std::hash::Hasher;

use super::BindingId;

/// A binding identity in one Rust type namespace.
///
/// # Examples
///
/// ```
/// use qubit_ioc::BindingId;
/// use qubit_ioc::BindingKey;
///
/// let id = BindingId::parse("database.primary")?;
/// let key = BindingKey::of::<String>(Some(id));
/// assert_eq!(key.id().map(BindingId::as_str), Some("database.primary"));
/// # Ok::<(), qubit_ioc::InvalidBindingId>(())
/// ```
#[derive(Clone, Debug)]
pub struct BindingKey {
    /// Runtime type identity used for key equality and hashing.
    type_id: TypeId,
    /// Readable Rust type name retained for diagnostics.
    type_name: &'static str,
    /// Validated exact ID, or `None` for an unnamed binding.
    id: Option<BindingId>,
}

impl PartialEq for BindingKey {
    /// Compares Rust type identity and the optional validated ID.
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id && self.id == other.id
    }
}

impl Eq for BindingKey {}

impl Hash for BindingKey {
    /// Hashes the same identity components used by equality.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id.hash(state);
        self.id.hash(state);
    }
}

impl BindingKey {
    /// Creates a key for `T` and an optional validated identifier.
    ///
    /// # Type Parameters
    ///
    /// `T` is the Rust type namespace for this binding and must be `'static`.
    ///
    /// # Parameters
    ///
    /// `id` is the validated identifier for the binding, or `None` for an
    /// unnamed binding.
    ///
    /// # Returns
    ///
    /// A key combining `T`'s runtime type identity with `id`.
    #[must_use]
    #[inline]
    pub fn of<T: ?Sized + 'static>(id: Option<BindingId>) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            id,
        }
    }

    /// Creates an internal lookup key from runtime type metadata and a
    /// validated optional identifier.
    ///
    /// # Parameters
    ///
    /// `type_id` identifies the Rust type namespace. `type_name` is the
    /// corresponding readable Rust type name, retained only for diagnostics.
    /// The caller must ensure both describe the same Rust type.
    /// `id` is a validated exact identifier, or `None` for an unnamed binding.
    ///
    /// # Returns
    ///
    /// A key whose equality and hashing use `type_id` and `id`, preserving
    /// `type_name` for diagnostic output without revalidating the identifier.
    #[must_use]
    #[inline]
    pub(crate) fn from_parts(type_id: TypeId, type_name: &'static str, id: Option<BindingId>) -> Self {
        Self { type_id, type_name, id }
    }

    /// Returns the Rust type identity used for equality.
    ///
    /// # Returns
    ///
    /// The process-local `TypeId` captured when this key was created.
    #[must_use]
    #[inline]
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    /// Returns the Rust type name used for diagnostics.
    ///
    /// # Returns
    ///
    /// The compiler-provided type name captured when this key was created.
    #[must_use]
    #[inline]
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Returns the validated ID, or `None` for an unnamed binding.
    ///
    /// # Returns
    ///
    /// `Some` borrows the validated ID; `None` indicates an unnamed key.
    #[must_use]
    #[inline]
    pub fn id(&self) -> Option<&BindingId> {
        self.id.as_ref()
    }
}
