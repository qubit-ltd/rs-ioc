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

use self::type_positions::TypePositions;
use super::BuiltBinding;
use crate::key::BindingKey;

// Owns the registration-ordered and collection-ordered position lists that back
// the typed lookups exposed by [`QueryIndex`].
mod type_positions;

/// Positions in the stable binding vector for typed and named lookups.
#[derive(Default)]
pub(crate) struct QueryIndex {
    /// Metadata positions grouped by Rust type with both stable orderings.
    by_type: HashMap<TypeId, TypePositions>,
    /// Exact typed binding keys mapped to stable metadata positions.
    by_key: HashMap<BindingKey, usize>,
}

impl QueryIndex {
    /// Builds the indexes once from validated, unique active binding keys.
    pub(crate) fn new(bindings: &[BuiltBinding]) -> Self {
        let mut index = Self::default();
        for (position, binding) in bindings.iter().enumerate() {
            index
                .by_type
                .entry(binding.key.type_id())
                .or_default()
                .registration
                .push(position);
            index.by_key.insert(binding.key.clone(), position);
        }
        for positions in index.by_type.values_mut() {
            positions.collection = positions.registration.clone();
            positions.collection.sort_by(|left, right| {
                let left = &bindings[*left];
                let right = &bindings[*right];
                left.order
                    .cmp(&right.order)
                    .then_with(|| left.key.id().cmp(&right.key.id()))
                    .then_with(|| left.source.cmp(&right.source))
            });
        }
        index
    }

    /// Returns active binding positions for one Rust type in registration
    /// order.
    ///
    /// # Returns
    ///
    /// A borrowed slice of positions into the binding vector this index was
    /// built from, in registration order, or an empty slice when `type_id` has
    /// no active binding. The slice borrows `self` and performs no allocation.
    #[inline]
    #[must_use]
    pub(crate) fn by_type(&self, type_id: TypeId) -> &[usize] {
        self.by_type
            .get(&type_id)
            .map_or(&[], |positions| positions.registration.as_slice())
    }

    /// Returns active binding positions for one Rust type in collection order.
    ///
    /// The same positions as [`Self::by_type`], reordered by the collection
    /// precedence `order`, then binding id, then definition source.
    ///
    /// # Returns
    ///
    /// A borrowed slice of positions into the binding vector this index was
    /// built from, in collection order, or an empty slice when `type_id` has no
    /// active binding. The slice borrows `self` and performs no allocation.
    #[inline]
    #[must_use]
    pub(crate) fn by_type_collection(&self, type_id: TypeId) -> &[usize] {
        self.by_type
            .get(&type_id)
            .map_or(&[], |positions| positions.collection.as_slice())
    }

    /// Returns the position for one exact typed key, if present.
    ///
    /// # Returns
    ///
    /// `Some(position)` with the position of the binding carrying exactly this
    /// type and binding id, or `None` when no active binding carries that key,
    /// which includes keys removed while de-duplicating overrides. Positions
    /// are only meaningful relative to the binding vector given to
    /// [`Self::new`].
    #[inline]
    #[must_use]
    pub(crate) fn by_key(&self, key: &BindingKey) -> Option<usize> {
        self.by_key.get(key).copied()
    }
}

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use super::QueryIndex;
    use crate::application_context::BuiltBinding;
    use crate::key::BindingId;
    use crate::key::BindingKey;
    use crate::options::DefinitionSource;

    #[test]
    fn test_collection_positions_use_collection_order_without_changing_registration_order() {
        let bindings = [binding("zeta", 1), binding("alpha", 1), binding("middle", 0)];
        let index = QueryIndex::new(&bindings);

        assert_eq!(index.by_type(TypeId::of::<u8>()), [0, 1, 2]);
        assert_eq!(index.by_type_collection(TypeId::of::<u8>()), [2, 1, 0]);
    }

    /// Creates test metadata for one typed binding.
    fn binding(id: &'static str, order: i32) -> BuiltBinding {
        BuiltBinding {
            key: BindingKey::of::<u8>(Some(BindingId::parse(id).expect("valid ID"))),
            primary: false,
            order,
            source: DefinitionSource::new("test", "test", "test.rs", 1, 1, id),
            replaced_sources: Vec::new(),
        }
    }
}
