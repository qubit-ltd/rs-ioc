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
///
/// # Parameters
///
/// `runtime` is the absolute path to the runtime crate as spelled in the
/// consuming manifest, such as `::qubit_ioc`. The path is spliced into the
/// generated tokens rather than resolved here, so no name lookup happens at
/// macro expansion time.
///
/// # Returns
///
/// A fresh token stream requesting the runtime's `Config` marker type through
/// `Dependency::of`. Every `Value` dependency of a component field or a bean
/// parameter collapses onto this single request; the generated `register` body
/// removes duplicates before building the definition.
pub(super) fn config_request(runtime: &TokenStream) -> TokenStream {
    quote!(#runtime::Dependency::of::<#runtime::__private::codegen_v1::Config>())
}

/// Reads one field or parameter with its source name for build diagnostics.
///
/// `dependency` must have a `Value` kind; this is enforced by the caller's
/// branch. The emitted expression returns a factory error for a failed read.
///
/// # Parameters
///
/// `dependency` supplies both the requested type and the configuration key
/// taken from its `Value` kind; `target` is the field or parameter identifier
/// whose spelling is reported in diagnostics; `runtime` is the absolute path to
/// the runtime crate as spelled in the consuming manifest; `context` is the
/// identifier of the generated factory context. All four are borrowed only to
/// build the token stream and are not retained.
///
/// # Returns
///
/// A fresh token stream that reads the configuration entry as
/// `config::get_value_for::<T>` against the `Config` stored in the context. A
/// missing `Config` is converted with `map_err(FactoryError::new)` and a
/// missing or mistyped entry is surfaced by the trailing `?`, so both failures
/// reach the consumer as a `FactoryError` instead of a panic in generated code.
/// The target identifier is passed through `::core::stringify!` so the error
/// message can name the offending field.
///
/// # Panics
///
/// Panics with `value code generation requires a value dependency` when
/// `dependency.kind` is not `DependencyKind::Value`. This is a macro-internal
/// invariant rather than a consumer-visible condition: every call site
/// (`macros/src/expand/bean.rs` and `macros/src/expand/component.rs`) invokes
/// this function from inside a `DependencyKind::Value` branch, so the panic can
/// only be reached by a future caller that skips that guard.
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
