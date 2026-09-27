// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Repository contract shared by the fixture crates.

/// A repository interface implemented in a separate provider crate.
pub trait Repository: Send + Sync {
    /// Returns a stable label for a record identifier.
    fn find(&self, id: u64) -> String;
}
