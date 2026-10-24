// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Defines the consumer's interface binding.

/// Defines the interface used to resolve the concrete service through a binding.
///
/// Implementations must be safe to share between threads and return the value
/// associated with the service instance.
pub(super) trait Api: Send + Sync {
    /// Returns the value exposed by this service instance.
    ///
    /// The shared reference does not permit mutation of the service, and the
    /// returned integer is copied without transferring ownership.
    ///
    /// # Returns
    ///
    /// The value associated with this service instance.
    #[must_use]
    fn value(&self) -> u32;
}
