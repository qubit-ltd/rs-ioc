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

/// Creates a role-specific identifier that does not capture call-site names.
pub(crate) fn internal_ident(role: &str, index: usize) -> Ident {
    Ident::new(&format!("__qubit_ioc_{role}_{index}"), Span::mixed_site())
}
