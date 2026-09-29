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

/// A single option with its original key span.
pub(crate) struct RawOption {
    /// Option key and its original source span.
    pub(crate) key: Ident,
    /// Parsed syntactic value, not yet checked against the option key.
    pub(crate) value: RawValue,
}
impl Parse for RawOption {
    /// Parses one `key`, `key = "literal"`, or typed IoC option.
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
