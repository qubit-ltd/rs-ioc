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
use crate::ir::Declaration;
use crate::runtime_path::RuntimePath;

/// Selects an expander only after parsing and validation have succeeded.
pub(crate) fn dispatch(declaration: Declaration, runtime: &RuntimePath) -> Result<TokenStream> {
    let context = ExpansionContext::for_runtime(runtime)?;
    match declaration {
        Declaration::Component(value) => super::component::expand(value, &context),
        Declaration::Bean(value) => super::bean::expand(*value, &context),
        Declaration::Configuration(value) => super::configuration::expand(value, &context),
        Declaration::ConfigurationProperties(value) => super::config_properties::expand(value, &context),
    }
}
