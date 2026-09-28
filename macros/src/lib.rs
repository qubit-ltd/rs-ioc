// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Attribute macros that turn declarations into explicit `qubit-ioc`
//! registrations.
//!
//! Enable the runtime crate's `macros` feature to use these attributes.
//! Generated definitions are staged with `ContainerBuilder::install` or a
//! generated `Configuration::register_ioc` function; registering a definition
//! does not construct its component.
//!
//! # Supported declarations
//!
//! | Attribute | Accepted item | Declaration options |
//! | --- | --- | --- |
//! | `Component`, `Service`, `Repository` | Named-field or unit struct | `id`, repeated `bind = dyn Trait`, `primary`, `order`, `profile` |
//! | `bean` | Safe, non-generic free function with named parameters and an owned return type | `id`, repeated `bind`, `primary`, `order`, `profile`, `type`, `marker` |
//! | `Configuration` | Inline module containing direct bean functions | `profile` |
//! | `ConfigurationProperties` | Named-field struct | required `prefix`, optional `id`, `primary`, `order`, `profile` |
//!
//! Struct injection accepts `Arc<T>`, `Option<Arc<T>>`, and `Vec<Arc<T>>`;
//! `#[inject]` and `#[inject(id = "storage.primary")]` refine selection.
//! With both `macros` and `config` features enabled, `#[value("path.key")]`
//! reads one value and `ConfigurationProperties` deserializes one subtree.
//!
//! `bean` supports synchronous or `async fn` factories returning `T`, `Arc<T>`,
//! or `Result<T, E>` / `Result<Arc<T>, E>`; managed functions return
//! `Managed<T>` (or `Result<Managed<T>, E>`). Result handling is recognized for
//! the Rust `Result` spelling and its standard `std::result::Result` or
//! `core::result::Result` paths. A custom alias wrapping Result cannot be
//! identified from its spelling; use a supported explicit Result spelling or
//! a handwritten factory registration.
//!
//! A bean keeps its original function callable and generates a public marker
//! with the same visibility. The default marker is the function name in Pascal
//! case followed by `Bean`; `marker = Name` selects another name. Install one
//! with `builder.install::<NameBean>()?`. `Configuration` applies its profile
//! only to direct children without a profile override and generates one
//! `register_ioc` function in source order.
//!
//! # Example
//!
//! ```text
//! #[qubit_ioc::Configuration(profile = "worker")]
//! mod wiring {
//!     #[qubit_ioc::bean(marker = LabelDefinition)]
//!     pub fn label() -> String { String::from("worker") }
//! }
//! wiring::register_ioc(&mut builder)?;
//! ```
//!
//! This snippet assumes `macros` is enabled and `builder` is a
//! `qubit_ioc::ContainerBuilder`. Attributes reject unsupported item kinds,
//! option names, signatures, injection shapes, duplicate/conflicting helpers,
//! and IDs that do not match the runtime segmented ASCII grammar.

use proc_macro::TokenStream;

mod conditions;
mod entrypoint;
mod expand;
// Later expansion tasks consume the complete IR; parser tests exercise it
// already.
#[allow(dead_code)]
mod ir;
mod parse;
mod validate;

#[cfg(test)]
mod parse_tests;

pub(crate) use entrypoint::expand_entry;
use ir::MacroKind;

/// Generates a component definition for a named-field or unit struct.
///
/// Accepted options are `id`, repeated `bind = dyn Trait`, `primary`, `order`,
/// and `profile`. Fields use `Arc<T>`, `Option<Arc<T>>`, or `Vec<Arc<T>>`;
/// `#[inject(id = "...")]` selects one exact binding. `#[value("...")]`
/// requires both runtime features `macros` and `config`.
///
/// # Parameters
///
/// `attribute` contains the declaration options; `item` contains the struct.
///
/// # Returns
///
/// The expanded struct and its generated component definition, or compiler
/// diagnostics for an invalid declaration.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn Component(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::Component, attribute, item)
}

/// Generates a struct-backed service definition using the `Component` field
/// injection rules.
///
/// # Parameters
///
/// `attribute` contains component options; `item` contains the struct.
///
/// # Returns
///
/// The expanded struct and its generated service definition, or compiler
/// diagnostics for an invalid declaration.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn Service(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::Service, attribute, item)
}

/// Generates a struct-backed repository definition using the `Component`
/// field injection rules.
///
/// # Parameters
///
/// `attribute` contains component options; `item` contains the struct.
///
/// # Returns
///
/// The expanded struct and its generated repository definition, or compiler
/// diagnostics for an invalid declaration.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn Repository(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::Repository, attribute, item)
}

/// Generates `register_ioc` for direct bean functions in an inline module.
/// Its optional `profile` becomes the default for child beans without their
/// own profile. The function installs definitions in source order; building
/// the container remains the application's responsibility.
///
/// # Parameters
///
/// `attribute` contains the optional profile; `item` contains the inline
/// module.
///
/// # Returns
///
/// The expanded module with a generated `register_ioc` function, or compiler
/// diagnostics for an invalid declaration.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn Configuration(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::Configuration, attribute, item)
}

/// Generates a config-backed component for a named-field struct that can be
/// deserialized by Serde. `prefix` is required; the runtime `config` feature
/// must be enabled along with `macros`.
///
/// # Parameters
///
/// `attribute` contains the required prefix and optional component settings;
/// `item` contains the struct.
///
/// # Returns
///
/// The expanded struct and its generated config-backed definition, or compiler
/// diagnostics for an invalid declaration.
#[allow(non_snake_case)]
#[proc_macro_attribute]
pub fn ConfigurationProperties(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::ConfigurationProperties, attribute, item)
}

/// Generates a callable factory marker and registration definition for a safe,
/// non-generic free function. Parameters use the supported injection shapes;
/// return forms and marker visibility are described in the crate-level docs.
///
/// # Parameters
///
/// `attribute` contains factory options; `item` contains the free function.
///
/// # Returns
///
/// The callable function and generated marker definition, or compiler
/// diagnostics for an invalid declaration.
#[proc_macro_attribute]
pub fn bean(attribute: TokenStream, item: TokenStream) -> TokenStream {
    expand_entry(MacroKind::Bean, attribute, item)
}
