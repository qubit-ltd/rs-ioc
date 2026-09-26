// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Applies module defaults to direct beans and emits a manual group entry.

use proc_macro2::TokenStream;
use proc_macro2::TokenTree;
use quote::quote;
use syn::Attribute;
use syn::Ident;
use syn::Item;
use syn::Meta;

use crate::expand::ExpansionContext;
use crate::expand::bean::default_marker;
use crate::ir::ConfigurationIr;
use crate::parse::RawValue;
use crate::parse::parse_options;

/// Emits an inline module with one ordered `register_ioc` function.
pub(crate) fn expand(value: ConfigurationIr, context: &ExpansionContext) -> syn::Result<TokenStream> {
    let ConfigurationIr {
        mut item,
        profile,
        source,
    } = value;
    let _ = source;
    let runtime = &context.runtime;
    let Some((_, children)) = &mut item.content else {
        return Err(syn::Error::new(
            item.ident.span(),
            "#[Configuration] requires an inline module",
        ));
    };
    let mut installations = Vec::new();
    for child in children.iter_mut() {
        let Item::Fn(function) = child else { continue };
        let function_name = function.sig.ident.clone();
        let cfg_attributes = function
            .attrs
            .iter()
            .filter(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
            .cloned()
            .collect::<Vec<_>>();
        let Some(attribute) = function.attrs.iter_mut().find(|attr| is_bean(attr)) else {
            continue;
        };
        let (marker, has_profile) = bean_options(attribute, &function_name)?;
        installations.push(quote!(#(#cfg_attributes)* builder.install::<#marker>()?;));
        if let Some(default_profile) = &profile
            && !has_profile
        {
            add_profile(attribute, default_profile);
        }
    }
    children.push(syn::parse_quote! {
        /// Installs this module's direct bean definitions in source order.
        pub fn register_ioc(builder: &mut #runtime::ContainerBuilder) -> Result<(), #runtime::RegistrationError> {
            #(#installations)*
            Ok(())
        }
    });
    Ok(quote!(#item))
}

/// Recognizes the direct child bean attribute, including a qualified path.
fn is_bean(attribute: &Attribute) -> bool {
    attribute
        .path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "bean")
}

/// Reads the marker option and whether the child already selected a profile.
fn bean_options(attribute: &Attribute, function_name: &Ident) -> syn::Result<(Ident, bool)> {
    let mut marker = None;
    let mut has_profile = false;
    let Meta::List(list) = &attribute.meta else {
        return Ok((default_marker(function_name), false));
    };
    for option in parse_options(list.tokens.clone())? {
        match (option.key.to_string().as_str(), option.value) {
            ("marker", RawValue::Ident(value)) => marker = Some(value),
            ("profile", RawValue::String(_)) => has_profile = true,
            _ => {}
        }
    }
    Ok((marker.unwrap_or_else(|| default_marker(function_name)), has_profile))
}

/// Adds the module's profile only where the child has no explicit override.
fn add_profile(attribute: &mut Attribute, profile: &syn::LitStr) {
    let path = attribute.path().clone();
    let tokens = match &attribute.meta {
        Meta::List(list) if !list.tokens.is_empty() => {
            let mut existing = list.tokens.clone().into_iter().collect::<Vec<_>>();
            if matches!(existing.last(), Some(TokenTree::Punct(punctuation)) if punctuation.as_char() == ',') {
                existing.pop();
            }
            let existing = TokenStream::from_iter(existing);
            quote!(#existing, profile = #profile)
        }
        _ => quote!(profile = #profile),
    };
    *attribute = syn::parse_quote!(#[#path(#tokens)]);
}
