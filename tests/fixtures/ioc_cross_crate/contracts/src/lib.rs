// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shared service contract used by the cross-crate acceptance fixture.

/// A repository interface implemented in a separate provider crate.
pub trait Repository: Send + Sync {
    /// Returns a stable label for a record identifier.
    fn find(&self, id: u64) -> String;
}
