// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component macro intermediate representation.

use syn::ItemStruct;

use crate::ir::BindingOptions;
use crate::ir::FieldIr;
use crate::ir::SourceIr;

/// Validated intermediate representation of one component, service, or
/// repository declaration.
///
/// The value is produced by `macros::validate::component::component` after the
/// struct shape, generics, and attribute arguments have been checked, and it is
/// consumed exactly once by `macros::expand::component::expand`, which
/// destructures it and forwards the individual fields. Because validation has
/// already normalized the declaration, expansion re-reads the field attributes
/// only to re-emit activation conditions and does not repeat the semantic
/// checks, so this type is a compile-time data carrier: it is owned, holds no
/// runtime handle or lazily initialized value, performs no I/O, and is dropped
/// once expansion has produced its token stream. Nothing here reaches the
/// generated program except through the tokens that expansion emits.
pub(crate) struct ComponentIr {
    /// The annotated struct after validation, retained so expansion can emit it
    /// verbatim as the first part of the generated output.
    ///
    /// The item has already passed the shape and generic checks, so its field
    /// types and generics are known to be emittable; its activation attributes
    /// are still present and are re-normalized during expansion, which is why
    /// expansion can fail on them even though validation succeeded.
    pub(crate) item: ItemStruct,
    /// Binding properties with every omitted attribute already replaced by its
    /// default, so expansion never has to distinguish "absent" from "empty".
    ///
    /// See [`BindingOptions`] for the meaning of each property and for the
    /// `None` cases that stand for "derive it from the source item".
    pub(crate) options: BindingOptions,
    /// One entry per declared field, kept in the same order as the source
    /// struct so generated initializer assignments follow declaration order.
    ///
    /// Each entry carries its own span, activation conditions, and — when a
    /// field's dependency attributes were invalid but the field is behind an
    /// activation condition — a deferred error that expansion re-emits as a
    /// compile error instead of aborting the whole declaration.
    pub(crate) fields: Vec<FieldIr>,
    /// Identity and span of the annotated declaration, used to build the
    /// `DefinitionSource` metadata of the generated registration entry and to
    /// anchor emitted diagnostics on the original item.
    pub(crate) source: SourceIr,
}
