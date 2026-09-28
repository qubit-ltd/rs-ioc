// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Normalizes declaration activation conditions for generated code.

use proc_macro2::TokenStream;
use quote::ToTokens;
use quote::quote;
use syn::Attribute;
use syn::Error;
use syn::Meta;
use syn::Result;
use syn::parse::Parser;
use syn::parse_quote;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;

/// Returns equivalent `cfg` attributes for supported activation attributes.
pub(crate) fn activation_attributes(attributes: &[Attribute]) -> Result<Vec<Attribute>> {
    let mut result = Vec::new();
    for attribute in attributes {
        match attribute.path().get_ident().map(ToString::to_string).as_deref() {
            Some("cfg") => result.push(attribute.clone()),
            Some("cfg_attr") => {
                let metas = attribute.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)?;
                let mut metas = metas.into_iter();
                let Some(predicate) = metas.next() else {
                    return Err(Error::new(attribute.span(), "`cfg_attr` requires a condition"));
                };
                let condition = predicate.into_token_stream();
                for nested in metas {
                    append_nested(&mut result, nested, condition.clone())?;
                }
            }
            _ => {}
        }
    }
    Ok(result)
}

fn append_nested(result: &mut Vec<Attribute>, meta: Meta, condition: TokenStream) -> Result<()> {
    let path = meta.path();
    if path.is_ident("inject") || path.is_ident("value") {
        return Err(Error::new(
            meta.span(),
            "`cfg_attr` cannot contain `inject` or `value`; apply the helper attribute directly",
        ));
    }
    if path.is_ident("cfg") {
        let Meta::List(list) = meta else {
            return Err(Error::new(meta.span(), "`cfg` requires a predicate"));
        };
        let predicate = list.tokens;
        result.push(parse_quote!(#[cfg(any(not(#condition), #predicate))]));
    } else if path.is_ident("cfg_attr") {
        let span = meta.span();
        let Meta::List(list) = meta else {
            return Err(Error::new(span, "`cfg_attr` requires a condition"));
        };
        let nested = Parser::parse2(Punctuated::<Meta, Comma>::parse_terminated, list.tokens)?;
        let mut nested = nested.into_iter();
        let Some(predicate) = nested.next() else {
            return Err(Error::new(span, "`cfg_attr` requires a condition"));
        };
        let predicate = predicate.into_token_stream();
        let combined = quote!(all(#condition, #predicate));
        for child in nested {
            append_nested(result, child, combined.clone())?;
        }
    }
    Ok(())
}
