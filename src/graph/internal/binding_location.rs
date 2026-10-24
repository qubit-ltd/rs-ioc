// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Position of a binding within the active registered definitions.

/// Identifies one binding by its position in the validated graph's definitions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BindingLocation {
    /// Position of the owning definition in the profile-filtered definition
    /// vector.
    pub(crate) definition: usize,
    /// Position of the binding in the owning definition's declaration order.
    pub(crate) binding: usize,
}
