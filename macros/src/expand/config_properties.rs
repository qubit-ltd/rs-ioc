// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Generates configuration-backed component definitions.

use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;

use crate::expand::ExpansionContext;
use crate::ir::ConfigurationPropertiesIr;

/// Emits the user's struct and its config-dependent typed factory.
pub(crate) fn expand(value: ConfigurationPropertiesIr, context: &ExpansionContext) -> syn::Result<TokenStream> {
    let ConfigurationPropertiesIr {
        item,
        prefix,
        options,
        source,
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
    let id = options.id.as_ref().map_or_else(
        || quote!(::std::option::Option::None),
        |id| quote!(::std::option::Option::Some(#id.to_owned())),
    );
    let profile = options.profile.as_ref().map_or_else(
        || quote!(::std::option::Option::None),
        |profile| quote!(::std::option::Option::Some(#profile.to_owned())),
    );
    let primary = options.primary;
    let order = options.order;

    let generated = quote! {
        impl #runtime::ComponentDefinition for #ident {
            fn source() -> #runtime::DefinitionSource {
                #source_expr
            }

            fn register(
                builder: &mut #runtime::ContainerBuilder,
            ) -> ::std::result::Result<(), #runtime::RegistrationError> {
                let __qubit_ioc_options = #runtime::BindingOptions {
                    id: #id,
                    primary: #primary,
                    order: #order,
                    profile: #profile,
                };
                let __qubit_ioc_dependencies = [
                    #runtime::Dependency::of::<#runtime::__private::codegen_v1::Config>()
                ];
                #runtime::__private::codegen_v1::DefinitionDraft::<#ident>::new_sync(
                    <#ident as #runtime::ComponentDefinition>::source(),
                    &__qubit_ioc_dependencies,
                    __qubit_ioc_options,
                    |__qubit_ioc_context| {
                        let __qubit_ioc_config = __qubit_ioc_context
                            .get::<#runtime::__private::codegen_v1::Config>()
                            .map_err(#runtime::FactoryError::new)?;
                        let __qubit_ioc_value = #runtime::config::deserialize_properties_for::<#ident>(
                            __qubit_ioc_config.as_ref(), #prefix, stringify!(#item_name),
                        )?;
                        ::std::result::Result::Ok(::std::sync::Arc::new(__qubit_ioc_value))
                    },
                )?.register(builder)
            }
        }
    };
    Ok(quote! {
        #item
        #runtime::__private::require_config! { #generated }
    })
}
