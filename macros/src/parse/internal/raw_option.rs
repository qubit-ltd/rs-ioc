// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Parsed option syntax and its original key span.

use syn::Error;
use syn::Ident;
use syn::Result;
use syn::Token;
use syn::ext::IdentExt;
use syn::parse::Parse;
use syn::parse::ParseStream;

use crate::parse::RawValue;

/// One option parsed from an attribute before macro-specific validation.
///
/// The key retains the user's identifier and source span for later validation
/// and diagnostics. The value records only the syntax selected by the key's
/// spelling; whether that key is allowed for a particular macro and whether
/// the value is semantically valid are checked in the validation stage.
pub(crate) struct RawOption {
    /// Option key and its original source span.
    pub(crate) key: Ident,
    /// Parsed syntactic value, not yet checked against the option key.
    pub(crate) value: RawValue,
}
impl Parse for RawOption {
    /// Parses one IoC option according to its key's syntactic value shape.
    ///
    /// `primary` is parsed as a bare flag; `id`, `profile`, and `prefix` take
    /// string literals; `order` takes an integer with an optional leading
    /// minus; `bind` and `type` take Rust types; and `marker` takes an
    /// identifier. This stage does not check which macro accepts a key or
    /// whether the parsed value satisfies that option's semantic constraints.
    /// Unknown keys, including the reserved `name` spelling, fail at the key
    /// span so the diagnostic points to the user's option.
    fn parse(input: ParseStream<'_>) -> Result<Self> {
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
                return Err(Error::new(
                    key.span(),
                    "unknown IoC option `name`; use `id` for injection selection",
                ));
            }
            _ => {
                return Err(Error::new(key.span(), format!("unknown IoC option `{key}`")));
            }
        };
        Ok(Self { key, value })
    }
}
