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
use syn::Fields;
use syn::Ident;
use syn::LitStr;
use syn::Result;

use crate::conditions::activation_attributes;
use crate::expand::ExpansionContext;
use crate::expand::value;
use crate::ir::BindingOptions;
use crate::ir::ComponentIr;
use crate::ir::DependencyIr;
use crate::ir::DependencyKind;

/// Emits the original struct, a typed factory and its linked registration
/// entry.
///
/// Returns a span-aware error if an activation attribute cannot be normalized.
pub(crate) fn expand(value: ComponentIr, context: &ExpansionContext) -> Result<TokenStream> {
    let ComponentIr {
        item,
        options,
        fields,
        source,
        ..
    } = value;
    let conditions = activation_attributes(&item.attrs)?;
    let runtime = &context.runtime;
    let ident = &item.ident;
    let item_name = &source.item;
    let source_expr = quote_spanned! {source.span=>
        #runtime::DefinitionSource::new(
            ::core::env!("CARGO_PKG_NAME"), ::core::module_path!(), ::core::file!(),
            ::core::line!(), ::core::column!(), ::core::stringify!(#item_name),
        )
    };
    let mut dependencies = Vec::with_capacity(fields.len());
    let mut initializers = Vec::with_capacity(fields.len());
    let mut validation_errors = Vec::new();
    for field in &fields {
        let field_ident = &field.ident;
        let conditions = &field.conditions;
        if let Some(error) = &field.validation_error {
            let error = error.to_compile_error();
            validation_errors.push(quote!(#(#conditions)* #error));
            initializers.push(quote!(#(#conditions)* #field_ident: loop {}));
            continue;
        }
        let dependency = &field.dependency;
        let request = if matches!(dependency.kind, DependencyKind::Value { .. }) {
            value::config_request(runtime)
        } else {
            dependency_tokens(dependency, runtime)
        };
        let request = if matches!(dependency.kind, DependencyKind::Value { .. }) {
            quote!(#runtime::__private::require_config! { #request })
        } else {
            request
        };
        dependencies.push(quote! {
            #(#conditions)* {
                let __qubit_ioc_request = #request;
                if !__qubit_ioc_dependencies.contains(&__qubit_ioc_request) {
                    __qubit_ioc_dependencies.push(__qubit_ioc_request);
                }
            }
        });
        let expression = field_expression(dependency, field_ident, runtime);
        let expression = if matches!(dependency.kind, DependencyKind::Value { .. }) {
            quote!(#runtime::__private::require_config! { #expression })
        } else {
            expression
        };
        initializers.push(quote!(#(#conditions)* #field_ident: #expression));
    }

    let construct = if matches!(item.fields, Fields::Unit) {
        quote!(#ident)
    } else {
        quote!(#ident { #(#initializers),* })
    };
    let concrete_options = options_tokens(&options, runtime, options.binds.is_empty());
    let validation_allow = if validation_errors.is_empty() {
        quote!()
    } else {
        quote!(#[allow(unreachable_code)])
    };
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
        #(#validation_errors)*
        #(#conditions)*
        impl #runtime::ComponentDefinition for #ident {
            fn source() -> #runtime::DefinitionSource {
                #source_expr
            }

            #validation_allow
            fn register(
                builder: &mut #runtime::ContainerBuilder,
            ) -> ::std::result::Result<(), #runtime::RegistrationError> {
                let mut __qubit_ioc_dependencies = ::std::vec::Vec::<#runtime::Dependency>::new();
                #(#dependencies)*
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
fn field_expression(dependency: &DependencyIr, field_ident: &Ident, runtime: &TokenStream) -> TokenStream {
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

#[cfg(test)]
mod tests {
    use proc_macro2::Span;
    use quote::quote;
    use syn::Expr;
    use syn::File;
    use syn::Item;
    use syn::ItemStruct;
    use syn::Lit;
    use syn::Meta;
    use syn::parse_quote;
    use syn::parse2;
    use syn::punctuated::Punctuated;
    use syn::token::Comma;

    use crate::expand::ExpansionContext;
    use crate::ir::BindingOptions;
    use crate::ir::ComponentIr;
    use crate::ir::SourceIr;

    /// Checks a feature predicate structurally, preserving the exact feature
    /// name.
    fn assert_feature(predicate: &Meta, expected: &str) {
        let Meta::NameValue(condition) = predicate else {
            panic!("expected feature predicate")
        };
        assert!(condition.path.is_ident("feature"));
        let Expr::Lit(literal) = &condition.value else {
            panic!("expected literal feature name")
        };
        let Lit::Str(feature) = &literal.lit else {
            panic!("expected string feature name")
        };
        assert_eq!(feature.value(), expected);
    }

    #[test]
    fn test_component_propagates_cfg_to_registration_impl() {
        let item: ItemStruct = parse_quote! {
            #[cfg(feature = "enabled")]
            #[cfg_attr(feature = "filtered", cfg(feature = "extra"), derive(Clone))]
            struct FooBar;
        };
        let source = SourceIr {
            item: item.ident.clone(),
            span: Span::call_site(),
        };
        let value = ComponentIr {
            item,
            options: BindingOptions {
                id: None,
                binds: Vec::new(),
                primary: false,
                order: 0,
                profile: None,
            },
            fields: Vec::new(),
            source,
        };
        let context = ExpansionContext {
            runtime: quote!(::qubit_ioc),
        };
        let tokens = super::expand(value, &context).expect("component expansion should succeed");
        let generated: File = parse2(tokens).expect("generated component should be valid Rust AST");
        assert_eq!(generated.items.len(), 2);
        let Item::Struct(definition) = &generated.items[0] else {
            panic!("expected original component struct");
        };
        let Item::Impl(registration) = &generated.items[1] else {
            panic!("expected registration impl");
        };
        let (_, trait_path, _) = registration
            .trait_
            .as_ref()
            .expect("expected ComponentDefinition trait");
        assert_eq!(
            trait_path
                .segments
                .last()
                .expect("trait path should not be empty")
                .ident,
            "ComponentDefinition"
        );
        for attributes in [&definition.attrs, &registration.attrs] {
            assert_eq!(
                attributes.len(),
                2,
                "definition and registration must have the same activation conditions"
            );
            assert!(attributes[0].path().is_ident("cfg"));
            assert_feature(
                &attributes[0].parse_args::<Meta>().expect("cfg predicate should parse"),
                "enabled",
            );
        }
        assert!(
            definition.attrs[1].path().is_ident("cfg_attr"),
            "original cfg_attr must be retained"
        );
        assert!(
            registration.attrs[1].path().is_ident("cfg"),
            "only activation should be propagated to the impl"
        );
        let Meta::List(activation) = registration.attrs[1]
            .parse_args::<Meta>()
            .expect("normalized cfg should parse")
        else {
            panic!("expected conditional activation predicate");
        };
        assert!(activation.path.is_ident("any"));
        let alternatives = activation
            .parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)
            .expect("activation alternatives should parse");
        assert_eq!(alternatives.len(), 2);
        let Meta::List(inactive) = &alternatives[0] else {
            panic!("expected negated cfg_attr condition")
        };
        assert!(inactive.path.is_ident("not"));
        assert_feature(
            &inactive.parse_args::<Meta>().expect("cfg_attr condition should parse"),
            "filtered",
        );
        assert_feature(&alternatives[1], "extra");
    }
}
