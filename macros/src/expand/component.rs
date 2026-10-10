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
use crate::expand::internal_ident;
use crate::expand::value;
use crate::ir::BindingOptions;
use crate::ir::ComponentIr;
use crate::ir::DependencyIr;
use crate::ir::DependencyKind;
use crate::ir::FieldIr;

/// Emits the original struct, a typed factory and its linked registration
/// entry.
///
/// The [`ComponentIr`] is destructured, so the caller keeps no handle on it
/// afterwards, and the returned tokens are the original struct followed by the
/// `ComponentDefinition` implementation. Each accepted non-configuration field
/// contributes a dependency request inside `register`, while configuration
/// fields use a guarded configuration request; every field contributes one
/// resolved value inside the factory closure, prefixed by its activation
/// conditions. A field that reads configuration is wrapped in the runtime's
/// configuration guard on both sides. A field the validator already rejected
/// is re-emitted as its compile error with a diverging initializer, so the rest
/// of the struct still type-checks while the macro reports the original
/// diagnostic.
///
/// # Errors
///
/// Returns the span-aware [`syn::Error`] produced when an activation attribute
/// cannot be normalized, so the caller points at the offending attribute
/// instead of emitting a definition that silently lost its `cfg` conditions.
///
/// # Panics
///
/// Never panics for validated input. Fields whose kind the validator already
/// rejected are emitted as compile errors before any request mapping runs, so
/// the `unreachable!` in the request and access mappers is only reachable when
/// the intermediate representation is internally inconsistent.
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
    let dependencies_ident = internal_ident("dependencies", 0);
    let request_ident = internal_ident("request", 0);
    let context_ident = internal_ident("context", 0);
    let definition_ident = internal_ident("definition", 0);
    let source_expr = quote_spanned! {source.span=>
        #runtime::DefinitionSource::new(
            ::core::env!("CARGO_PKG_NAME"), ::core::module_path!(), ::core::file!(),
            ::core::line!(), ::core::column!(), ::core::stringify!(#item_name),
        )
    };
    let (dependencies, initializers, validation_errors) =
        field_bindings(&fields, runtime, &request_ident, &dependencies_ident, &context_ident);

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
    let aliases = alias_bindings(&options, runtime);

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
                let mut #dependencies_ident = ::std::vec::Vec::<#runtime::Dependency>::new();
                #(#dependencies)*
                let #definition_ident = #runtime::Definition::<#ident>::builder()
                    .source(<#ident as #runtime::ComponentDefinition>::source())
                    .binding(#concrete_options)
                    .dependencies(&#dependencies_ident)
                    .factory(|#context_ident| {
                        ::std::result::Result::Ok(::std::sync::Arc::new(#construct))
                    })
                    #(#aliases)*
                    .build()?;
                builder.register_definition(#definition_ident)
            }
        }

    };
    Ok(quote! { #item #generated })
}

/// Maps every declared field onto its registration request, struct
/// initializer and validation error, in declaration order.
///
/// The returned vectors are the three parallel halves the generated
/// `ComponentDefinition` impl splices together: `dependencies` goes inside
/// `register`, `initializers` builds the struct literal inside the factory
/// closure, and `validation_errors` is emitted ahead of the impl. A field the
/// validator already rejected contributes only a compile error plus a diverging
/// initializer, so it never reaches request or access mapping and the caller's
/// `#[allow(unreachable_code)]` decision depends solely on whether this list is
/// non-empty.
#[must_use]
fn field_bindings(
    fields: &[FieldIr],
    runtime: &TokenStream,
    request_ident: &Ident,
    dependencies_ident: &Ident,
    context_ident: &Ident,
) -> (Vec<TokenStream>, Vec<TokenStream>, Vec<TokenStream>) {
    let mut dependencies = Vec::with_capacity(fields.len());
    let mut initializers = Vec::with_capacity(fields.len());
    let mut validation_errors = Vec::new();
    for field in fields {
        let field_ident = &field.ident;
        let conditions = &field.conditions;
        if let Some(error) = &field.validation_error {
            let error = error.to_compile_error();
            validation_errors.push(quote!(#(#conditions)* #error));
            initializers.push(quote!(#(#conditions)* #field_ident: loop {}));
            continue;
        }
        let dependency = &field.dependency;
        let request = request_tokens(dependency, runtime);
        dependencies.push(quote! {
            #(#conditions)* {
                let #request_ident = #request;
                if !#dependencies_ident.contains(&#request_ident) {
                    #dependencies_ident.push(#request_ident);
                }
            }
        });
        let expression = field_expression(dependency, field_ident, runtime, context_ident);
        let expression = config_guard(dependency, expression, runtime);
        initializers.push(quote!(#(#conditions)* #field_ident: #expression));
    }
    (dependencies, initializers, validation_errors)
}

/// Emits one `.bind` builder step per declared interface alias.
///
/// The alias options are identical for every bind, so they are built once
/// outside the loop; each step only differs by the aliased type parameter.
#[must_use]
fn alias_bindings(options: &BindingOptions, runtime: &TokenStream) -> Vec<TokenStream> {
    let alias_options = options_tokens(options, runtime, true);
    options
        .binds
        .iter()
        .map(|bind| {
            quote! {
                .bind::<#bind, _>(#alias_options, |concrete| {
                    let alias: ::std::sync::Arc<#bind> = concrete;
                    alias
                })
            }
        })
        .collect()
}

/// Emits the graph request for one field, guarding configuration-backed fields.
///
/// A [`DependencyKind::Value`] field never reaches [`dependency_tokens`], which
/// rejects that kind, so it is delegated to
/// [`crate::expand::value::config_request`] and wrapped in the runtime's
/// configuration guard. Every other kind takes the plain graph declaration
/// and needs no guard.
#[must_use]
fn request_tokens(dependency: &DependencyIr, runtime: &TokenStream) -> TokenStream {
    if matches!(dependency.kind, DependencyKind::Value { .. }) {
        let request = value::config_request(runtime);
        quote!(#runtime::__private::require_config! { #request })
    } else {
        dependency_tokens(dependency, runtime)
    }
}

/// Wraps a resolved field expression in the configuration guard when needed.
///
/// The expression is produced by [`field_expression`], which already routes a
/// [`DependencyKind::Value`] field to [`crate::expand::value::read_value`], so
/// building it eagerly stays correct for every kind and only the guard decision
/// depends on the dependency kind.
#[must_use]
fn config_guard(dependency: &DependencyIr, expression: TokenStream, runtime: &TokenStream) -> TokenStream {
    if matches!(dependency.kind, DependencyKind::Value { .. }) {
        quote!(#runtime::__private::require_config! { #expression })
    } else {
        expression
    }
}

/// Converts one normalized dependency into its graph declaration.
///
/// A [`DependencyKind::Required`] field is mandatory, an
/// [`DependencyKind::Optional`] field is resolved leniently, and a
/// [`DependencyKind::All`] field ignores any declared id because the runtime
/// collects every binding of the type. A [`DependencyKind::Value`] field is
/// rejected, because configuration reads are routed through
/// [`request_tokens`] instead of a graph edge.
///
/// # Panics
///
/// Panics on a [`DependencyKind::Value`] dependency. Callers must route that
/// kind through [`request_tokens`] first, which only happens for a field the
/// validator accepted.
#[must_use]
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
///
/// A [`DependencyKind::Value`] field is delegated to
/// [`crate::expand::value::read_value`], which emits a configuration lookup
/// instead of a context resolution. Every other kind selects the matching
/// accessor for its identifier form and propagates a lookup failure as a
/// [`crate::ir`] `FactoryError` through the enclosing factory closure.
#[must_use]
fn field_expression(
    dependency: &DependencyIr,
    field_ident: &Ident,
    runtime: &TokenStream,
    context: &Ident,
) -> TokenStream {
    let requested_type = &dependency.requested_type;
    let context = quote!(#context);
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
///
/// `selection` is false for the concrete binding, which never carries
/// interface projection ordering, and true for every generated alias so that
/// `primary` and `order` reach the runtime binding options.
#[must_use]
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
///
/// The literal is cloned into the generated code, so an absent value stays a
/// plain `None` instead of an empty string that the runtime would treat as a
/// configured id or profile.
#[must_use]
#[inline]
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
