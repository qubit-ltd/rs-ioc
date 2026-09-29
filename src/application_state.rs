// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Observable lifecycle state shared by application query handles.

/// Lifecycle state of the application owner and its managed components.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ApplicationState {
    /// Construction succeeded and shutdown has not started.
    Running = 0,
    /// The owner has started the shutdown protocol.
    ShuttingDown = 1,
    /// Every managed component has confirmed termination.
    Closed = 2,
    /// At least one managed component has not confirmed termination.
    Incomplete = 3,
}
