// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Output macro intermediate representation.

use syn::Type;

use crate::ir::OutputShape;

/// Normalized factory output, including the concrete component type.
pub(crate) struct OutputIr {
    /// Whether the function returns a value, `Arc`, or fallible form.
    pub(crate) shape: OutputShape,
    /// Whether the factory also returns explicit stop and optional wait
    /// actions.
    pub(crate) managed: bool,
    /// Concrete component type registered with the runtime.
    pub(crate) component_type: Type,
}
