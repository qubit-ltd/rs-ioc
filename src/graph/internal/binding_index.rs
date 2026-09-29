// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Registration-ordered, multi-value lookup for active graph bindings.

use std::any::TypeId;
use std::collections::HashMap;

use crate::graph::internal::node::Node;
use crate::key::BindingKey;

/// Preserves every matching position until reachability resolves conflicts.
pub(in crate::graph) struct BindingIndex {
    /// All positions for an exact type and optional ID, in registration order.
    by_key: HashMap<BindingKey, Vec<usize>>,
    /// All positions in each Rust type namespace, in registration order.
    by_type: HashMap<TypeId, Vec<usize>>,
}

impl BindingIndex {
    /// Indexes `nodes` without selecting or rejecting duplicate registrations.
    ///
    /// # Parameters
    ///
    /// `nodes` contains active bindings in registration order.
    ///
    /// # Returns
    ///
    /// An index retaining every position for each exact key and type namespace,
    /// including duplicates, in the order supplied by `nodes`.
    #[must_use]
    pub(in crate::graph) fn new(nodes: &[Node]) -> Self {
        let mut by_key = HashMap::with_capacity(nodes.len());
        let mut by_type = HashMap::new();
        for (position, node) in nodes.iter().enumerate() {
            by_type
                .entry(node.key.type_id())
                .or_insert_with(Vec::new)
                .push(position);
            by_key.entry(node.key.clone()).or_insert_with(Vec::new).push(position);
        }
        Self { by_key, by_type }
    }

    /// Borrows every exact match for `key`, or an empty slice when absent.
    ///
    /// # Parameters
    ///
    /// `key` specifies the Rust type namespace and optional exact identifier.
    ///
    /// # Returns
    ///
    /// All matching positions in registration order, including duplicate
    /// registrations, borrowed from this index; an absent key returns an empty
    /// slice.
    #[must_use]
    #[inline]
    pub(in crate::graph) fn by_key(&self, key: &BindingKey) -> &[usize] {
        self.by_key.get(key).map_or(&[], Vec::as_slice)
    }

    /// Borrows all registrations of `type_id`, or an empty slice when absent.
    ///
    /// # Parameters
    ///
    /// `type_id` identifies the requested Rust type namespace.
    ///
    /// # Returns
    ///
    /// All positions in that namespace in registration order, borrowed from
    /// this index; an absent namespace returns an empty slice.
    #[must_use]
    #[inline]
    pub(in crate::graph) fn by_type(&self, type_id: TypeId) -> &[usize] {
        self.by_type.get(&type_id).map_or(&[], Vec::as_slice)
    }
}
