// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Runs a declaration through parsing, validation, and code generation.

use proc_macro::TokenStream;
use syn::Error;

use crate::expand;
use crate::internal::RuntimePath;
use crate::ir::MacroKind;
use crate::parse;
use crate::validate;

/// Runs one declaration macro invocation end to end.
///
/// The invocation is first parsed into a raw declaration, then the crate
/// runtime path is resolved, then the raw declaration is validated against
/// that path, and finally the validated declaration is expanded into generated
/// code. Every stage runs in the proc-macro process, performs no I/O, and
/// passes ownership of the intermediate value to the next stage, so no stage
/// leaves partially shared state behind.
///
/// A failing stage is not turned into a panic: the whole pipeline runs inside
/// one `Result`, and the first [`syn::Error`] is converted into a
/// `compile_error!` invocation that the compiler reports at the span the
/// failing stage observed. Later stages are skipped, so exactly one diagnostic
/// is emitted per invocation.
///
/// # Parameters
///
/// `kind` selects the declaration family (component, configuration, ...) and
/// selects which parse rules apply. `attribute` holds the macro attribute
/// arguments and `item` holds the annotated item; both are consumed here,
/// because parsing takes ownership of their token streams.
///
/// # Returns
///
/// The generated token stream on success. On failure it is a single
/// `compile_error!` token stream carrying the message of the first failing
/// stage, which the compiler surfaces as one error instead of aborting the
/// compilation of unrelated crates.
pub(crate) fn expand_entry(kind: MacroKind, attribute: TokenStream, item: TokenStream) -> TokenStream {
    let result = parse::parse(kind, attribute.into(), item.into()).and_then(|raw| {
        let runtime = RuntimePath::resolve()?;
        let declaration = validate::validate(raw, &runtime)?;
        expand::dispatch(declaration, &runtime)
    });
    result.unwrap_or_else(Error::into_compile_error).into()
}
