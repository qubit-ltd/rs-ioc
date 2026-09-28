// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Dispatches validated declarations to independent expansion domains.

use proc_macro_crate::FoundCrate;
use proc_macro_crate::crate_name;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::quote;
use syn::Error;
use syn::Ident;
use syn::Result;

use crate::ir::Declaration;

/// Context shared by expansion domains without assuming the runtime crate
/// alias.
pub(crate) struct ExpansionContext {
    /// Absolute or crate-local path to the runtime crate in the consumer.
    #[allow(dead_code)] // The implementation tasks consume this resolved path.
    pub(crate) runtime: TokenStream,
}

impl ExpansionContext {
    /// Resolves the runtime path as named in the consuming crate's manifest.
    pub(crate) fn for_runtime() -> Result<Self> {
        let runtime = match crate_name("qubit-ioc") {
            Ok(FoundCrate::Itself) => quote!(::qubit_ioc),
            Ok(FoundCrate::Name(name)) => {
                let ident = Ident::new_raw(&name, Span::call_site());
                quote!(::#ident)
            }
            Err(error) => {
                return Err(Error::new(
                    Span::call_site(),
                    format!("cannot locate `qubit-ioc` runtime dependency: {error}"),
                ));
            }
        };
        Ok(Self { runtime })
    }
}

/// Selects an expander only after parsing and validation have succeeded.
pub(crate) fn dispatch(declaration: Declaration) -> Result<TokenStream> {
    let context = ExpansionContext::for_runtime()?;
    match declaration {
        Declaration::Component(value) => component::expand(value, &context),
        Declaration::Bean(value) => bean::expand(*value, &context),
        Declaration::Configuration(value) => configuration::expand(value, &context),
        Declaration::ConfigurationProperties(value) => config_properties::expand(value, &context),
    }
}

mod component;

mod bean;
mod config_properties;
mod configuration;
mod symbols;
mod value;
