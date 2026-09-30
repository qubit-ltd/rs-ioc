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

/// Normalizes declaration activation attributes into plain `cfg` attributes.
///
/// Declarations are written once and may be re-emitted by several generated
/// items, so every consumer needs the same activation semantics expressed as
/// `cfg` predicates rather than as `cfg_attr` chains that expand helpers
/// differently per item. Existing `cfg` attributes are preserved verbatim and
/// cloned; a `cfg_attr(predicate, rest...)` is rewritten so that `rest` applies
/// only when `predicate` holds, and nested `cfg` predicates under an inherited
/// condition become the implication `any(not(condition), nested)`. All other
/// attributes are ignored.
///
/// This helper allocates a fresh attribute vector, clones the accepted
/// attributes, and runs inside procedural-macro expansion, so it performs no
/// I/O and holds no state across calls.
///
/// # Parameters
///
/// `attributes` holds the outer attributes written on the declaration, field,
/// or function currently being expanded. It is borrowed for the duration of
/// the call and is never mutated.
///
/// # Returns
///
/// Returns `Ok` with one `cfg` attribute per activated clause, in source order.
/// `Ok` with an empty vector means the input carried no `cfg` or `cfg_attr`
/// attribute and therefore never restricts generation.
///
/// # Errors
///
/// Returns a [`syn::Error`] located at the offending attribute when a
/// `cfg_attr` argument list is not valid `Meta` syntax, when a `cfg_attr`
/// carries no condition, or when the nested `cfg`/`cfg_attr` form is not a
/// `Meta::List`. Clauses already appended before the failure stay in the
/// partially built vector, which the caller discards on error.
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

/// Appends normalized activation attributes under an inherited condition.
///
/// Nested `cfg_attr` predicates are recursively conjoined with `condition`;
/// each nested `cfg` becomes an implication from that combined condition to
/// its predicate. Other attributes are ignored without changing `result`.
///
/// # Parameters
///
/// `result` receives normalized `cfg` attributes, `meta` is one nested
/// attribute, and `condition` contains all enclosing activation predicates.
///
/// # Returns
///
/// Returns `Ok(())` after normalizing or ignoring the nested attribute.
///
/// # Errors
///
/// Returns a syntax error for non-list `cfg`/`cfg_attr`, missing or unparseable
/// nested `cfg_attr` metadata, or nested `inject`/`value` helpers. Attributes
/// appended before an error remain in `result`.
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
