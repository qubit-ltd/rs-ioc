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
use syn::Error;
use syn::Ident;
use syn::Item;
use syn::LitStr;
use syn::Meta;
use syn::Result;
use syn::parse_quote;

use crate::conditions::activation_attributes;
use crate::expand::ExpansionContext;
use crate::expand::bean::default_marker;
use crate::ir::ConfigurationIr;
use crate::parse::RawValue;
use crate::parse::parse_options;

/// Expands a `#[Configuration]` group into the module that carries its beans.
///
/// The annotated item must be an inline module. Every direct child function
/// that carries a `#[bean]` attribute contributes one installation call, and
/// the module gains a single generated `register_ioc` function that replays
/// those calls in source order. When the group declares a profile, it is
/// written onto each child that does not already select one. The returned
/// stream is the whole rewritten module, so the caller's surrounding items are
/// unaffected.
///
/// # Errors
///
/// Returns an error when the annotated item is not an inline module, when a
/// child's activation attributes are malformed, or when a `#[bean]` option list
/// cannot be parsed.
pub(crate) fn expand(value: ConfigurationIr, context: &ExpansionContext) -> Result<TokenStream> {
    let ConfigurationIr {
        mut item,
        profile,
        source,
    } = value;
    let _ = source;
    let Some((_, children)) = &mut item.content else {
        return Err(Error::new(
            item.ident.span(),
            "#[Configuration] requires an inline module",
        ));
    };
    let installations = collect_installations(children, profile.as_ref())?;
    children.push(registration_function(&installations, &context.runtime));
    Ok(quote!(#item))
}

/// Collects one installation statement per direct bean child, in source order.
///
/// Only direct function children carrying a `#[bean]` attribute contribute;
/// other children and nested modules are skipped. Each contributing child is
/// also rewritten in place so it inherits the group profile when it has no
/// explicit one, which is why the slice is taken mutably.
fn collect_installations(children: &mut [Item], profile: Option<&LitStr>) -> Result<Vec<TokenStream>> {
    let mut installations = Vec::new();
    for child in children {
        let Item::Fn(function) = child else { continue };
        let function_name = function.sig.ident.clone();
        let cfg_attributes = activation_attributes(&function.attrs)?;
        let Some(attribute) = function.attrs.iter_mut().find(|attr| is_bean(attr)) else {
            continue;
        };
        let (marker, has_profile) = bean_options(attribute, &function_name)?;
        installations.push(quote!(#(#cfg_attributes)* builder.install::<#marker>()?;));
        if let Some(default_profile) = profile
            && !has_profile
        {
            add_profile(attribute, default_profile);
        }
    }
    Ok(installations)
}

/// Builds the public `register_ioc` function that replays the collected calls.
fn registration_function(installations: &[TokenStream], runtime: &TokenStream) -> Item {
    parse_quote! {
        /// Installs this module's direct bean definitions in source order.
        pub fn register_ioc(
            builder: &mut #runtime::ContainerBuilder,
        ) -> ::core::result::Result<(), #runtime::RegistrationError> {
            #(#installations)*
            ::core::result::Result::Ok(())
        }
    }
}

/// Reports whether an attribute is the bean marker, qualified paths included.
///
/// Only the final path segment is compared, so `#[bean]` and
/// `#[qubit_ioc::bean]` both match while `#[beans]` does not. The check borrows
/// the attribute only and performs no allocation.
fn is_bean(attribute: &Attribute) -> bool {
    attribute
        .path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "bean")
}

/// Reads the marker option and whether the child already selected a profile.
///
/// `function_name` supplies the fallback marker derived from the child's own
/// name when no `marker = ...` option is present, so a bare `#[bean]` or an
/// option list without a recognized marker still yields a usable marker. The
/// returned flag is `true` only when the child already carries a `profile =
/// "..."` string, which tells the caller to leave that profile in place.
///
/// # Errors
///
/// Returns an error when the attribute's option list cannot be parsed as
/// `key = value` pairs.
fn bean_options(attribute: &Attribute, function_name: &Ident) -> Result<(Ident, bool)> {
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

/// Appends `profile = ...` to a bean attribute in place.
///
/// A trailing comma in the current option list is dropped first so the appended
/// option is not separated by a doubled comma, and an empty or non-list
/// attribute becomes a fresh single-option list. The attribute's path is
/// preserved, so a qualified `#[qubit_ioc::bean(...)]` stays qualified. Callers
/// must only invoke this for attributes that do not already declare a profile.
fn add_profile(attribute: &mut Attribute, profile: &LitStr) {
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
    *attribute = parse_quote!(#[#path(#tokens)]);
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;
    use quote::quote;
    use syn::Expr;
    use syn::File;
    use syn::GenericArgument;
    use syn::Item;
    use syn::ItemFn;
    use syn::ItemMod;
    use syn::Lit;
    use syn::LitStr;
    use syn::Meta;
    use syn::Stmt;
    use syn::Type;
    use syn::Visibility;
    use syn::parse_quote;
    use syn::parse2;
    use syn::punctuated::Punctuated;
    use syn::token::Comma;

    use crate::expand::ExpansionContext;
    use crate::ir::ConfigurationIr;
    use crate::ir::SourceIr;

    /// Expands an inline group and returns its actual generated child items.
    fn expand_group(item: ItemMod, profile: Option<LitStr>) -> Vec<Item> {
        let source = SourceIr {
            item: item.ident.clone(),
            span: Span::call_site(),
        };
        let value = ConfigurationIr { item, profile, source };
        let context = ExpansionContext {
            runtime: quote!(::qubit_ioc),
        };
        let tokens = super::expand(value, &context).expect("configuration expansion should succeed");
        let generated: File = parse2(tokens).expect("generated configuration should be valid Rust AST");
        let Item::Mod(module) = generated
            .items
            .into_iter()
            .next()
            .expect("expected configuration module")
        else {
            panic!("expected module");
        };
        let (_, children) = module.content.expect("expected inline module");
        children
    }

    #[test]
    fn test_configuration_emits_registration_in_declaration_order() {
        let children = expand_group(
            parse_quote! {
                mod group {
                    #[cfg(feature = "enabled")]
                    #[qubit_ioc::bean]
                    fn foo_bar() -> u8 { 1 }

                    fn helper() -> u8 { 2 }

                    mod nested {
                        #[qubit_ioc::bean]
                        fn ignored() -> u8 { 3 }
                    }

                    #[qubit_ioc::bean(marker = SelectedFactory)]
                    fn second() -> u8 { 4 }
                }
            },
            None,
        );
        let registrations = children
            .iter()
            .filter_map(|item| match item {
                Item::Fn(function) if function.sig.ident == "register_ioc" => Some(function),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(registrations.len(), 1, "one manual registration entry must be emitted");
        let register = registrations[0];
        assert!(matches!(register.vis, Visibility::Public(_)));
        assert_eq!(
            register.block.stmts.len(),
            3,
            "only direct beans should be installed before returning Ok"
        );
        let mut markers = Vec::new();
        for (index, statement) in register.block.stmts[..2].iter().enumerate() {
            let Stmt::Expr(Expr::Try(installation), Some(_)) = statement else {
                panic!("expected fallible installation")
            };
            assert_eq!(
                installation.attrs.len(),
                usize::from(index == 0),
                "bean activation should gate its install call"
            );
            if index == 0 {
                assert!(installation.attrs[0].path().is_ident("cfg"));
            }
            let Expr::MethodCall(call) = installation.expr.as_ref() else {
                panic!("expected install method call")
            };
            assert_eq!(call.method, "install");
            let Expr::Path(receiver) = call.receiver.as_ref() else {
                panic!("expected builder receiver")
            };
            assert!(receiver.path.is_ident("builder"));
            assert!(call.args.is_empty());
            let arguments = &call.turbofish.as_ref().expect("install must name a marker").args;
            assert_eq!(arguments.len(), 1);
            let GenericArgument::Type(Type::Path(marker)) = &arguments[0] else {
                panic!("expected marker type")
            };
            markers.push(
                marker
                    .path
                    .get_ident()
                    .expect("expected unqualified marker")
                    .to_string(),
            );
        }
        assert_eq!(markers, ["FooBarBean", "SelectedFactory"]);
    }

    /// Reads one bean's generated profile as a parsed attribute argument.
    fn bean_profile(function: &ItemFn) -> Option<String> {
        let bean = function
            .attrs
            .iter()
            .find(|attribute| {
                attribute
                    .path()
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "bean")
            })
            .expect("expected bean attribute");
        let options = bean
            .parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)
            .expect("bean options should be valid attribute arguments");
        options.into_iter().find_map(|option| {
            let Meta::NameValue(option) = option else {
                return None;
            };
            if !option.path.is_ident("profile") {
                return None;
            }
            let Expr::Lit(literal) = option.value else {
                panic!("expected profile literal")
            };
            let Lit::Str(profile) = literal.lit else {
                panic!("expected profile string")
            };
            Some(profile.value())
        })
    }

    #[test]
    fn test_configuration_inherits_profile_and_preserves_override() {
        let children = expand_group(
            parse_quote! {
                mod group {
                    #[qubit_ioc::bean(marker = FirstFactory,)]
                    fn first() -> u8 { 1 }

                    #[qubit_ioc::bean(profile = "child")]
                    fn second() -> u8 { 2 }
                }
            },
            Some(parse_quote!("group")),
        );
        let profiles = children
            .iter()
            .filter_map(|item| match item {
                Item::Fn(function) if function.sig.ident != "register_ioc" => bean_profile(function),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(profiles, ["group", "child"]);
    }
}
