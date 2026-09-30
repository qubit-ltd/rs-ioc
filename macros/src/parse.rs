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

use crate::ir::MacroKind;

mod internal;

pub(crate) use internal::RawDeclaration;
pub(crate) use internal::RawOption;
pub(crate) use internal::RawValue;

/// Parses attribute tokens and the annotated Rust item without local semantic
/// checks.
///
/// This is the first stage of the macro pipeline. It only recovers syntax: the
/// option list becomes [`RawOption`] values and the annotated item becomes a
/// [`syn::Item`]. Whether the item kind matches the attribute, whether a
/// required option is present, and whether option keys agree with the item are
/// all decided later by `crate::validate`, so nothing here depends on the
/// runtime crate. The call runs entirely inside the proc-macro process,
/// performs no I/O, and keeps no state across invocations.
///
/// # Parameters
///
/// `kind` selects the declaration family and is only recorded on the returned
/// declaration; the same rules apply to every kind at this stage.
/// `attribute` holds the macro attribute arguments and `item` holds the
/// annotated item. Both are consumed here, because parsing takes ownership of
/// their token streams.
///
/// # Returns
///
/// `Ok` with a [`RawDeclaration`] carrying the parsed item and the option list
/// in source order. An empty attribute is not an error here: a declaration that
/// needs a required option is rejected by validation, which points at the
/// attribute item instead of the absent token.
///
/// # Errors
///
/// Returns the first [`syn::Error`] produced by [`parse_options`], located at
/// the offending option key, or the error produced by parsing `item` as a
/// [`syn::Item`], located at the annotated item. Only the first error survives,
/// so one bad option hides the rest until it is fixed.
pub(crate) fn parse(kind: MacroKind, attribute: TokenStream, item: TokenStream) -> Result<RawDeclaration> {
    let options = parse_options(attribute)?;
    let item = parse2(item)?;
    Ok(RawDeclaration { kind, item, options })
}

/// Parses a comma-separated option list, preserving each key's source span.
///
/// The list is parsed as `Punctuated<RawOption, Token![,]>` in terminated mode,
/// so a trailing comma is accepted and each element keeps the span of the key
/// the user wrote. Those spans let later stages report unknown keys or missing
/// required options at the original source position rather than at the whole
/// attribute.
///
/// # Parameters
///
/// `tokens` holds the option list tokens of one attribute. It is consumed by
/// this call, and the parsed elements are collected into a `Vec` in source
/// order.
///
/// # Returns
///
/// `Ok` with one [`RawOption`] per `key` or `key = value` element in source
/// order, and `Ok` with an empty vector when `tokens` is empty. The value
/// variant is chosen syntactically from the key spelling only; whether a flag
/// key was given a value, or a valued key was given none, is not checked here.
///
/// # Errors
///
/// Returns a [`syn::Error`] located at the option key when a key is not a valid
/// identifier, when the tokens do not form a terminated comma-separated list,
/// or when the key is unknown to the IoC option grammar, including the reserved
/// `name` spelling that reports the `id` replacement. Parsing stops at the
/// first failing element, so later options are not examined.
pub(crate) fn parse_options(tokens: TokenStream) -> Result<Vec<RawOption>> {
    let parser = Punctuated::<RawOption, Token![,]>::parse_terminated;
    Ok(parser.parse2(tokens)?.into_iter().collect())
}

/// Creates a span-bearing error for a missing required option.
///
/// This helper only builds the [`syn::Error`]; it does not report a failure by
/// itself, so callers pass it to `ok_or_else` to keep the surrounding
/// validation function free of duplicated message text. Interpolating `name`
/// keeps the diagnostic identical across every required option.
///
/// # Parameters
///
/// `span` is the source position reported for the error, normally the item that
/// requires the option. `name` is the option spelling without brackets, as
/// written by the user.
///
/// # Returns
///
/// A [`syn::Error`] whose message is `missing required `<name>` option` and
/// whose span is `span`. The value is not a failure on its own, so discarding
/// it produces no diagnostic.
pub(crate) fn missing_option(span: Span, name: &str) -> Error {
    Error::new(span, format!("missing required `{name}` option"))
}
