// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared immutable query data and atomically published lifecycle state.

use std::sync::atomic::AtomicU8;

use super::built_binding::BuiltBinding;
use super::query_index::QueryIndex;
use crate::application_state::ApplicationState;
use crate::store::InstanceStore;

/// Data shared by every clone of an application query context.
pub(crate) struct ContextInner {
    /// Constructed values retained while a query context exists.
    pub(crate) store: InstanceStore,
    /// Stable metadata describing the selected bindings.
    pub(crate) bindings: Vec<BuiltBinding>,
    /// Precomputed exact and typed query indexes.
    pub(crate) query_index: QueryIndex,
    /// Lifecycle state published by the unique application or shutdown owner.
    pub(crate) state: AtomicU8,
}

impl ContextInner {
    /// Builds query indexes once and initializes a successfully built graph.
    pub(crate) fn new(store: InstanceStore, bindings: Vec<BuiltBinding>) -> Self {
        let query_index = QueryIndex::new(&bindings);
        Self {
            store,
            bindings,
            query_index,
            state: AtomicU8::new(ApplicationState::Running as u8),
        }
    }
}
