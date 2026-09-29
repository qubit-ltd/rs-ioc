// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Flattened binding identity and registration metadata.

use crate::graph::internal::binding_location::BindingLocation;
use crate::key::BindingKey;
use crate::options::DefinitionSource;

/// One flattened binding with its stable registration position.
#[derive(Clone)]
pub(in crate::graph) struct Node {
    /// Original position within the definition list.
    pub(in crate::graph) location: BindingLocation,
    /// Exact typed identity used during matching.
    pub(in crate::graph) key: BindingKey,
    /// Definition source retained for diagnostics and stable ordering.
    pub(in crate::graph) source: DefinitionSource,
    /// Whether an unnamed request can select this candidate as primary.
    pub(in crate::graph) primary: bool,
    /// Collection ordering value.
    pub(in crate::graph) order: i32,
}
