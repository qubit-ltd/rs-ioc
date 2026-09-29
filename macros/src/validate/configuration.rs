// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Inline configuration module validation.

use syn::Error;
use syn::Item;
use syn::ItemMod;
use syn::Result;
use syn::spanned::Spanned;

use super::ValidatedOptions;
use crate::ir::ConfigurationIr;
use crate::ir::SourceIr;

/// Normalizes an inline module and rejects a conflicting generated function.
pub(super) fn configuration(item: ItemMod, options: ValidatedOptions) -> Result<ConfigurationIr> {
    let Some((_, items)) = &item.content else {
        return Err(Error::new(item.span(), "#[Configuration] requires an inline module"));
    };
    for child in items {
        if let Item::Fn(function) = child
            && function.sig.ident == "register_ioc"
        {
            return Err(Error::new(
                function.sig.ident.span(),
                "#[Configuration] conflicts with existing `register_ioc`",
            ));
        }
    }
    let source = SourceIr {
        item: item.ident.clone(),
        span: item.ident.span(),
    };
    Ok(ConfigurationIr {
        item,
        profile: options.profile,
        source,
    })
}
