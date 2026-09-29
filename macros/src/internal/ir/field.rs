// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Field macro intermediate representation.

use syn::Attribute;
use syn::Ident;

use crate::ir::DependencyIr;

/// A named field and its dependency request.
pub(crate) struct FieldIr {
    /// Field initialized by the generated factory.
    pub(crate) ident: Ident,
    /// Request used to construct the field value.
    pub(crate) dependency: DependencyIr,
    /// Conditions under which generated dependency and initialization code
    /// exists.
    pub(crate) conditions: Vec<Attribute>,
}
