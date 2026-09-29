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
