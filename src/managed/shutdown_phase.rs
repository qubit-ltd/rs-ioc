// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Lifecycle phase associated with a cleanup failure.

/// Identifies which lifecycle phase failed.
///
/// # Examples
///
/// ```
/// use qubit_ioc::ShutdownPhase;
///
/// assert_ne!(ShutdownPhase::Stop, ShutdownPhase::Wait);
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownPhase {
    /// The component's synchronous stop action failed.
    Stop,
    /// The component's asynchronous wait action failed.
    Wait,
}
