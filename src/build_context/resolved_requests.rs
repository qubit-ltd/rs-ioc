// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Stores resolved factory requests using the measured small-list crossover.

use std::collections::HashMap;

use crate::dependency::Dependency;
use crate::graph::ResolvedDependency;
use crate::key::BindingKey;

/// Avoids hashing overhead for small request sets while indexing larger ones.
pub(super) enum ResolvedRequests {
    /// Keeps small declarations in their original order for direct lookup.
    Linear(
        /// Requests and selected keys resolved during graph validation.
        Vec<ResolvedDependency>,
    ),
    /// Uses average constant-time exact lookup for larger declarations.
    Indexed(
        /// Maps complete requests to their selected keys.
        HashMap<Dependency, Vec<BindingKey>>,
    ),
}

/// The measured crossover; larger request sets use exact-request hashing.
const HASH_INDEX_THRESHOLD: usize = 16;

impl ResolvedRequests {
    /// Retains small lists and indexes larger lists, moving all request data.
    ///
    /// # Parameters
    ///
    /// * `resolved` — Requests and selected keys fixed during graph validation.
    ///
    /// # Returns
    ///
    /// A representation chosen for the measured request count.
    pub(super) fn new(resolved: Vec<ResolvedDependency>) -> Self {
        if resolved.len() <= HASH_INDEX_THRESHOLD {
            return Self::Linear(resolved);
        }

        let mut requests = HashMap::with_capacity(resolved.len());
        for entry in resolved {
            requests.entry(entry.request).or_insert(entry.keys);
        }
        Self::Indexed(requests)
    }
}
