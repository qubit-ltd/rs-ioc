// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Component macro intermediate representation.

use syn::ItemStruct;

use crate::ir::BindingOptions;
use crate::ir::FieldIr;
use crate::ir::SourceIr;

/// A validated component, service, or repository struct.
pub(crate) struct ComponentIr {
    /// Original item retained for output.
    pub(crate) item: ItemStruct,
    /// Normalized binding metadata.
    pub(crate) options: BindingOptions,
    /// Field requests in declaration order.
    pub(crate) fields: Vec<FieldIr>,
    /// Original source identity for generated registration metadata.
    pub(crate) source: SourceIr,
}
