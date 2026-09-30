// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Map of constructed component values keyed by exact binding identity.

use std::collections::HashMap;
use std::sync::Arc;

use crate::key::BindingKey;
use crate::store::ErasedInstance;

/// Stores built components by exact type and optional identifier.
#[derive(Default)]
pub(crate) struct InstanceStore {
    /// Values indexed by exact typed key.
    values: HashMap<BindingKey, ErasedInstance>,
}

impl InstanceStore {
    /// Returns a clone of the stored `Arc<T>`, or `None` when absent or
    /// mistyped.
    ///
    /// # Type Parameters
    ///
    /// `T` is the type the caller expects to read back. It must be
    /// `Send + Sync + 'static` and must match the type encoded by `key`; a
    /// mismatch is reported as `None` rather than as a panic.
    ///
    /// # Parameters
    ///
    /// `key` is the exact typed key to look up. A key that is absent and a key
    /// whose stored value holds a different concrete type both yield `None`.
    ///
    /// # Returns
    ///
    /// `Some` with a shared clone of the stored value when `key` is present and
    /// holds an `Arc<T>`, otherwise `None` - both for an unknown key and for a
    /// key whose erased value holds a different concrete type.
    #[must_use]
    #[inline]
    pub(crate) fn get<T: ?Sized + Send + Sync + 'static>(&self, key: &BindingKey) -> Option<Arc<T>> {
        self.get_erased(key)?.downcast_ref::<Arc<T>>().cloned()
    }

    /// Borrows the erased value for alias projection, or `None` if absent.
    ///
    /// The borrow stays valid while `self` is borrowed; it neither clones nor
    /// allocates, and it does not check the concrete type behind the erased
    /// value, leaving that projection to the caller.
    ///
    /// # Parameters
    ///
    /// `key` is the exact typed key whose erased entry is borrowed; no type
    /// check is performed against the value behind it.
    ///
    /// # Returns
    ///
    /// `Some` referencing the stored erased value when `key` is present, or
    /// `None` when no value is stored under that exact key.
    #[must_use]
    #[inline]
    pub(crate) fn get_erased(&self, key: &BindingKey) -> Option<&ErasedInstance> {
        self.values.get(key)
    }

    /// Clones one erased value so a factory can retain only its resolved
    /// dependencies.
    ///
    /// # Parameters
    ///
    /// `key` is the exact typed key whose erased entry is cloned into a new
    /// shared handle.
    ///
    /// # Returns
    ///
    /// Returns `Some` with a shared clone when `key` is present, or `None`
    /// when no value is stored under that exact key.
    #[must_use]
    #[inline]
    pub(crate) fn get_erased_cloned(&self, key: &BindingKey) -> Option<ErasedInstance> {
        self.values.get(key).cloned()
    }

    /// Inserts an already erased `Arc` under `key`, replacing any prior value.
    ///
    /// The caller must ensure the erased value contains the `Arc<T>` identified
    /// by `key`; factories and alias projectors are responsible for this
    /// invariant.
    ///
    /// # Parameters
    ///
    /// `key` is the exact typed key the erased value is filed under, and
    /// `value` is the already type-erased shared component that must match
    /// the concrete type `key` encodes. Any prior entry for the same key is
    /// replaced.
    pub(crate) fn insert_erased(&mut self, key: BindingKey, value: ErasedInstance) {
        self.values.insert(key, value);
    }
}
