// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Declaration option validation and ID grammar.

use std::collections::HashSet;

use syn::Error;
use syn::LitStr;
use syn::Result;
use syn::Type;

use super::ValidatedOptions;
use crate::ir::MacroKind;
use crate::parse::RawOption;
use crate::parse::RawValue;

/// Checks option presence, duplicates, type restrictions, and ID literals.
pub(super) fn validate_options(kind: MacroKind, options: Vec<RawOption>) -> Result<ValidatedOptions> {
    let mut seen = HashSet::new();
    let mut validated = ValidatedOptions::default();
    for RawOption { key, value } in options {
        let name = key.to_string();
        let allowed = match kind {
            MacroKind::Component | MacroKind::Service | MacroKind::Repository => {
                matches!(name.as_str(), "id" | "bind" | "primary" | "order" | "profile")
            }
            MacroKind::Bean => matches!(
                name.as_str(),
                "id" | "bind" | "primary" | "order" | "profile" | "type" | "marker"
            ),
            MacroKind::Configuration => name == "profile",
            MacroKind::ConfigurationProperties => {
                matches!(name.as_str(), "prefix" | "id" | "primary" | "order" | "profile")
            }
        };
        if !allowed {
            return Err(Error::new(
                key.span(),
                format!("unknown #[{}] option `{name}`", kind.name()),
            ));
        }
        if name != "bind" && !seen.insert(name.clone()) {
            return Err(Error::new(
                key.span(),
                format!("duplicate #[{}] option `{name}`", kind.name()),
            ));
        }
        match (name.as_str(), value) {
            ("id", RawValue::String(value)) => {
                validate_id(&value)?;
                validated.id = Some(value);
            }
            ("bind", RawValue::Type(Type::TraitObject(value))) => validated.binds.push(value),
            ("bind", _) => {
                return Err(Error::new(key.span(), "`bind` requires `dyn Trait`"));
            }
            ("primary", RawValue::Flag) => validated.primary = true,
            ("order", RawValue::Integer { literal, negative }) => {
                let magnitude = literal
                    .base10_parse::<i64>()
                    .map_err(|_| Error::new(literal.span(), "`order` must fit in a 32-bit integer"))?;
                let signed = if negative { -magnitude } else { magnitude };
                validated.order = i32::try_from(signed)
                    .map_err(|_| Error::new(literal.span(), "`order` must fit in a 32-bit integer"))?;
            }
            ("profile", RawValue::String(value)) => validated.profile = Some(value),
            ("prefix", RawValue::String(value)) => validated.prefix = Some(value),
            ("type", RawValue::Type(value)) => validated.explicit_type = Some(value),
            ("marker", RawValue::Ident(value)) => validated.marker = Some(value),
            _ => return Err(Error::new(key.span(), "invalid IoC option value")),
        }
    }
    Ok(validated)
}

/// Checks the same ASCII segmented ID grammar as the runtime `BindingId`.
pub(super) fn validate_id(value: &LitStr) -> Result<()> {
    let text = value.value();
    let valid = !text.is_empty()
        && text.split('.').all(|segment| {
            let mut bytes = segment.bytes();
            bytes.next().is_some_and(|first| first.is_ascii_alphabetic())
                && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        });
    if valid {
        Ok(())
    } else {
        Err(Error::new(
            value.span(),
            "invalid `id`: each dot-separated segment must match [A-Za-z][A-Za-z0-9_]*",
        ))
    }
}
