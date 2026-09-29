// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Syntax parsing for IoC declarations and their options.

use proc_macro2::Span;
use proc_macro2::TokenStream;
use syn::Error;
use syn::Result;
use syn::Token;
use syn::parse::Parser;
use syn::parse2;
use syn::punctuated::Punctuated;

mod internal;

pub(crate) use internal::RawDeclaration;
pub(crate) use internal::RawOption;
pub(crate) use internal::RawValue;

use crate::ir::MacroKind;

/// Parses attribute tokens and the annotated Rust item without local semantic
/// checks.
pub(crate) fn parse(kind: MacroKind, attribute: TokenStream, item: TokenStream) -> Result<RawDeclaration> {
    let options = parse_options(attribute)?;
    let item = parse2(item)?;
    Ok(RawDeclaration { kind, item, options })
}

/// Parses a comma-separated option list, preserving each key's source span.
pub(crate) fn parse_options(tokens: TokenStream) -> Result<Vec<RawOption>> {
    let parser = Punctuated::<RawOption, Token![,]>::parse_terminated;
    Ok(parser.parse2(tokens)?.into_iter().collect())
}

/// Creates a span-bearing error for a missing required option.
pub(crate) fn missing_option(span: Span, name: &str) -> Error {
    Error::new(span, format!("missing required `{name}` option"))
}
