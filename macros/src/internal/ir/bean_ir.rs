// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Bean macro intermediate representation.

use syn::Ident;
use syn::ItemFn;

use crate::ir::BindingOptions;
use crate::ir::OutputIr;
use crate::ir::ParamIr;
use crate::ir::SourceIr;

/// Validated intermediate representation of a bean function.
///
/// The value is produced by the validation stage after the raw declaration has
/// been parsed, checked against the runtime path, and normalized, and it is
/// consumed by the expansion stage that emits the registration code. It is
/// never stored in the user's program: no runtime state, no registry handle,
/// and no lazy initialization live here, and the whole value is dropped once
/// the expansion produces its token stream.
///
/// Because the expansion stage trusts this representation, every field is
/// already normalized, so expansion performs no re-validation and no
/// fallible parsing.
pub(crate) struct BeanIr {
    /// Original callable item retained in the expansion.
    pub(crate) item: ItemFn,
    /// Registration properties after normalization.
    ///
    /// Aliases, binding hints, and cardinality defaults have been resolved, so
    /// expansion reads the canonical form and never has to re-interpret raw
    /// attribute tokens.
    pub(crate) options: BindingOptions,
    /// Generated registration marker name, if explicitly selected.
    pub(crate) marker: Option<Ident>,
    /// Factory parameters and their requests in declaration order.
    pub(crate) params: Vec<ParamIr>,
    /// Factory output type and shape after normalization.
    ///
    /// The recorded shape is what tells expansion which return branches to
    /// emit, for example a direct value, a future, or an explicit result type.
    pub(crate) output: OutputIr,
    /// Original source identity for diagnostics.
    pub(crate) source: SourceIr,
}
