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

/// A validated synchronous or asynchronous bean function.
pub(crate) struct BeanIr {
    /// Original callable item retained in the expansion.
    pub(crate) item: ItemFn,
    /// Normalized registration properties.
    pub(crate) options: BindingOptions,
    /// Generated registration marker name, if explicitly selected.
    pub(crate) marker: Option<Ident>,
    /// Factory parameters and their requests in declaration order.
    pub(crate) params: Vec<ParamIr>,
    /// Normalized output type and shape.
    pub(crate) output: OutputIr,
    /// Original source identity for diagnostics.
    pub(crate) source: SourceIr,
}
