// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Internal storage for constructed component values.

use std::any::Any;
#[cfg(test)]
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;

use crate::key::BindingKey;

/// An erased complete `Arc<T>`, including any trait-object vtable metadata.
pub(crate) type ErasedInstance = Box<dyn Any + Send + Sync>;

/// Stores built components by exact type and optional identifier.
#[derive(Default)]
pub(crate) struct InstanceStore {
    /// Values indexed by exact typed key.
    values: HashMap<BindingKey, ErasedInstance>,
}

impl InstanceStore {
    /// Inserts `value` under `key`, replacing an existing value with the same
    /// key.
    ///
    /// `T` must match the type encoded by `key`; callers validate key
    /// uniqueness before construction. The whole `Arc<T>` is erased so
    /// wide-pointer metadata remains available for later queries.
    #[cfg(test)]
    pub(crate) fn insert<T: ?Sized + Send + Sync + 'static>(&mut self, key: BindingKey, value: Arc<T>) {
        debug_assert_eq!(key.type_id(), TypeId::of::<T>());
        self.insert_erased(key, Box::new(value));
    }

    /// Inserts an already erased `Arc` under `key`, replacing any prior value.
    ///
    /// The caller must ensure the erased value contains the `Arc<T>` identified
    /// by `key`; factories and alias projectors are responsible for this
    /// invariant.
    pub(crate) fn insert_erased(&mut self, key: BindingKey, value: ErasedInstance) {
        self.values.insert(key, value);
    }

    /// Returns a clone of the stored `Arc<T>`, or `None` when absent or
    /// mistyped.
    pub(crate) fn get<T: ?Sized + Send + Sync + 'static>(&self, key: &BindingKey) -> Option<Arc<T>> {
        self.get_erased(key)?.downcast_ref::<Arc<T>>().cloned()
    }

    /// Borrows the erased value for alias projection, or `None` if absent.
    pub(crate) fn get_erased(&self, key: &BindingKey) -> Option<&ErasedInstance> {
        self.values.get(key)
    }
}
