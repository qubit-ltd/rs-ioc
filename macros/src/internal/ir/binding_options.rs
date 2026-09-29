// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Binding options macro intermediate representation.

use syn::LitStr;
use syn::TypeTraitObject;

/// Common binding properties; an omitted ID remains `None`.
pub(crate) struct BindingOptions {
    /// Validated optional component ID literal.
    pub(crate) id: Option<LitStr>,
    /// Trait object interfaces projected from the concrete component.
    pub(crate) binds: Vec<TypeTraitObject>,
    /// Whether unnamed requests prefer the binding.
    pub(crate) primary: bool,
    /// Ordering value used for collection injection.
    pub(crate) order: i32,
    /// Optional activation profile.
    pub(crate) profile: Option<LitStr>,
}
