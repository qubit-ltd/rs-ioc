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

/// Emits the original function, a marker definition, and optional linked
/// discovery.
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
    let dependencies = dependency_requests(
        &params.iter().map(|param| &param.dependency).collect::<Vec<_>>(),
        runtime,
    );
    let arguments = (0..params.len())
        .map(|index| internal_ident("argument", index))
        .collect::<Vec<_>>();
    let context = internal_ident("context", 0);
    let draft = internal_ident("draft", 0);
    let dependencies_ident = internal_ident("dependencies", 0);
    let dependency_ident = internal_ident("dependency", 0);
    let output_ident = internal_ident("output", 0);
    let accesses = params
        .iter()
        .enumerate()
        .map(|(index, param)| {
            let ident = &arguments[index];
            let access = dependency_access(&param.dependency, &param.ident, runtime, &context);
            quote!(let #ident = #access;)
        })
        .collect::<Vec<_>>();
    let requires_config = params
        .iter()
        .any(|param| matches!(param.dependency.kind, DependencyKind::Value { .. }));
    let invocation = quote!(#function(#(#arguments),*));
    let invocation = if item.sig.asyncness.is_some() {
        quote!(#invocation.await)
    } else {
        invocation
    };
    let wrap = match output.shape {
        OutputShape::Bare => quote!(::std::sync::Arc::new(#invocation)),
        OutputShape::Arc => invocation.clone(),
        OutputShape::ResultBare => {
            quote!(::std::sync::Arc::new(#invocation.map_err(#runtime::FactoryError::new)?))
        }
        OutputShape::ResultArc => quote!(#invocation.map_err(#runtime::FactoryError::new)?),
    };
    let factory = if output.managed {
        let managed_wrap = match output.shape {
            OutputShape::Bare => invocation.clone(),
            OutputShape::ResultBare => quote!(#invocation.map_err(#runtime::FactoryError::new)?),
            OutputShape::Arc | OutputShape::ResultArc => unreachable!("managed bean cannot also be an Arc output"),
        };
        if item.sig.asyncness.is_some() {
            quote!(|#context| -> #runtime::ManagedFactoryFuture<#component_type> {
                ::std::boxed::Box::pin(async move {
                    #(#accesses)*
                    let #output_ident: #runtime::Managed<#component_type> = #managed_wrap;
                    ::core::result::Result::Ok(#output_ident)
                })
            })
        } else {
            quote!(|#context| -> ::core::result::Result<#runtime::Managed<#component_type>, #runtime::FactoryError> {
                #(#accesses)*
                let #output_ident: #runtime::Managed<#component_type> = #managed_wrap;
                ::core::result::Result::Ok(#output_ident)
            })
        }
    } else if item.sig.asyncness.is_some() {
        quote!(|#context| -> #runtime::FactoryFuture<#component_type> {
            ::std::boxed::Box::pin(async move {
                #(#accesses)*
                let #output_ident: ::std::sync::Arc<#component_type> = #wrap;
                ::core::result::Result::Ok(#output_ident)
            })
        })
    } else {
        quote!(|#context| -> ::core::result::Result<::std::sync::Arc<#component_type>, #runtime::FactoryError> {
            #(#accesses)*
            let #output_ident: ::std::sync::Arc<#component_type> = #wrap;
            ::core::result::Result::Ok(#output_ident)
        })
    };
    let draft_constructor = if output.managed {
        if item.sig.asyncness.is_some() {
            quote!(new_managed_async)
        } else {
            quote!(new_managed_sync)
        }
    } else if item.sig.asyncness.is_some() {
        quote!(new_async)
    } else {
        quote!(new_sync)
    };
    let aliases = options.binds.iter().map(|target| {
        quote! {
            #draft.bind::<#target, _>(#alias_options, |concrete| {
                let alias: ::std::sync::Arc<#target> = concrete;
                alias
            })?;
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
                let mut #draft = #runtime::__private::codegen_v1::DefinitionDraft::<#component_type>::#draft_constructor(
                    Self::__IOC_SOURCE,
                    &#dependencies_ident,
                    #concrete_options,
                    #factory,
                )?;
                #(#aliases)*
                #draft.register(builder)
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

/// Converts parameter IR into graph requests, sharing one Config request for
/// value inputs.
fn dependency_requests(params: &[&DependencyIr], runtime: &TokenStream) -> Vec<TokenStream> {
    let mut requests = Vec::with_capacity(params.len());
    let mut config_requested = false;
    for dependency in params {
        let target = &dependency.requested_type;
        let request = match &dependency.kind {
            DependencyKind::Required => match &dependency.id {
                Some(id) => quote!(#runtime::Dependency::with_id::<#target>(#id)),
                None => quote!(#runtime::Dependency::of::<#target>()),
            },
            DependencyKind::Optional => match &dependency.id {
                Some(id) => quote!(#runtime::Dependency::optional_with_id::<#target>(#id)),
                None => quote!(#runtime::Dependency::optional::<#target>()),
            },
            DependencyKind::All => quote!(#runtime::Dependency::all::<#target>()),
            DependencyKind::Value { .. } => {
                if config_requested {
                    continue;
                }
                config_requested = true;
                value::config_request(runtime)
            }
        };
        requests.push(request);
    }
    requests
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
    use quote::quote;
    use syn::File;
    use syn::Ident;
    use syn::Item;
    use syn::ItemFn;
    use syn::Type;
    use syn::parse_quote;
    use syn::parse2;

    use crate::expand::ExpansionContext;
    use crate::ir::BeanIr;
    use crate::ir::BindingOptions;
    use crate::ir::OutputIr;
    use crate::ir::OutputShape;
    use crate::ir::SourceIr;

    /// Expands a callable bean with the selected marker through the real
    /// expander.
    fn expand_bean(marker: Option<Ident>) -> File {
        let item: ItemFn = parse_quote! {
            pub fn foo_bar() -> u8 { 1 }
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
            params: Vec::new(),
            output: OutputIr {
                shape: OutputShape::Bare,
                managed: false,
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

    #[test]
    fn test_bean_emits_default_pascal_case_marker() {
        assert_marker(&expand_bean(None), "FooBarBean");
    }

    #[test]
    fn test_bean_emits_marker_override() {
        assert_marker(&expand_bean(Some(parse_quote!(CustomFactory))), "CustomFactory");
    }
}
