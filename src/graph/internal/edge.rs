// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Resolved graph edges and deferred binding failures.

use crate::dependency::Dependency;
use crate::key::BindingKey;

/// A resolved edge or a failure delayed until its root path is known.
pub(in crate::graph) enum Edge {
    /// A dependency or alias target selected by validation.
    Target(usize),
    /// A required request with no matching key.
    MissingDependency(Dependency),
    /// A single-value request with multiple candidates.
    AmbiguousDependency(Dependency, Vec<BindingKey>),
    /// An alias refers to a key absent from active definitions.
    MissingAliasTarget(BindingKey),
}
