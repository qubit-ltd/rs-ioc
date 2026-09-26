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
use syn::ext::IdentExt;

use crate::expand::ExpansionContext;
use crate::expand::value;
use crate::ir::BeanIr;
use crate::ir::BindingOptions;
use crate::ir::DependencyIr;
use crate::ir::DependencyKind;
use crate::ir::OutputShape;

/// Emits the original function, a marker definition, and optional linked
/// discovery.
pub(crate) fn expand(value: BeanIr, context: &ExpansionContext) -> syn::Result<TokenStream> {
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
    let arguments = params.iter().map(|param| &param.ident).collect::<Vec<_>>();
    let accesses = params
        .iter()
        .map(|param| {
            let ident = &param.ident;
            let access = dependency_access(&param.dependency, ident, runtime);
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
        OutputShape::Arc => invocation,
        OutputShape::ResultBare => {
            quote!(::std::sync::Arc::new(#invocation.map_err(#runtime::FactoryError::new)?))
        }
        OutputShape::ResultArc => quote!(#invocation.map_err(#runtime::FactoryError::new)?),
    };
    let factory = if item.sig.asyncness.is_some() {
        quote!(|__qubit_context| -> #runtime::FactoryFuture<#component_type> {
            ::std::boxed::Box::pin(async move {
                #(#accesses)*
                let value: ::std::sync::Arc<#component_type> = #wrap;
                Ok(value)
            })
        })
    } else {
        quote!(|__qubit_context| -> Result<::std::sync::Arc<#component_type>, #runtime::FactoryError> {
            #(#accesses)*
            let value: ::std::sync::Arc<#component_type> = #wrap;
            Ok(value)
        })
    };
    let draft_constructor = if item.sig.asyncness.is_some() {
        quote!(new_async)
    } else {
        quote!(new_sync)
    };
    let aliases = options.binds.iter().map(|target| {
        quote! {
            draft.bind::<#target, _>(#alias_options, |concrete| {
                let alias: ::std::sync::Arc<#target> = concrete;
                alias
            })?;
        }
    });
    let item_name = &source.item;
    let generated = quote! {
        #visibility struct #marker;

        impl #marker {
            const __IOC_SOURCE: #runtime::DefinitionSource = #runtime::DefinitionSource::new(
                env!("CARGO_PKG_NAME"), module_path!(), file!(), line!(), column!(), stringify!(#item_name),
            );
            const __IOC_ID: &'static str = concat!(
                env!("CARGO_PKG_NAME"), "@", env!("CARGO_PKG_VERSION"),
                "::", module_path!(), "::", stringify!(#marker),
            );
        }

        impl #runtime::ComponentDefinition for #marker {
            fn source() -> #runtime::DefinitionSource { Self::__IOC_SOURCE }

            fn definition_id() -> &'static str { Self::__IOC_ID }

            fn register(builder: &mut #runtime::ContainerBuilder) -> Result<(), #runtime::RegistrationError> {
                let mut dependencies: ::std::vec::Vec<#runtime::Dependency> = ::std::vec::Vec::new();
                for dependency in [#(#dependencies),*] {
                    if !dependencies.contains(&dependency) {
                        dependencies.push(dependency);
                    }
                }
                let mut draft = #runtime::__private::codegen_v1::DefinitionDraft::<#component_type>::#draft_constructor(
                    Self::__IOC_SOURCE,
                    &dependencies,
                    #concrete_options,
                    #factory,
                )?;
                #(#aliases)*
                draft.register(builder)
            }
        }

        #runtime::__private::submit_component!(
            #runtime::discovery::RegistrationEntry::new(
                <#marker as #runtime::ComponentDefinition>::register,
                #marker::__IOC_SOURCE,
                #marker::__IOC_ID,
            )
        );
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
    let id = options
        .id
        .as_ref()
        .map_or_else(|| quote!(None), |id| quote!(Some(#id.to_owned())));
    let profile = options
        .profile
        .as_ref()
        .map_or_else(|| quote!(None), |profile| quote!(Some(#profile.to_owned())));
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
fn dependency_access(dependency: &DependencyIr, parameter: &Ident, runtime: &TokenStream) -> TokenStream {
    let target = &dependency.requested_type;
    if let DependencyKind::Value { .. } = &dependency.kind {
        return value::read_value(dependency, parameter, runtime, &quote!(__qubit_context));
    }
    let request = match &dependency.kind {
        DependencyKind::Required => match &dependency.id {
            Some(id) => quote!(__qubit_context.get_by_id::<#target>(#id)),
            None => quote!(__qubit_context.get::<#target>()),
        },
        DependencyKind::Optional => match &dependency.id {
            Some(id) => quote!(__qubit_context.try_get_by_id::<#target>(#id)),
            None => quote!(__qubit_context.try_get::<#target>()),
        },
        DependencyKind::All => quote!(__qubit_context.get_all::<#target>()),
        DependencyKind::Value { .. } => unreachable!("value dependencies were handled above"),
    };
    quote!(#request.map_err(#runtime::FactoryError::new)?)
}
