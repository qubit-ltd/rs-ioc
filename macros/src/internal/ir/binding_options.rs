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

/// Binding properties shared by every declaration that registers a component.
///
/// The value is filled in by the validation stage, after raw attribute tokens
/// have been parsed and checked, and it is read by the expansion stage that
/// emits the registration code. It never reaches the generated program: it
/// carries no runtime handle, no registry state, and no lazily initialized
/// value, and it is dropped as soon as expansion produces its token stream.
///
/// Fields the declaration omitted keep their defaults here, so expansion never
/// has to distinguish "absent" from "empty" for a property it must always emit.
pub(crate) struct BindingOptions {
    /// Component ID literal, when the declaration names one explicitly.
    ///
    /// `None` means the declaration relied on the automatically derived
    /// component name, which expansion computes from the source item.
    pub(crate) id: Option<LitStr>,
    /// Trait object interfaces projected from the concrete component.
    pub(crate) binds: Vec<TypeTraitObject>,
    /// Whether unnamed requests prefer the binding.
    pub(crate) primary: bool,
    /// Ordering value used for collection injection.
    pub(crate) order: i32,
    /// Activation profile projected onto a `cfg` predicate.
    ///
    /// `None` means the declaration is active under every configuration.
    pub(crate) profile: Option<LitStr>,
}
