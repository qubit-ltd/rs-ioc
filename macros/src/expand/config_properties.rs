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
use syn::Result;

use crate::expand::ExpansionContext;
use crate::ir::ConfigurationPropertiesIr;

/// Emits the user's struct and its config-dependent typed factory.
pub(crate) fn expand(value: ConfigurationPropertiesIr, context: &ExpansionContext) -> Result<TokenStream> {
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
            ::core::env!("CARGO_PKG_NAME"), ::core::module_path!(), ::core::file!(),
            ::core::line!(), ::core::column!(), ::core::stringify!(#item_name),
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
                            __qubit_ioc_config.as_ref(), #prefix, ::core::stringify!(#item_name),
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

#[cfg(test)]
mod tests {
    use proc_macro2::Span;
    use quote::quote;
    use syn::AngleBracketedGenericArguments;
    use syn::Expr;
    use syn::File;
    use syn::GenericArgument;
    use syn::ImplItem;
    use syn::Item;
    use syn::ItemStruct;
    use syn::Lit;
    use syn::Path;
    use syn::PathArguments;
    use syn::Stmt;
    use syn::Type;
    use syn::parse_quote;
    use syn::parse2;

    use crate::expand::ExpansionContext;
    use crate::ir::BindingOptions;
    use crate::ir::ConfigurationPropertiesIr;
    use crate::ir::SourceIr;

    /// Checks a generated absolute path without relying on token formatting.
    fn assert_path(path: &Path, expected: &[&str]) {
        assert!(path.leading_colon.is_some(), "runtime bridge must use an absolute path");
        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        assert_eq!(segments, expected);
    }

    /// Checks the single Config type argument used by graph requests and reads.
    fn assert_config_argument(arguments: &AngleBracketedGenericArguments) {
        assert_eq!(arguments.args.len(), 1);
        let GenericArgument::Type(Type::Path(config)) = &arguments.args[0] else {
            panic!("expected Config type argument");
        };
        assert_path(&config.path, &["renamed_ioc", "__private", "codegen_v1", "Config"]);
    }

    #[test]
    fn test_config_properties_emits_prefix_and_runtime_config_bridge() {
        let item: ItemStruct = parse_quote! {
            struct Settings { port: u16 }
        };
        let source = SourceIr {
            item: item.ident.clone(),
            span: Span::call_site(),
        };
        let value = ConfigurationPropertiesIr {
            item,
            prefix: parse_quote!("server.settings"),
            options: BindingOptions {
                id: None,
                binds: Vec::new(),
                primary: false,
                order: 0,
                profile: None,
            },
            source,
        };
        let context = ExpansionContext {
            runtime: quote!(::renamed_ioc),
        };
        let tokens = super::expand(value, &context).expect("config properties expansion should succeed");
        let generated: File = parse2(tokens).expect("generated config properties should be valid Rust AST");
        assert_eq!(generated.items.len(), 2);
        let Item::Struct(definition) = &generated.items[0] else {
            panic!("expected original properties struct")
        };
        assert_eq!(definition.ident, "Settings");
        let Item::Macro(bridge) = &generated.items[1] else {
            panic!("expected config feature bridge macro")
        };
        assert_path(&bridge.mac.path, &["renamed_ioc", "__private", "require_config"]);
        let bridged: File = parse2(bridge.mac.tokens.clone()).expect("bridge contents should be valid Rust AST");
        assert_eq!(bridged.items.len(), 1);
        let Item::Impl(registration) = &bridged.items[0] else {
            panic!("expected registration impl")
        };
        let (_, trait_path, _) = registration.trait_.as_ref().expect("expected registration trait");
        assert_path(trait_path, &["renamed_ioc", "ComponentDefinition"]);
        let register = registration
            .items
            .iter()
            .find_map(|item| match item {
                ImplItem::Fn(function) if function.sig.ident == "register" => Some(function),
                _ => None,
            })
            .expect("expected registration method");
        let Stmt::Local(dependencies) = &register.block.stmts[1] else {
            panic!("expected dependency declaration")
        };
        let Expr::Array(requests) = dependencies
            .init
            .as_ref()
            .expect("expected dependency initializer")
            .expr
            .as_ref()
        else {
            panic!("expected dependency array");
        };
        assert_eq!(requests.elems.len(), 1);
        let Expr::Call(request) = &requests.elems[0] else {
            panic!("expected dependency request")
        };
        let Expr::Path(request_path) = request.func.as_ref() else {
            panic!("expected dependency function path")
        };
        assert_path(&request_path.path, &["renamed_ioc", "Dependency", "of"]);
        let PathArguments::AngleBracketed(arguments) = &request_path
            .path
            .segments
            .last()
            .expect("expected of segment")
            .arguments
        else {
            panic!("expected Config request argument");
        };
        assert_config_argument(arguments);

        let Stmt::Expr(Expr::MethodCall(install), None) =
            register.block.stmts.last().expect("expected registration call")
        else {
            panic!("expected draft registration");
        };
        assert_eq!(install.method, "register");
        let Expr::Try(draft) = install.receiver.as_ref() else {
            panic!("expected fallible draft creation")
        };
        let Expr::Call(constructor) = draft.expr.as_ref() else {
            panic!("expected draft constructor")
        };
        let Expr::Path(constructor_path) = constructor.func.as_ref() else {
            panic!("expected draft constructor path")
        };
        assert_path(
            &constructor_path.path,
            &["renamed_ioc", "__private", "codegen_v1", "DefinitionDraft", "new_sync"],
        );
        let Expr::Closure(factory) = &constructor.args[3] else {
            panic!("expected properties factory")
        };
        let Expr::Block(factory_body) = factory.body.as_ref() else {
            panic!("expected factory block")
        };
        let Stmt::Local(config) = &factory_body.block.stmts[0] else {
            panic!("expected Config lookup")
        };
        let Expr::Try(lookup) = config.init.as_ref().expect("expected Config initializer").expr.as_ref() else {
            panic!("expected fallible Config lookup");
        };
        let Expr::MethodCall(map_error) = lookup.expr.as_ref() else {
            panic!("expected error mapping")
        };
        let Expr::MethodCall(get_config) = map_error.receiver.as_ref() else {
            panic!("expected Config get call")
        };
        assert_eq!(get_config.method, "get");
        assert_config_argument(
            get_config
                .turbofish
                .as_ref()
                .expect("get must name the bridge Config type"),
        );
        let Stmt::Local(properties) = &factory_body.block.stmts[1] else {
            panic!("expected deserialization")
        };
        let Expr::Try(deserialization) = properties
            .init
            .as_ref()
            .expect("expected properties initializer")
            .expr
            .as_ref()
        else {
            panic!("expected fallible deserialization");
        };
        let Expr::Call(deserialize) = deserialization.expr.as_ref() else {
            panic!("expected deserialize call")
        };
        let Expr::Path(deserialize_path) = deserialize.func.as_ref() else {
            panic!("expected deserialize function path")
        };
        assert_path(
            &deserialize_path.path,
            &["renamed_ioc", "config", "deserialize_properties_for"],
        );
        let Expr::Lit(prefix) = &deserialize.args[1] else {
            panic!("expected prefix literal")
        };
        let Lit::Str(prefix) = &prefix.lit else {
            panic!("expected string prefix")
        };
        assert_eq!(prefix.value(), "server.settings");
    }
}
