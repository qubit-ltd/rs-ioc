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
///
/// The set of accepted keys depends on the macro, and this function is the
/// single authority for that table: `#[component]`, `#[service]`, and
/// `#[repository]` accept `id`, `bind`, `primary`, `order`, and `profile`;
/// `#[bean]` additionally accepts `type` and `marker`; `#[configuration]`
/// accepts only `profile`; and `#[configuration_properties]` accepts
/// `prefix` in addition to the first five. Any other key is rejected.
///
/// Each key also constrains its value: `id`, `profile`, and `prefix` take a
/// string literal, `bind` takes a `dyn Trait` projection and is the only key
/// that may repeat, `primary` is a bare marker, `order` takes an integer that
/// must fit in 32 bits, `type` takes any type, and `marker` takes an
/// identifier. An unknown key and a known key with a wrong value produce
/// different diagnostics so the caller can tell a typo from a bad value.
///
/// Options are processed in declaration order and the result is the
/// declaration's option set with only the keys that were actually written.
///
/// # Parameters
///
/// * `kind` – macro that owns the declaration; it selects the accepted key set.
/// * `options` – the parsed key/value pairs, consumed in declaration order.
///
/// # Returns
///
/// The accumulated option set, or the first error for an unknown key, a
/// duplicated non-repeatable key, a malformed `id`, a `bind` that is not a
/// trait object, an `order` outside the 32-bit range, or a value whose shape
/// does not match its key.
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
///
/// The identifier must be non-empty and split on `.` into segments that each
/// match `[A-Za-z][A-Za-z0-9_]*`: the first byte of a segment is an ASCII
/// letter and every later byte is an ASCII alphanumeric or `_`. The check is
/// pure ASCII, so letters outside ASCII are rejected. The number of segments
/// is not bounded here; keeping the two implementations in step is what makes
/// a macro-time identifier interchangeable with a runtime one.
///
/// # Parameters
///
/// * `value` – the literal to check; the diagnostic points at the literal.
///
/// # Returns
///
/// `Ok(())` when the text satisfies the grammar, otherwise an error describing
/// the required pattern.
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
