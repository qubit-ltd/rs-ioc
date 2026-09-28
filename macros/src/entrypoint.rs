// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runs a declaration through parsing, validation, and code generation.

use proc_macro::TokenStream;
use syn::Error;

use crate::expand;
use crate::ir::MacroKind;
use crate::parse;
use crate::validate;

/// Runs every declaration through parsing, validation, normalization, and
/// expansion.
pub(crate) fn expand_entry(kind: MacroKind, attribute: TokenStream, item: TokenStream) -> TokenStream {
    parse::parse(kind, attribute.into(), item.into())
        .and_then(validate::validate)
        .and_then(expand::dispatch)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}
