// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Immutable typed and exact-key indexes for built binding metadata.

use std::any::TypeId;
use std::collections::HashMap;

use super::BuiltBinding;
use crate::key::BindingKey;

/// Positions in the stable binding vector for typed and named lookups.
#[derive(Default)]
pub(crate) struct QueryIndex {
    by_type: HashMap<TypeId, Vec<usize>>,
    by_key: HashMap<BindingKey, usize>,
}

impl QueryIndex {
    /// Builds the indexes once from validated, unique active binding keys.
    pub(crate) fn new(bindings: &[BuiltBinding]) -> Self {
        let mut index = Self::default();
        for (position, binding) in bindings.iter().enumerate() {
            index.by_type.entry(binding.key.type_id()).or_default().push(position);
            index.by_key.insert(binding.key.clone(), position);
        }
        index
    }

    /// Returns active binding positions for one Rust type in registration
    /// order.
    pub(crate) fn by_type(&self, type_id: TypeId) -> &[usize] {
        self.by_type.get(&type_id).map_or(&[], Vec::as_slice)
    }

    /// Returns the position for one exact typed key, if present.
    pub(crate) fn by_key(&self, key: &BindingKey) -> Option<usize> {
        self.by_key.get(key).copied()
    }
}
