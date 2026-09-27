// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Generates component definitions from named-field and unit structs.

use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use syn::LitStr;

use crate::expand::ExpansionContext;
use crate::expand::value;
use crate::ir::BindingOptions;
use crate::ir::ComponentIr;
use crate::ir::DependencyIr;
use crate::ir::DependencyKind;

/// Emits the original struct, a typed factory and its linked registration
/// entry.
pub(crate) fn expand(value: ComponentIr, context: &ExpansionContext) -> syn::Result<TokenStream> {
    let ComponentIr {
        item,
        options,
        fields,
        source,
        ..
    } = value;
    let runtime = &context.runtime;
    let ident = &item.ident;
    let item_name = &source.item;
    let source_expr = quote_spanned! {source.span=>
        #runtime::DefinitionSource::new(
            env!("CARGO_PKG_NAME"), module_path!(), file!(), line!(), column!(),
            stringify!(#item_name),
        )
    };
    let mut dependencies = Vec::with_capacity(fields.len());
    let mut initializers = Vec::with_capacity(fields.len());
    let requires_config = fields
        .iter()
        .any(|field| matches!(field.dependency.kind, DependencyKind::Value { .. }));
    for field in &fields {
        let field_ident = &field.ident;
        let dependency = &field.dependency;
        if matches!(dependency.kind, DependencyKind::Value { .. }) {
            dependencies.push(value::config_request(runtime));
        } else {
            dependencies.push(dependency_tokens(dependency, runtime));
        }
        let expression = field_expression(dependency, field_ident, runtime);
        initializers.push(quote!(#field_ident: #expression));
    }

    let construct = if matches!(item.fields, syn::Fields::Unit) {
        quote!(#ident)
    } else {
        quote!(#ident { #(#initializers),* })
    };
    let concrete_options = options_tokens(&options, runtime, options.binds.is_empty());
    let mut aliases = Vec::with_capacity(options.binds.len());
    for bind in &options.binds {
        let alias_options = options_tokens(&options, runtime, true);
        aliases.push(quote! {
            __qubit_ioc_draft.bind::<#bind, _>(#alias_options, |concrete| {
                let alias: ::std::sync::Arc<#bind> = concrete;
                alias
            })?;
        });
    }

    let generated = quote! {
        impl #runtime::ComponentDefinition for #ident {
            fn source() -> #runtime::DefinitionSource {
                #source_expr
            }

            fn register(
                builder: &mut #runtime::ContainerBuilder,
            ) -> ::std::result::Result<(), #runtime::RegistrationError> {
                let mut __qubit_ioc_dependencies = ::std::vec::Vec::<#runtime::Dependency>::new();
                for request in [#(#dependencies),*] {
                    if !__qubit_ioc_dependencies.contains(&request) {
                        __qubit_ioc_dependencies.push(request);
                    }
                }
                let mut __qubit_ioc_draft =
                    #runtime::__private::codegen_v1::DefinitionDraft::<#ident>::new_sync(
                        <#ident as #runtime::ComponentDefinition>::source(),
                        &__qubit_ioc_dependencies,
                        #concrete_options,
                        |__qubit_ioc_context| {
                            ::std::result::Result::Ok(::std::sync::Arc::new(#construct))
                        },
                    )?;
                #(#aliases)*
                __qubit_ioc_draft.register(builder)
            }
        }

    };
    let generated = if requires_config {
        quote!(#runtime::__private::require_config! { #generated })
    } else {
        generated
    };
    Ok(quote! { #item #generated })
}

/// Converts one normalized dependency into its graph declaration.
fn dependency_tokens(dependency: &DependencyIr, runtime: &TokenStream) -> TokenStream {
    let requested_type = &dependency.requested_type;
    match (&dependency.kind, &dependency.id) {
        (DependencyKind::Required, Some(id)) => {
            quote!(#runtime::Dependency::with_id::<#requested_type>(#id))
        }
        (DependencyKind::Required, None) => quote!(#runtime::Dependency::of::<#requested_type>()),
        (DependencyKind::Optional, Some(id)) => {
            quote!(#runtime::Dependency::optional_with_id::<#requested_type>(#id))
        }
        (DependencyKind::Optional, None) => {
            quote!(#runtime::Dependency::optional::<#requested_type>())
        }
        (DependencyKind::All, _) => quote!(#runtime::Dependency::all::<#requested_type>()),
        (DependencyKind::Value { .. }, _) => unreachable!("value fields use Config dependency"),
    }
}

/// Builds one field from the exact request registered above or a Config read.
fn field_expression(dependency: &DependencyIr, field_ident: &syn::Ident, runtime: &TokenStream) -> TokenStream {
    let requested_type = &dependency.requested_type;
    let context = quote!(__qubit_ioc_context);
    let access = match (&dependency.kind, &dependency.id) {
        (DependencyKind::Required, Some(id)) => {
            quote!(#context.get_by_id::<#requested_type>(#id))
        }
        (DependencyKind::Required, None) => quote!(#context.get::<#requested_type>()),
        (DependencyKind::Optional, Some(id)) => {
            quote!(#context.try_get_by_id::<#requested_type>(#id))
        }
        (DependencyKind::Optional, None) => quote!(#context.try_get::<#requested_type>()),
        (DependencyKind::All, _) => quote!(#context.get_all::<#requested_type>()),
        (DependencyKind::Value { .. }, _) => {
            return value::read_value(dependency, field_ident, runtime, &context);
        }
    };
    quote!(#access.map_err(#runtime::FactoryError::new)?)
}

/// Uses interface ordering metadata only on interface aliases when any exist.
fn options_tokens(options: &BindingOptions, runtime: &TokenStream, selection: bool) -> TokenStream {
    let id = optional_string(&options.id);
    let profile = optional_string(&options.profile);
    let primary = options.primary && selection;
    let order = if selection { options.order } else { 0 };
    quote! {
        #runtime::BindingOptions {
            id: #id,
            primary: #primary,
            order: #order,
            profile: #profile,
        }
    }
}

/// Keeps an omitted literal as `None` and preserves supplied text exactly.
fn optional_string(value: &Option<LitStr>) -> TokenStream {
    match value {
        Some(value) => quote!(::std::option::Option::Some(#value.to_owned())),
        None => quote!(::std::option::Option::None),
    }
}
