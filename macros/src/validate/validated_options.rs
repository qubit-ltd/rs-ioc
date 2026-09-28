// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Holds validated declaration options before conversion to the macro IR.

use syn::Ident;
use syn::LitStr;
use syn::Type;
use syn::TypeTraitObject;

use crate::ir::BindingOptions;

/// The checked option set before it is split into declaration-specific IR.
#[derive(Default)]
pub(super) struct ValidatedOptions {
    /// Validated optional component ID.
    pub(super) id: Option<LitStr>,
    /// Interface projections declared by repeated `bind` options.
    pub(super) binds: Vec<TypeTraitObject>,
    /// Whether the binding is preferred by unnamed selection.
    pub(super) primary: bool,
    /// Order value used by collection requests.
    pub(super) order: i32,
    /// Optional activation profile.
    pub(super) profile: Option<LitStr>,
    /// Required configuration subtree prefix.
    pub(super) prefix: Option<LitStr>,
    /// Explicit component type for an opaque bean return alias.
    pub(super) explicit_type: Option<Type>,
    /// Optional generated registration marker name.
    pub(super) marker: Option<Ident>,
}

impl ValidatedOptions {
    /// Moves common binding values into their stable IR representation.
    pub(super) fn binding(self) -> BindingOptions {
        BindingOptions {
            id: self.id,
            binds: self.binds,
            primary: self.primary,
            order: self.order,
            profile: self.profile,
        }
    }
}
