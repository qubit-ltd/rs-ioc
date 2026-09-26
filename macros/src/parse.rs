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
use syn::Ident;
use syn::Item;
use syn::LitInt;
use syn::LitStr;
use syn::Token;
use syn::Type;
use syn::ext::IdentExt;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse::Parser;
use syn::punctuated::Punctuated;

use crate::ir::MacroKind;

/// An attribute whose syntax has been parsed but not semantically validated.
pub(crate) struct RawDeclaration {
    /// Macro attribute that selected the accepted syntax.
    pub(crate) kind: MacroKind,
    /// Parsed Rust item receiving the attribute.
    pub(crate) item: Item,
    /// Options retained until semantic validation.
    pub(crate) options: Vec<RawOption>,
}

/// A single option with its original key span.
pub(crate) struct RawOption {
    /// Option key and its original source span.
    pub(crate) key: Ident,
    /// Parsed syntactic value, not yet checked against the option key.
    pub(crate) value: RawValue,
}

/// Syntactic values accepted by the common option grammar.
pub(crate) enum RawValue {
    /// A marker option with no assigned value.
    Flag,
    /// A string literal option value.
    String(LitStr),
    /// An integer literal with its source sign tracked separately.
    Integer {
        /// Integer token before semantic range checking.
        literal: LitInt,
        /// Whether a leading minus token was present.
        negative: bool,
    },
    /// A Rust type value such as `dyn Trait`.
    Type(Type),
    /// An identifier value such as a generated marker name.
    Ident(Ident),
}

impl Parse for RawOption {
    /// Parses one `key`, `key = "literal"`, or typed IoC option.
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let key = input.call(Ident::parse_any)?;
        let value = match key.to_string().as_str() {
            "primary" => RawValue::Flag,
            "id" | "profile" | "prefix" => {
                input.parse::<Token![=]>()?;
                RawValue::String(input.parse()?)
            }
            "order" => {
                input.parse::<Token![=]>()?;
                let negative = input.peek(Token![-]);
                if negative {
                    input.parse::<Token![-]>()?;
                }
                RawValue::Integer {
                    literal: input.parse()?,
                    negative,
                }
            }
            "bind" | "type" => {
                input.parse::<Token![=]>()?;
                RawValue::Type(input.parse()?)
            }
            "marker" => {
                input.parse::<Token![=]>()?;
                RawValue::Ident(input.parse()?)
            }
            "name" => {
                return Err(syn::Error::new(
                    key.span(),
                    "unknown IoC option `name`; use `id` for injection selection",
                ));
            }
            _ => {
                return Err(syn::Error::new(key.span(), format!("unknown IoC option `{key}`")));
            }
        };
        Ok(Self { key, value })
    }
}

/// Parses attribute tokens and the annotated Rust item without local semantic
/// checks.
pub(crate) fn parse(kind: MacroKind, attribute: TokenStream, item: TokenStream) -> syn::Result<RawDeclaration> {
    let options = parse_options(attribute)?;
    let item = syn::parse2(item)?;
    Ok(RawDeclaration { kind, item, options })
}

/// Parses a comma-separated option list, preserving each key's source span.
pub(crate) fn parse_options(tokens: TokenStream) -> syn::Result<Vec<RawOption>> {
    let parser = Punctuated::<RawOption, Token![,]>::parse_terminated;
    Ok(parser.parse2(tokens)?.into_iter().collect())
}

/// Creates a span-bearing error for a missing required option.
pub(crate) fn missing_option(span: Span, name: &str) -> syn::Error {
    syn::Error::new(span, format!("missing required `{name}` option"))
}
