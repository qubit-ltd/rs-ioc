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

use self::internal::ValidatedOptions;
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

/// Validates a parsed declaration and normalizes it for its later expander.
///
/// This is the second stage of the macro pipeline and the entry point of the
/// `validate` domain. It normalizes the attribute options once, then routes the
/// declaration to the submodule that owns its kind: the local semantic checks
/// themselves live in `bean`, `component`, `configuration`,
/// `configuration_properties` and `options`. The call runs entirely inside the
/// proc-macro process, reads no files, and keeps no state across invocations.
///
/// # Parameters
///
/// `raw` is the declaration produced by `crate::parse`; it is consumed here
/// because routing moves its item and option list into the owning submodule.
/// `runtime` is the resolved runtime crate identity; of the four routes only
/// the bean route uses it, to recognise `Managed<T>` factory return types, and
/// it is borrowed rather than moved.
///
/// # Returns
///
/// `Ok` with a [`Declaration`] whose variant records which route ran:
/// `Component` for the component, service and repository kinds;
/// `Bean` for a function factory; `Configuration` for an inline module; and
/// `ConfigurationProperties` for a prefix declaration struct. The value is
/// exactly what `crate::expand::dispatch` consumes next, so validation never
/// emits tokens itself.
///
/// # Errors
///
/// Returns the first error of the pipeline. Option problems surface from
/// `options::validate_options` before routing starts, so an unknown key,
/// duplicate key, missing required option or out-of-range value is reported
/// without any item being inspected. Otherwise a mismatch between the attribute
/// kind and the annotated item, such as `#[bean]` on a struct, returns a
/// [`syn::Error`] anchored at the item span whose message names the attribute,
/// and any semantic rejection from the owning submodule is returned unchanged.
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
