// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Local semantic checks and normalization into expander-ready IoC IR.

use syn::Error;
use syn::Item;
use syn::Result;
use syn::spanned::Spanned;

use crate::internal::RuntimePath;
use crate::ir::Declaration;
use crate::ir::MacroKind;
use crate::parse::RawDeclaration;

mod bean;
mod component;
mod configuration;
mod configuration_properties;
mod internal;
mod options;

use internal::ValidatedOptions;

/// Validates a parsed declaration and normalizes it for its later expander.
pub(crate) fn validate(raw: RawDeclaration, runtime: &RuntimePath) -> Result<Declaration> {
    let kind = raw.kind;
    let options = options::validate_options(kind, raw.options)?;
    match (kind, raw.item) {
        (MacroKind::Component | MacroKind::Service | MacroKind::Repository, Item::Struct(item)) => {
            component::component(kind, item, options).map(Declaration::Component)
        }
        (MacroKind::Bean, Item::Fn(item)) => {
            bean::bean(item, options, runtime).map(|value| Declaration::Bean(Box::new(value)))
        }
        (MacroKind::Configuration, Item::Mod(item)) => {
            configuration::configuration(item, options).map(Declaration::Configuration)
        }
        (MacroKind::ConfigurationProperties, Item::Struct(item)) => {
            configuration_properties::configuration_properties(item, options).map(Declaration::ConfigurationProperties)
        }
        (_, item) => Err(Error::new(
            item.span(),
            format!("#[{}] cannot be used on this Rust item", kind.name()),
        )),
    }
}
