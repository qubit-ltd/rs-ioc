// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Configuration macro intermediate representation.

use syn::ItemMod;
use syn::LitStr;

use crate::ir::SourceIr;

/// An inline configuration module and its default profile.
pub(crate) struct ConfigurationIr {
    /// Original inline module containing the declarations.
    pub(crate) item: ItemMod,
    /// Default profile applied to direct beans without an override.
    pub(crate) profile: Option<LitStr>,
    /// Source identity retained for diagnostics.
    pub(crate) source: SourceIr,
}
