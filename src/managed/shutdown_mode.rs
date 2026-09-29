// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Requested application shutdown behavior.

/// Selects dependency-preserving graceful shutdown or immediate cancellation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownMode {
    /// Stop and confirm each consumer before stopping its dependencies.
    Graceful,
    /// Request every abort before waiting for any component.
    Immediate,
}
