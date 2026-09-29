// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Position of a binding within the active registered definitions.

/// The position of one binding in its registered definition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BindingLocation {
    /// Index of the owning definition in the active definition vector.
    pub(crate) definition: usize,
    /// Index of the binding within that definition.
    pub(crate) binding: usize,
}
