// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Configuration-backed properties declaration validation.

use syn::Error;
use syn::Fields;
use syn::ItemStruct;
use syn::Result;
use syn::spanned::Spanned;

use super::ValidatedOptions;
use super::component::validate_struct;
use crate::ir::ConfigurationPropertiesIr;
use crate::ir::MacroKind;
use crate::ir::SourceIr;
use crate::parse::missing_option;

/// Normalizes a deserializable configuration struct with a required prefix.
///
/// Shape checking is shared with `#[component]` through
/// [`validate_struct`], which rejects generics, `where` clauses, and tuple
/// structs; this function adds the stricter rule that the struct must have
/// named fields, so a unit struct is rejected here as well. The declared field
/// types are not inspected — deserializability is checked when the generated
/// code is compiled.
///
/// `prefix` is mandatory. It is taken out of `options`, which is why the
/// remaining binding recorded in the result no longer carries it.
///
/// # Parameters
///
/// * `item` – the struct being annotated, taken by value and stored unchanged
///   in the result.
/// * `options` – already validated macro options; the `prefix` entry is
///   consumed from it.
///
/// # Errors
///
/// Returns an error when the struct has generic parameters, a `where` clause,
/// or unnamed fields, and when the `prefix` option is missing.
pub(super) fn configuration_properties(
    item: ItemStruct,
    mut options: ValidatedOptions,
) -> Result<ConfigurationPropertiesIr> {
    validate_struct(&item, MacroKind::ConfigurationProperties)?;
    if !matches!(item.fields, Fields::Named(_)) {
        return Err(Error::new(
            item.fields.span(),
            "#[ConfigurationProperties] requires named fields",
        ));
    }
    let prefix = options
        .prefix
        .take()
        .ok_or_else(|| missing_option(item.span(), "prefix"))?;
    let source = SourceIr {
        item: item.ident.clone(),
        span: item.ident.span(),
    };
    Ok(ConfigurationPropertiesIr {
        item,
        prefix,
        options: options.binding(),
        source,
    })
}
