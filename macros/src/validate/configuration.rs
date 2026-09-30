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
///
/// The module must carry its body inline, so a file-style `mod name;` is
/// rejected. Only a direct child free function named `register_ioc` counts as
/// a conflict; every other child item is carried through untouched. The
/// resulting IR keeps the whole module, so the expansion stage can emit the
/// user's own items alongside the generated registration.
///
/// # Parameters
///
/// * `item` – the module being annotated, taken by value and stored unchanged
///   in the result.
/// * `options` – already validated macro options; only the profile is read, and
///   its value was validated upstream by `ValidatedOptions`.
///
/// # Errors
///
/// Returns an error when `item` has no inline body, or when it already declares
/// a `register_ioc` function that the generated code would collide with.
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
