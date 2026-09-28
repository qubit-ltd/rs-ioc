// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared code generation for configuration-backed fields and bean parameters.

use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use crate::ir::DependencyIr;
use crate::ir::DependencyKind;

/// Declares Config as a graph dependency before a generated factory can run.
pub(super) fn config_request(runtime: &TokenStream) -> TokenStream {
    quote!(#runtime::Dependency::of::<#runtime::__private::codegen_v1::Config>())
}

/// Reads one field or parameter with its source name for build diagnostics.
///
/// `dependency` must have a `Value` kind; this is enforced by the caller's
/// branch. The emitted expression returns a factory error for a failed read.
pub(super) fn read_value(
    dependency: &DependencyIr,
    target: &Ident,
    runtime: &TokenStream,
    context: &TokenStream,
) -> TokenStream {
    let DependencyKind::Value { path } = &dependency.kind else {
        unreachable!("value code generation requires a value dependency")
    };
    let requested_type = &dependency.requested_type;
    quote! {
        #runtime::config::get_value_for::<#requested_type>(
            #context.get::<#runtime::__private::codegen_v1::Config>()
                .map_err(#runtime::FactoryError::new)?.as_ref(),
            #path,
            ::core::stringify!(#target),
        )?
    }
}
