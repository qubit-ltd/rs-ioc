// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Dependency macro intermediate representation.

use syn::LitStr;
use syn::Type;

use crate::ir::DependencyKind;

/// Normalized dependency preserving the exact requested type and optional ID.
pub(crate) struct DependencyIr {
    /// Selection or configuration operation to generate.
    pub(crate) kind: DependencyKind,
    /// Concrete or trait-object type requested by the declaration.
    pub(crate) requested_type: Type,
    /// Optional exact ID for single-value selection.
    pub(crate) id: Option<LitStr>,
}
