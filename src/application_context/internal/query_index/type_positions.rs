// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Cached registration and collection order for one binding type.

/// Positions for all bindings sharing one Rust type.
///
/// Every position is a zero-based index into the built binding metadata vector
/// the owning [`QueryIndex`](super::QueryIndex) was constructed from, so a
/// position is only meaningful together with that vector. The two fields hold
/// the same set of positions under two different orderings.
#[derive(Default)]
pub(super) struct TypePositions {
    /// Positions in registration order, the order in which the bindings appear
    /// in the built binding metadata vector.
    pub(super) registration: Vec<usize>,
    /// The same positions reordered by collection precedence: explicit
    /// `order`, then binding id, then definition source.
    pub(super) collection: Vec<usize>,
}
