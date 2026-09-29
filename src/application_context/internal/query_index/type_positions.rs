// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cached registration and collection order for one binding type.

/// Positions for all bindings sharing one Rust type.
#[derive(Default)]
pub(super) struct TypePositions {
    /// Stable metadata positions in registration order.
    pub(super) registration: Vec<usize>,
    /// Stable metadata positions in collection order.
    pub(super) collection: Vec<usize>,
}
