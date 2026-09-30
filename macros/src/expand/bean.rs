// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Emits callable bean functions and their generated registration markers.

use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;
use syn::Ident;
use syn::Result;
use syn::ext::IdentExt;

use crate::expand::ExpansionContext;
use crate::expand::internal_ident;
use crate::expand::value;
use crate::ir::BeanIr;
use crate::ir::BindingOptions;
use crate::ir::DependencyIr;
use crate::ir::DependencyKind;
use crate::ir::OutputShape;

/// Emits the original callable function together with the generated
/// registration marker and its `ComponentDefinition` implementation.
///
/// The [`BeanIr`] is consumed, and the returned tokens are the original item
/// followed by the marker struct, so the factory keeps its call signature while
/// registration becomes a separate `install` step. Every declared parameter is
/// re-emitted twice: once as a graph request inside `register` and once as a
/// resolved access inside the factory closure, each prefixed by the parameter's
/// `#[cfg]` conditions so conditional injection survives expansion. Parameters
/// that read configuration are additionally wrapped in the runtime's
/// configuration guard on both sides.
///
/// # Errors
///
/// Returns the [`syn::Error`] produced while the generated tokens are parsed,
/// which surfaces an inconsistent intermediate representation to the macro
/// caller instead of emitting code that cannot compile.
///
/// # Panics
///
/// Panics when a declared parameter carries no graph request, because every
/// parameter kind maps to at least one request, and when a managed bean
/// declares an `Arc` output shape, because a managed handle cannot be recovered
/// from a shared pointer. Both conditions are rejected earlier during
/// validation, so reaching them means the intermediate representation is
/// internally inconsistent.
pub(crate) fn expand(value: BeanIr, context: &ExpansionContext) -> Result<TokenStream> {
    let BeanIr {
        item,
        options,
        marker,
        params,
        output,
        source,
    } = value;
    let runtime = &context.runtime;
    let function = &item.sig.ident;
    let marker = marker.unwrap_or_else(|| default_marker(function));
    let visibility = &item.vis;
    let component_type = &output.component_type;
    let has_aliases = !options.binds.is_empty();
    let concrete_options = binding_options(&options, has_aliases, runtime);
    let alias_options = binding_options(&options, false, runtime);
    let dependencies = params
        .iter()
        .map(|param| {
            let conditions = &param.conditions;
            let request = dependency_request(&param.dependency, runtime);
            let request = require_config(&param.dependency, request, runtime);
            quote!(#(#conditions)* #request)
        })
        .collect::<Vec<_>>();
    let arguments = (0..params.len())
        .map(|index| internal_ident("argument", index))
        .collect::<Vec<_>>();
    let context = internal_ident("context", 0);
    let definition = internal_ident("definition", 0);
    let dependencies_ident = internal_ident("dependencies", 0);
    let dependency_ident = internal_ident("dependency", 0);
    let output_ident = internal_ident("output", 0);
    let accesses = params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let ident = &arguments[index];
            let access = dependency_access(&param.dependency, &param.ident, runtime, &context);
            let access = require_config(&param.dependency, access, runtime);
            let conditions = &param.conditions;
            quote!(#(#conditions)* let #ident = #access;)
        })
        .collect::<Vec<_>>();
    let invocation_arguments = params
        .iter()
        .zip(arguments.iter())
        .map(|(param, argument)| {
            let conditions = &param.conditions;
            quote!(#(#conditions)* #argument)
        })
        .collect::<Vec<_>>();
    let invocation = quote!(#function(#(#invocation_arguments),*));
    let is_async = item.sig.asyncness.is_some();
    let invocation = if is_async {
        quote!(#invocation.await)
    } else {
        invocation
    };
    let value = shaped_output(&invocation, output.shape, output.managed, runtime);
    let (output_type, factory_type, factory_method) = match (output.managed, is_async) {
        (true, true) => (
            quote!(#runtime::Managed<#component_type>),
            quote!(#runtime::ManagedFactoryFuture<#component_type>),
            quote!(managed_async_factory),
        ),
        (true, false) => (
            quote!(#runtime::Managed<#component_type>),
            quote!(::core::result::Result<#runtime::Managed<#component_type>, #runtime::FactoryError>),
            quote!(managed_factory),
        ),
        (false, true) => (
            quote!(::std::sync::Arc<#component_type>),
            quote!(#runtime::FactoryFuture<#component_type>),
            quote!(async_factory),
        ),
        (false, false) => (
            quote!(::std::sync::Arc<#component_type>),
            quote!(::core::result::Result<::std::sync::Arc<#component_type>, #runtime::FactoryError>),
            quote!(factory),
        ),
    };
    let factory = factory_closure(
        &context,
        &accesses,
        &output_ident,
        &output_type,
        &value,
        &factory_type,
        is_async,
    );
    let aliases = options.binds.iter().map(|target| {
        quote! {
            .bind::<#target, _>(#alias_options, |concrete| {
                let alias: ::std::sync::Arc<#target> = concrete;
                alias
            })
        }
    });
    let item_name = &source.item;
    let generated = quote! {
        #[doc = concat!(
            "Registration definition for the `", ::core::stringify!(#function), "` factory. ",
            "Install it with `ContainerBuilder::install::<", ::core::stringify!(#marker), ">()`; ",
            "calling the original factory function does not register it."
        )]
        #visibility struct #marker;

        impl #marker {
            const __IOC_SOURCE: #runtime::DefinitionSource = #runtime::DefinitionSource::new(
                ::core::env!("CARGO_PKG_NAME"), ::core::module_path!(), ::core::file!(),
                ::core::line!(), ::core::column!(), ::core::stringify!(#item_name),
            );
        }

        impl #runtime::ComponentDefinition for #marker {
            fn source() -> #runtime::DefinitionSource { Self::__IOC_SOURCE }

            fn register(builder: &mut #runtime::ContainerBuilder) -> ::core::result::Result<(), #runtime::RegistrationError> {
                let mut #dependencies_ident: ::std::vec::Vec<#runtime::Dependency> = ::std::vec::Vec::new();
                for #dependency_ident in [#(#dependencies),*] {
                    if !#dependencies_ident.contains(&#dependency_ident) {
                        #dependencies_ident.push(#dependency_ident);
                    }
                }
                let #definition = #runtime::Definition::<#component_type>::builder()
                    .source(Self::__IOC_SOURCE)
                    .binding(#concrete_options)
                    .dependencies(&#dependencies_ident)
                    .#factory_method(#factory)
                    #(#aliases)*
                    .build()?;
                builder.register_definition(#definition)
            }
        }
    };
    Ok(quote! { #item #generated })
}

/// Wraps a request or access expression in the config requirement guard when
/// the dependency reads a configuration value.
fn require_config(dependency: &DependencyIr, expression: TokenStream, runtime: &TokenStream) -> TokenStream {
    if matches!(dependency.kind, DependencyKind::Value { .. }) {
        quote!(#runtime::__private::require_config! { #expression })
    } else {
        expression
    }
}

/// Applies the declared output shape to one call expression, wrapping results
/// into `FactoryError` and shared handles into `Arc` where the shape asks for
/// it.
///
/// Managed outputs must be produced directly by the bean, so an `Arc` shape is
/// rejected because the managed handle cannot be recovered from a shared
/// pointer.
fn shaped_output(invocation: &TokenStream, shape: OutputShape, managed: bool, runtime: &TokenStream) -> TokenStream {
    let fallible = match shape {
        OutputShape::ResultBare | OutputShape::ResultArc => {
            quote!(#invocation.map_err(#runtime::FactoryError::new)?)
        }
        OutputShape::Bare | OutputShape::Arc => invocation.clone(),
    };
    match (managed, shape) {
        (true, OutputShape::Arc | OutputShape::ResultArc) => {
            unreachable!("managed bean cannot also be an Arc output")
        }
        (_, OutputShape::Bare | OutputShape::ResultBare) if !managed => {
            quote!(::std::sync::Arc::new(#fallible))
        }
        _ => fallible,
    }
}

/// Emits the closure passed to the definition builder, resolving every
/// declared parameter and returning the shaped bean value.
///
/// Asynchronous beans are boxed into a future so the generated closure keeps a
/// single signature shape for the builder.
fn factory_closure(
    context: &Ident,
    accesses: &[TokenStream],
    output_ident: &Ident,
    output_type: &TokenStream,
    value: &TokenStream,
    factory_type: &TokenStream,
    is_async: bool,
) -> TokenStream {
    let body = quote! {
        #(#accesses)*
        let #output_ident: #output_type = #value;
        ::core::result::Result::Ok(#output_ident)
    };
    if is_async {
        quote!(|#context| -> #factory_type {
            ::std::boxed::Box::pin(async move { #body })
        })
    } else {
        quote!(|#context| -> #factory_type { #body })
    }
}

/// Converts a function name such as `db_pool` to its `DbPoolBean` marker.
pub(super) fn default_marker(function: &Ident) -> Ident {
    let raw = function.unraw().to_string();
    let mut name = String::with_capacity(raw.len() + 4);
    let mut uppercase_next = true;
    for ch in raw.chars() {
        if ch == '_' {
            uppercase_next = true;
        } else if uppercase_next {
            name.extend(ch.to_uppercase());
            uppercase_next = false;
        } else {
            name.push(ch);
        }
    }
    name.push_str("Bean");
    format_ident!("{name}", span = function.span())
}

/// Builds runtime options, leaving priority and order on aliases when present.
fn binding_options(options: &BindingOptions, concrete_has_aliases: bool, runtime: &TokenStream) -> TokenStream {
    let id = options.id.as_ref().map_or_else(
        || quote!(::core::option::Option::None),
        |id| quote!(::core::option::Option::Some(#id.to_owned())),
    );
    let profile = options.profile.as_ref().map_or_else(
        || quote!(::core::option::Option::None),
        |profile| quote!(::core::option::Option::Some(#profile.to_owned())),
    );
    let primary = options.primary && !concrete_has_aliases;
    let order = if concrete_has_aliases { 0 } else { options.order };
    quote!(#runtime::BindingOptions {
        id: #id,
        primary: #primary,
        order: #order,
        profile: #profile,
    })
}

/// Converts one parameter IR entry into its graph request.
///
/// A `Value` dependency always maps to the shared `Config` request; the
/// generated `register` body removes duplicates before building the definition.
fn dependency_request(dependency: &DependencyIr, runtime: &TokenStream) -> TokenStream {
    let target = &dependency.requested_type;
    match &dependency.kind {
        DependencyKind::Required => match &dependency.id {
            Some(id) => quote!(#runtime::Dependency::with_id::<#target>(#id)),
            None => quote!(#runtime::Dependency::of::<#target>()),
        },
        DependencyKind::Optional => match &dependency.id {
            Some(id) => quote!(#runtime::Dependency::optional_with_id::<#target>(#id)),
            None => quote!(#runtime::Dependency::optional::<#target>()),
        },
        DependencyKind::All => quote!(#runtime::Dependency::all::<#target>()),
        DependencyKind::Value { .. } => value::config_request(runtime),
    }
}

/// Reads one declared request and wraps any impossible access failure as a
/// factory error.
fn dependency_access(
    dependency: &DependencyIr,
    parameter: &Ident,
    runtime: &TokenStream,
    context: &Ident,
) -> TokenStream {
    let target = &dependency.requested_type;
    if let DependencyKind::Value { .. } = &dependency.kind {
        return value::read_value(dependency, parameter, runtime, &quote!(#context));
    }
    let request = match &dependency.kind {
        DependencyKind::Required => match &dependency.id {
            Some(id) => quote!(#context.get_by_id::<#target>(#id)),
            None => quote!(#context.get::<#target>()),
        },
        DependencyKind::Optional => match &dependency.id {
            Some(id) => quote!(#context.try_get_by_id::<#target>(#id)),
            None => quote!(#context.try_get::<#target>()),
        },
        DependencyKind::All => quote!(#context.get_all::<#target>()),
        DependencyKind::Value { .. } => unreachable!("value dependencies were handled above"),
    };
    quote!(#request.map_err(#runtime::FactoryError::new)?)
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;
    use quote::ToTokens;
    use quote::quote;
    use syn::File;
    use syn::Ident;
    use syn::Item;
    use syn::ItemFn;
    use syn::LitStr;
    use syn::Type;
    use syn::parse_quote;
    use syn::parse2;

    use crate::expand::ExpansionContext;
    use crate::ir::BeanIr;
    use crate::ir::BindingOptions;
    use crate::ir::DependencyIr;
    use crate::ir::DependencyKind;
    use crate::ir::OutputIr;
    use crate::ir::OutputShape;
    use crate::ir::ParamIr;
    use crate::ir::SourceIr;

    /// Expands a callable bean with the selected marker through the real
    /// expander.
    fn expand_bean(marker: Option<Ident>) -> File {
        expand_bean_with(marker, OutputShape::Bare, false, false, Vec::new())
    }

    /// Expands a bean with the selected output strategy and parameters.
    fn expand_bean_with(
        marker: Option<Ident>,
        shape: OutputShape,
        managed: bool,
        is_async: bool,
        params: Vec<ParamIr>,
    ) -> File {
        let item: ItemFn = if is_async {
            parse_quote! {
                pub async fn foo_bar() -> u8 { 1 }
            }
        } else {
            parse_quote! {
                pub fn foo_bar() -> u8 { 1 }
            }
        };
        let source = SourceIr {
            item: item.sig.ident.clone(),
            span: Span::call_site(),
        };
        let value = BeanIr {
            item,
            options: BindingOptions {
                id: None,
                binds: Vec::new(),
                primary: false,
                order: 0,
                profile: None,
            },
            marker,
            params,
            output: OutputIr {
                shape,
                managed,
                component_type: parse_quote!(u8),
            },
            source,
        };
        let context = ExpansionContext {
            runtime: quote!(::qubit_ioc),
        };
        let tokens = super::expand(value, &context).expect("bean expansion should succeed");
        parse2(tokens).expect("generated bean should be valid Rust AST")
    }

    /// Renders one generated item so shape and builder method choices can be
    /// asserted on the exact emitted text.
    fn registration_tokens(generated: &File) -> String {
        let registration = generated
            .items
            .iter()
            .find_map(|item| match item {
                Item::Impl(implementation) if implementation.trait_.is_some() => Some(implementation),
                _ => None,
            })
            .expect("expected registration trait impl");
        registration.to_token_stream().to_string()
    }

    /// Checks both the marker definition and the target of its registration
    /// impl.
    fn assert_marker(generated: &File, expected: &str) {
        let Item::Fn(function) = &generated.items[0] else {
            panic!("expected original bean function")
        };
        assert_eq!(function.sig.ident, "foo_bar");
        let markers = generated
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Struct(marker) => Some(marker),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(markers.len(), 1, "exactly one registration marker must be emitted");
        assert_eq!(markers[0].ident, expected);
        let registration = generated
            .items
            .iter()
            .find_map(|item| match item {
                Item::Impl(implementation) if implementation.trait_.is_some() => Some(implementation),
                _ => None,
            })
            .expect("expected registration trait impl");
        let Type::Path(target) = registration.self_ty.as_ref() else {
            panic!("expected marker impl target")
        };
        assert!(target.path.is_ident(expected));
    }

    /// Asserts that the registration body contains the expected fragment,
    /// reporting the whole rendering when the shape strategy changes.
    fn assert_registration_contains(generated: &File, expected: &str) {
        let rendering = registration_tokens(generated);
        assert!(
            rendering.contains(expected),
            "expected registration to contain `{expected}`, rendered: {rendering}"
        );
    }

    /// Asserts that the registration body does not contain the fragment.
    fn assert_registration_lacks(generated: &File, unexpected: &str) {
        let rendering = registration_tokens(generated);
        assert!(
            !rendering.contains(unexpected),
            "expected registration to avoid `{unexpected}`, rendered: {rendering}"
        );
    }

    /// Builds one required dependency for the given requested type.
    fn required_dependency(requested_type: Type) -> DependencyIr {
        DependencyIr {
            kind: DependencyKind::Required,
            requested_type,
            id: None,
        }
    }

    /// Builds one configuration-reading dependency for the given path.
    fn value_dependency(requested_type: Type, path: &str) -> DependencyIr {
        DependencyIr {
            kind: DependencyKind::Value {
                path: LitStr::new(path, Span::call_site()),
            },
            requested_type,
            id: None,
        }
    }

    /// Builds one unconditional parameter for the given dependency.
    fn parameter(name: &str, dependency: DependencyIr) -> ParamIr {
        ParamIr {
            ident: parse_str_ident(name),
            conditions: Vec::new(),
            dependency,
        }
    }

    /// Parses a plain parameter name.
    fn parse_str_ident(name: &str) -> Ident {
        Ident::new(name, Span::call_site())
    }

    #[test]
    fn test_bean_emits_default_pascal_case_marker() {
        assert_marker(&expand_bean(None), "FooBarBean");
    }

    #[test]
    fn test_bean_emits_marker_override() {
        assert_marker(&expand_bean(Some(parse_quote!(CustomFactory))), "CustomFactory");
    }

    #[test]
    fn test_bean_wraps_bare_output_in_arc_and_uses_sync_factory() {
        let generated = expand_bean_with(None, OutputShape::Bare, false, false, Vec::new());
        assert_registration_contains(&generated, ":: std :: sync :: Arc :: new (foo_bar ())");
        assert_registration_contains(&generated, ". factory (");
        assert_registration_lacks(&generated, "FactoryError :: new");
    }

    #[test]
    fn test_bean_passes_arc_output_through_without_wrapping() {
        let generated = expand_bean_with(None, OutputShape::Arc, false, false, Vec::new());
        assert_registration_contains(
            &generated,
            "let __qubit_ioc_output_0 : :: std :: sync :: Arc < u8 > = foo_bar () ;",
        );
        assert_registration_lacks(&generated, "Arc :: new");
    }

    #[test]
    fn test_bean_maps_fallible_bare_output_error_before_wrapping() {
        let generated = expand_bean_with(None, OutputShape::ResultBare, false, false, Vec::new());
        assert_registration_contains(
            &generated,
            ":: std :: sync :: Arc :: new (foo_bar () . map_err (:: qubit_ioc :: FactoryError :: new) ?)",
        );
    }

    #[test]
    fn test_bean_maps_fallible_arc_output_error_without_wrapping() {
        let generated = expand_bean_with(None, OutputShape::ResultArc, false, false, Vec::new());
        assert_registration_contains(
            &generated,
            "let __qubit_ioc_output_0 : :: std :: sync :: Arc < u8 > = foo_bar () . map_err (:: qubit_ioc :: FactoryError :: new) ? ;",
        );
        assert_registration_lacks(&generated, "Arc :: new");
    }

    #[test]
    fn test_bean_boxes_async_bare_output_in_a_factory_future() {
        let generated = expand_bean_with(None, OutputShape::Bare, false, true, Vec::new());
        assert_registration_contains(&generated, ". async_factory (");
        assert_registration_contains(&generated, "Box :: pin (async move");
        assert_registration_contains(&generated, "-> :: qubit_ioc :: FactoryFuture < u8 >");
    }

    #[test]
    fn test_bean_emits_managed_closure_without_arc_wrapping() {
        let generated = expand_bean_with(None, OutputShape::Bare, true, false, Vec::new());
        assert_registration_contains(&generated, ". managed_factory (");
        assert_registration_contains(
            &generated,
            "let __qubit_ioc_output_0 : :: qubit_ioc :: Managed < u8 > = foo_bar () ;",
        );
        assert_registration_lacks(&generated, "Arc :: new");
    }

    #[test]
    fn test_bean_emits_managed_async_factory_future() {
        let generated = expand_bean_with(None, OutputShape::Bare, true, true, Vec::new());
        assert_registration_contains(&generated, ". managed_async_factory (");
        assert_registration_contains(&generated, "-> :: qubit_ioc :: ManagedFactoryFuture < u8 >");
    }

    #[test]
    fn test_bean_maps_managed_fallible_output_error_without_arc() {
        let generated = expand_bean_with(None, OutputShape::ResultBare, true, false, Vec::new());
        assert_registration_contains(
            &generated,
            ":: qubit_ioc :: Managed < u8 > = foo_bar () . map_err (:: qubit_ioc :: FactoryError :: new) ? ;",
        );
    }

    #[test]
    #[should_panic(expected = "managed bean cannot also be an Arc output")]
    fn test_bean_rejects_managed_arc_output() {
        let _ = expand_bean_with(None, OutputShape::Arc, true, false, Vec::new());
    }

    #[test]
    fn test_bean_declares_graph_request_and_context_access_for_each_parameter() {
        let parameter = parameter("dep", required_dependency(parse_quote!(u16)));
        let generated = expand_bean_with(None, OutputShape::Bare, false, false, vec![parameter]);
        assert_registration_contains(&generated, ":: qubit_ioc :: Dependency :: of :: < u16 > ()");
        assert_registration_contains(
            &generated,
            "__qubit_ioc_context_0 . get :: < u16 > () . map_err (:: qubit_ioc :: FactoryError :: new) ?",
        );
        assert_registration_contains(&generated, "foo_bar (__qubit_ioc_argument_0)");
    }

    #[test]
    fn test_bean_wraps_config_value_dependencies_in_the_config_guard() {
        let parameter = parameter("value", value_dependency(parse_quote!(u32), "app.port"));
        let generated = expand_bean_with(None, OutputShape::Bare, false, false, vec![parameter]);
        let rendering = registration_tokens(&generated);
        let guard = ":: qubit_ioc :: __private :: require_config !";
        let guarded = rendering.matches(guard).count();
        assert_eq!(
            guarded, 2,
            "config dependency must be guarded on both request and access: {rendering}"
        );
        assert_registration_contains(&generated, ":: qubit_ioc :: config :: get_value_for :: < u32 >");
    }
}
