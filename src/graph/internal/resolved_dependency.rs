// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! A declared request paired with the selected binding keys.

use crate::dependency::Dependency;
use crate::key::BindingKey;

/// A declared request and its exact selected keys, in injection order.
pub(crate) struct ResolvedDependency {
    /// Original request declared by the definition.
    pub(crate) request: Dependency,
    /// Selected keys in deterministic injection order.
    pub(crate) keys: Vec<BindingKey>,
}
