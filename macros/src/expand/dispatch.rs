// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Dispatches validated declarations to their expansion implementation.

use proc_macro2::TokenStream;
use syn::Result;

use crate::expand::ExpansionContext;
use crate::internal::RuntimePath;
use crate::ir::Declaration;

/// Selects an expander only after parsing and validation have succeeded.
///
/// Dispatch is a pure routing step: it builds the shared expansion context and
/// then hands the already validated declaration to the expander that owns its
/// kind. It adds no generation logic of its own, allocates nothing beyond that
/// context, and never panics.
///
/// # Parameters
///
/// * `declaration` — The validated declaration to expand. Its variant selects
///   the expander: the `Component`, `Configuration`, and
///   `ConfigurationProperties` payloads are moved into their expander, while
///   the `Bean` payload is a `Box` that is dereferenced before it is passed on.
/// * `runtime` — Resolved path to the consuming `qubit_ioc` crate, used to
///   build the [`ExpansionContext`] the expander emits absolute paths against.
///
/// # Returns
///
/// The token stream produced by the selected expander, or the first error the
/// context construction reports. Errors raised by the expander itself are
/// propagated unchanged, so a caller sees a single flattened `syn::Error`
/// regardless of which stage rejected the input.
///
/// # Errors
///
/// Returns a [`syn::Error`] when [`ExpansionContext::for_runtime`] cannot build
/// the context, which happens when the resolved runtime path cannot be turned
/// into the identifiers the expansion quotes. The current implementation of
/// `for_runtime` has no failing branch, so this error is not produced today;
/// the `Result` signature is kept so that a future runtime path validation
/// reports through the same channel instead of adding a second error path here.
pub(crate) fn dispatch(declaration: Declaration, runtime: &RuntimePath) -> Result<TokenStream> {
    let context = ExpansionContext::for_runtime(runtime)?;
    match declaration {
        Declaration::Component(value) => super::component::expand(value, &context),
        Declaration::Bean(value) => super::bean::expand(*value, &context),
        Declaration::Configuration(value) => super::configuration::expand(value, &context),
        Declaration::ConfigurationProperties(value) => super::config_properties::expand(value, &context),
    }
}
