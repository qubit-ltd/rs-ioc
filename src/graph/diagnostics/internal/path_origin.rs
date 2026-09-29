// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Records how a selected binding entered the graph closure.

use crate::graph::BindingLocation;

/// Explains how one selected binding entered the graph closure.
#[derive(Clone, Copy)]
pub(crate) enum PathOrigin {
    /// The binding was selected directly by a root request or synthetic seed.
    Root,
    /// The binding was selected through a declared dependency or alias target.
    Dependency(
        /// Requesting binding that selected this dependency or alias target.
        BindingLocation,
    ),
    /// The binding was included because another binding selected its
    /// definition.
    DefinitionMember(
        /// Binding that caused the shared definition to enter the closure.
        BindingLocation,
    ),
}
