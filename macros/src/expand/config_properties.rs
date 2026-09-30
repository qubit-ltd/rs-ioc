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
use syn::LitStr;
use syn::Result;

use crate::expand::ExpansionContext;
use crate::expand::internal_ident;
use crate::ir::ConfigurationPropertiesIr;

/// Emits the user's struct and its config-dependent typed factory.
///
/// The [`ConfigurationPropertiesIr`] is destructured, so the caller keeps no
/// handle on it afterwards, and the emitted stream is the original struct
/// followed by a `ComponentDefinition` implementation wrapped in the runtime's
/// configuration bridge macro. The bridge makes a program that declares a
/// configuration-backed definition fail to compile with an actionable message
/// when the configuration support is not enabled, instead of failing later with
/// an unresolved `Config` request.
///
/// The generated `register` method always declares exactly one graph request,
/// for the bridge `Config` type, and binds `id` and `profile` from the
/// declaration while leaving omitted properties as `None`. The factory first
/// resolves `Config` from the resolution context, mapping a missing value to
/// the runtime's `FactoryError`, and then deserializes the properties for
/// `prefix` and the declaring item name, so the configuration key space stays
/// derived from the declaration rather than from user input at run time. The
/// returned value is wrapped in an `Arc` because the definition is shared.
///
/// The tokens never reach the compiled program as data: expansion happens once
/// per declaration, at compile time, and discards the intermediate
/// representation immediately.
///
/// # Errors
///
/// Returns a span-aware [`syn::Error`] when the validation stage rejected the
/// declaration, so the caller reports the original diagnostic instead of
/// emitting a definition that cannot be built.
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
    let options_ident = internal_ident("options", 0);
    let dependencies_ident = internal_ident("dependencies", 0);
    let context_ident = internal_ident("context", 0);
    let config_ident = internal_ident("config", 0);
    let value_ident = internal_ident("value", 0);
    let definition_ident = internal_ident("definition", 0);
    let source_expr = quote_spanned! {source.span=>
        #runtime::DefinitionSource::new(
            ::core::env!("CARGO_PKG_NAME"), ::core::module_path!(), ::core::file!(),
            ::core::line!(), ::core::column!(), ::core::stringify!(#item_name),
        )
    };
    let id = optional_owned_literal(options.id.as_ref());
    let profile = optional_owned_literal(options.profile.as_ref());
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
                let #options_ident = #runtime::BindingOptions {
                    id: #id,
                    primary: #primary,
                    order: #order,
                    profile: #profile,
                };
                let #dependencies_ident = [
                    #runtime::Dependency::of::<#runtime::__private::codegen_v1::Config>()
                ];
                let #definition_ident = #runtime::Definition::<#ident>::builder()
                    .source(<#ident as #runtime::ComponentDefinition>::source())
                    .binding(#options_ident)
                    .dependencies(&#dependencies_ident)
                    .factory(|#context_ident| {
                        let #config_ident = #context_ident
                            .get::<#runtime::__private::codegen_v1::Config>()
                            .map_err(#runtime::FactoryError::new)?;
                        let #value_ident = #runtime::config::deserialize_properties_for::<#ident>(
                            #config_ident.as_ref(), #prefix, ::core::stringify!(#item_name),
                        )?;
                        ::std::result::Result::Ok(::std::sync::Arc::new(#value_ident))
                    })
                    .build()?;
                builder.register_definition(#definition_ident)
            }
        }
    };
    Ok(quote! {
        #item
        #runtime::__private::require_config! { #generated }
    })
}

/// Maps an optional declared property onto the binding-option token form.
///
/// A property the declaration omitted becomes `Option::None`, and a declared
/// literal becomes `Option::Some(<literal>.to_owned())` so the generated
/// program owns its copy of the text instead of borrowing the validator's
/// value. Both `id` and `profile` share this mapping so the emitted binding
/// options stay consistent.
///
/// The mapping is a single expression over one borrowed literal and allocates
/// nothing beyond the returned token stream, so it is inlined into its two
/// call sites in [`expand`].
#[must_use]
#[inline]
fn optional_owned_literal(value: Option<&LitStr>) -> TokenStream {
    value.map_or_else(
        || quote!(::std::option::Option::None),
        |literal| quote!(::std::option::Option::Some(#literal.to_owned())),
    )
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;
    use proc_macro2::TokenStream;
    use quote::quote;
    use syn::AngleBracketedGenericArguments;
    use syn::Expr;
    use syn::File;
    use syn::GenericArgument;
    use syn::ImplItem;
    use syn::ImplItemFn;
    use syn::Item;
    use syn::ItemStruct;
    use syn::Lit;
    use syn::Member;
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

    /// Navigates from generated tokens to the bridged registration method.
    fn generated_register(tokens: TokenStream) -> ImplItemFn {
        let generated: File = parse2(tokens).expect("generated config properties should be valid Rust AST");
        let Item::Macro(bridge) = &generated.items[1] else {
            panic!("expected config feature bridge macro")
        };
        let bridged: File = parse2(bridge.mac.tokens.clone()).expect("bridge contents should be valid Rust AST");
        let Item::Impl(registration) = &bridged.items[0] else {
            panic!("expected registration impl")
        };
        registration
            .items
            .iter()
            .find_map(|item| match item {
                ImplItem::Fn(function) if function.sig.ident == "register" => Some(function.clone()),
                _ => None,
            })
            .expect("expected registration method")
    }

    /// Returns the initializer of one field of the generated binding options.
    fn binding_options_field<'a>(register: &'a ImplItemFn, name: &str) -> &'a Expr {
        let Stmt::Local(options) = &register.block.stmts[0] else {
            panic!("expected binding options declaration")
        };
        let Expr::Struct(options) = options
            .init
            .as_ref()
            .expect("expected binding options initializer")
            .expr
            .as_ref()
        else {
            panic!("expected binding options literal")
        };
        let field = options
            .fields
            .iter()
            .find(|field| match &field.member {
                Member::Named(ident) => ident == name,
                Member::Unnamed(_) => false,
            })
            .unwrap_or_else(|| panic!("expected `{name}` binding option"));
        &field.expr
    }

    /// Checks that a declared binding option is emitted as an owned `Some`.
    fn assert_owned_some(value: &Expr, expected: &str) {
        let Expr::Call(some) = value else {
            panic!("expected Option::Some value")
        };
        let Expr::Path(constructor) = some.func.as_ref() else {
            panic!("expected Option::Some call")
        };
        assert_path(&constructor.path, &["std", "option", "Option", "Some"]);
        let Expr::MethodCall(owned) = &some.args[0] else {
            panic!("expected owned literal argument")
        };
        assert_eq!(owned.method, "to_owned");
        let Expr::Lit(literal) = owned.receiver.as_ref() else {
            panic!("expected string literal receiver")
        };
        let Lit::Str(text) = &literal.lit else {
            panic!("expected string literal")
        };
        assert_eq!(text.value(), expected);
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
            panic!("expected public definition registration");
        };
        assert_eq!(install.method, "register_definition");
        let Stmt::Local(definition) = &register.block.stmts[2] else {
            panic!("expected complete definition");
        };
        let Expr::Try(built) = definition.init.as_ref().expect("definition initializer").expr.as_ref() else {
            panic!("expected validated definition");
        };
        let Expr::MethodCall(build) = built.expr.as_ref() else {
            panic!("expected build call");
        };
        assert_eq!(build.method, "build");
        let Expr::MethodCall(constructor) = build.receiver.as_ref() else {
            panic!("expected public factory setter");
        };
        assert_eq!(constructor.method, "factory");
        let Expr::Closure(factory) = &constructor.args[0] else {
            panic!("expected properties factory");
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

    #[test]
    fn test_config_properties_forwards_declared_id_and_profile() {
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
                id: Some(parse_quote!("server.settings.id")),
                binds: Vec::new(),
                primary: false,
                order: 0,
                profile: Some(parse_quote!("production")),
            },
            source,
        };
        let context = ExpansionContext {
            runtime: quote!(::renamed_ioc),
        };
        let tokens = super::expand(value, &context).expect("config properties expansion should succeed");
        let register = generated_register(tokens);
        assert_owned_some(binding_options_field(&register, "id"), "server.settings.id");
        assert_owned_some(binding_options_field(&register, "profile"), "production");
    }
}
