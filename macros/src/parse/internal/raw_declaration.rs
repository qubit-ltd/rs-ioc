// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Parsed declaration before semantic validation.

use syn::Item;

use crate::ir::MacroKind;
use crate::parse::RawOption;

/// An attribute whose syntax has been parsed but not semantically validated.
pub(crate) struct RawDeclaration {
    /// Macro attribute that selected the accepted syntax.
    pub(crate) kind: MacroKind,
    /// Parsed Rust item receiving the attribute.
    pub(crate) item: Item,
    /// Options retained until semantic validation.
    pub(crate) options: Vec<RawOption>,
}
