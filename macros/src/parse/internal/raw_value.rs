// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Syntactic values accepted by the shared option grammar.

use syn::Ident;
use syn::LitInt;
use syn::LitStr;
use syn::Type;

/// Syntactic values accepted by the common option grammar.
pub(crate) enum RawValue {
    /// A marker option with no assigned value.
    Flag,
    /// A string literal option value.
    String(
        /// String literal before option-specific semantic validation.
        LitStr,
    ),
    /// An integer literal with its source sign tracked separately.
    Integer {
        /// Integer token before semantic range checking.
        literal: LitInt,
        /// Whether a leading minus token was present.
        negative: bool,
    },
    /// A Rust type value such as `dyn Trait`.
    Type(
        /// Parsed Rust type before validating the option's supported shape.
        Type,
    ),
    /// An identifier value such as a generated marker name.
    Ident(
        /// Identifier token retained for generated names and diagnostics.
        Ident,
    ),
}
