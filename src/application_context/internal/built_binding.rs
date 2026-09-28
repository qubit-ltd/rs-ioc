// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Active binding metadata used for immutable context queries.

use crate::key::BindingKey;
use crate::options::DefinitionSource;

/// Lookup metadata retained for candidate selection after construction.
pub(crate) struct BuiltBinding {
    /// Typed key used for exact lookup and candidate selection.
    pub(crate) key: BindingKey,
    /// Whether unnamed requests prefer this binding over other candidates.
    pub(crate) primary: bool,
    /// Position used to order collection queries.
    pub(crate) order: i32,
    /// Source of the currently active definition.
    pub(crate) source: DefinitionSource,
    /// Earlier sources whose exact key was replaced.
    pub(crate) replaced_sources: Vec<DefinitionSource>,
}
