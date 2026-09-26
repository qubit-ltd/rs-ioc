// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Declarative component registration macros for `qubit-ioc`.

use proc_macro::TokenStream;

mod expand;
// Later expansion tasks consume the complete IR; parser tests exercise it
// already.
#[allow(dead_code)]
mod ir;
mod parse;
mod validate;

use ir::MacroKind;

/// Runs every declaration through parsing, validation, normalization, and
/// expansion.
fn expand_entry(kind: MacroKind, attribute: TokenStream, item: TokenStream) -> TokenStream {
    parse::parse(kind, attribute.into(), item.into())
        .and_then(validate::validate)
        .and_then(expand::dispatch)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Marks a component definition.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn Component(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::Component, attribute, item)
}

/// Marks a service definition.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn Service(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::Service, attribute, item)
}

/// Marks a repository definition.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn Repository(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::Repository, attribute, item)
}

/// Marks a configuration module.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn Configuration(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::Configuration, attribute, item)
}

/// Marks a configuration properties type.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn ConfigurationProperties(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::ConfigurationProperties, attribute, item)
}

/// Marks a component factory function.
#[proc_macro_attribute]
pub fn bean(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::Bean, attribute, item)
}
