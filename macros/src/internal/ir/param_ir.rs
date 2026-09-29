// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Param macro intermediate representation.

use syn::Ident;

use crate::ir::DependencyIr;

/// A named bean parameter and its dependency request.
pub(crate) struct ParamIr {
    /// Parameter supplied when invoking the original factory function.
    pub(crate) ident: Ident,
    /// Request used to obtain the parameter value.
    pub(crate) dependency: DependencyIr,
}
