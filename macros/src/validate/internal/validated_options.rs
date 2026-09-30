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
///
/// Each annotated declaration builds one of these during validation, fills the
/// fields that its own macro cares about, and then hands the common part to
/// [`ValidatedOptions::binding`]. The value never reaches the generated
/// program: it exists only while the macro expands and is dropped afterwards,
/// so it carries no runtime state, no registration handle, and no deferred
/// initialization, and it is never shared across threads.
///
/// The first five fields are the options common to every declaration; the last
/// three belong to individual macros and are consumed by those macros directly.
/// A value is produced through [`Default`] and filled in field by field by the
/// validation stage, never through a constructor with optional arguments.
#[derive(Default)]
pub(in crate::validate) struct ValidatedOptions {
    /// Identifier the declaration is published under, when one was written.
    ///
    /// The literal already satisfies the segment grammar checked by
    /// `options::validate_id`, so it can be handed to the runtime `BindingId`
    /// without further rewriting. `None` means the declaration keeps its
    /// generated name instead of an explicit one.
    pub(in crate::validate) id: Option<LitStr>,
    /// Interface projections collected from the `bind` options of the
    /// declaration.
    ///
    /// `bind` is the only repeatable option, so this vector preserves
    /// declaration order and may hold more than one `dyn Trait` projection.
    /// Each entry is the projection as written; its binder lifetime and
    /// modifiers are not interpreted while the macro expands.
    pub(in crate::validate) binds: Vec<TypeTraitObject>,
    /// Whether unnamed selection prefers this binding when several candidates
    /// satisfy the same request.
    ///
    /// Defaults to `false`; only a `primary` marker option sets it, so a
    /// declaration that does not opt in never wins an ambiguous selection.
    pub(in crate::validate) primary: bool,
    /// Sort key applied when the binding is collected from a collection
    /// request.
    ///
    /// The value has already been range-checked to fit in 32 bits, so the
    /// expansion stage can emit it into generated sorting code without
    /// re-checking. `0` is the value used when no `order` option is written.
    pub(in crate::validate) order: i32,
    /// Activation profile gating the declaration, when one was written.
    ///
    /// Only the literal is carried here; whether a profile is active is
    /// decided at runtime by the activation conditions, so the macro never
    /// evaluates the profile while expanding.
    pub(in crate::validate) profile: Option<LitStr>,
    /// Configuration subtree prefix the declaration reads its values from.
    ///
    /// Mandatory for the configuration-properties macro, which takes this
    /// field out of the option set before building the binding; the resulting
    /// binding therefore no longer carries it. A `None` observed afterwards
    /// means the prefix has already been consumed rather than never present.
    pub(in crate::validate) prefix: Option<LitStr>,
    /// Component type named by the `type` option of an opaque bean.
    ///
    /// Used to spell the return alias of a bean whose component type cannot be
    /// inferred from the method signature. `None` when the bean relies on that
    /// inference instead.
    pub(in crate::validate) explicit_type: Option<Type>,
    /// Name of the marker the generated registration exposes for the bean.
    ///
    /// `None` when the bean asked for no marker. A generated fallback name is
    /// deliberately not stored here, so the expansion stage stays the single
    /// place that decides it.
    pub(in crate::validate) marker: Option<Ident>,
}

impl ValidatedOptions {
    /// Moves common binding values into their stable IR representation.
    ///
    /// This consumes the option set, so the caller cannot read it afterwards.
    /// Only the five common fields are carried over; `prefix`, `explicit_type`,
    /// and `marker` are deliberately dropped because the macro that declared
    /// them consumes them before this call. A caller that still needs one of
    /// those fields must take or clone it first.
    ///
    /// # Parameters
    ///
    /// * `self` – the option set to consume; all of its fields are moved out.
    ///
    /// # Returns
    ///
    /// The binding options in their stable IR form, carrying the identifier,
    /// interface projections, primary flag, order, and profile.
    #[must_use]
    #[inline]
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
