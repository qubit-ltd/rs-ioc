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
pub(in crate::validate) struct ValidatedOptions {
    /// Validated optional component ID.
    pub(in crate::validate) id: Option<LitStr>,
    /// Interface projections declared by repeated `bind` options.
    pub(in crate::validate) binds: Vec<TypeTraitObject>,
    /// Whether the binding is preferred by unnamed selection.
    pub(in crate::validate) primary: bool,
    /// Order value used by collection requests.
    pub(in crate::validate) order: i32,
    /// Optional activation profile.
    pub(in crate::validate) profile: Option<LitStr>,
    /// Required configuration subtree prefix.
    pub(in crate::validate) prefix: Option<LitStr>,
    /// Explicit component type for an opaque bean return alias.
    pub(in crate::validate) explicit_type: Option<Type>,
    /// Optional generated registration marker name.
    pub(in crate::validate) marker: Option<Ident>,
}

impl ValidatedOptions {
    /// Moves common binding values into their stable IR representation.
    pub(in crate::validate) fn binding(self) -> BindingOptions {
        BindingOptions {
            id: self.id,
            binds: self.binds,
            primary: self.primary,
            order: self.order,
            profile: self.profile,
        }
    }
}
