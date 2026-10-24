// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Defines the concrete service used by the consumer.

use crate::api::Api;

/// Holds the value exposed through the interface binding.
pub(super) struct Service(
    /// The value returned by [`Api::value`].
    u32,
);

impl Api for Service {
    /// Returns the service's stored value without modifying it.
    ///
    /// # Returns
    ///
    /// The value stored in this service.
    #[inline]
    fn value(&self) -> u32 {
        self.0
    }
}
