// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Inline configuration module validation.

use syn::Error;
use syn::ItemMod;
use syn::Result;
use syn::spanned::Spanned;

use super::ValidatedOptions;
use crate::ir::ConfigurationIr;

/// Validates that a configuration module has an inline body.
///
/// The module must carry its body inline, so a file-style `mod name;` is
/// rejected. Child items are carried through untouched. Name conflicts with
/// generated items are left to Rust, which can account for conditional
/// compilation before deciding whether a conflict exists. The resulting IR
/// keeps the whole module, so the expansion stage can emit the user's own
/// items alongside the generated registration.
///
/// # Parameters
///
/// * `item` – the module being annotated, taken by value and stored unchanged
///   in the result.
/// * `options` – already validated macro options; only the profile is read, and
///   its value was validated upstream by `ValidatedOptions`.
///
/// # Returns
///
/// Returns a [`ConfigurationIr`] containing the unchanged module, its default
/// profile.
///
/// # Errors
///
/// Returns an error when `item` has no inline body.
pub(super) fn configuration(item: ItemMod, options: ValidatedOptions) -> Result<ConfigurationIr> {
    let Some((_, _)) = &item.content else {
        return Err(Error::new(
            item.span(),
            "#[Configuration] requires an inline module",
        ));
    };
    Ok(ConfigurationIr {
        item,
        profile: options.profile,
    })
}
