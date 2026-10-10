// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Creates identifiers that stay local to macro-generated code.

use proc_macro2::Span;
use syn::Ident;

/// Creates an identifier for a generated symbol without capturing call-site
/// names.
///
/// The generated spelling combines `role` and `index` under the crate's
/// reserved `__qubit_ioc_` prefix. `Span::mixed_site()` gives the identifier
/// macro-local hygiene while preserving the supplied spelling.
///
/// # Parameters
///
/// - `role`: Describes the generated symbol's purpose.
/// - `index`: Distinguishes symbols with the same role.
///
/// # Returns
///
/// An identifier whose spelling is `__qubit_ioc_{role}_{index}`.
#[must_use]
pub(crate) fn internal_ident(role: &str, index: usize) -> Ident {
    Ident::new(&format!("__qubit_ioc_{role}_{index}"), Span::mixed_site())
}
