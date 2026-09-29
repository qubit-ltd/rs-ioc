// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! A staged definition override and its source position.

use crate::key::BindingKey;

/// One explicit definition override and the definition that supplied it.
pub(in crate::builder) struct Replacement {
    /// Exact typed key identifying the earlier definition to remove.
    pub(in crate::builder) anchor: BindingKey,
    /// Index of the staged definition that supplies the replacement.
    pub(in crate::builder) definition_index: usize,
}
