// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Configuration properties macro intermediate representation.

use syn::ItemStruct;
use syn::LitStr;

use crate::ir::BindingOptions;
use crate::ir::SourceIr;

/// A configuration-backed struct and the subtree to deserialize.
pub(crate) struct ConfigurationPropertiesIr {
    /// Original properties struct retained for expansion.
    pub(crate) item: ItemStruct,
    /// Configuration subtree prefix to deserialize.
    pub(crate) prefix: LitStr,
    /// Binding options for the generated component.
    pub(crate) options: BindingOptions,
    /// Source identity retained for registration metadata.
    pub(crate) source: SourceIr,
}
