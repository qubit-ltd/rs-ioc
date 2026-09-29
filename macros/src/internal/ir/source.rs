// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Source macro intermediate representation.

use proc_macro2::Span;
use syn::Ident;

/// Original declaration location for generated static definition metadata.
pub(crate) struct SourceIr {
    /// Name of the annotated declaration.
    pub(crate) item: Ident,
    /// Source span used for generated diagnostics.
    pub(crate) span: Span,
}
