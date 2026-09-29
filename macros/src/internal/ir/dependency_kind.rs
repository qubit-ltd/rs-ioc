// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Dependency kind macro intermediate representation.

use syn::LitStr;

/// Cardinality and source of a field or parameter value.
pub(crate) enum DependencyKind {
    /// Exactly one binding is required.
    Required,
    /// A missing binding becomes `None`.
    Optional,
    /// All matching bindings are collected in order.
    All,
    /// A value is read from the configuration snapshot at the given path.
    Value {
        /// Configuration lookup path retained with its diagnostic source span.
        path: LitStr,
    },
}
