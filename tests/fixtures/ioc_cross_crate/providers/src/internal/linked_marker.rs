// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Explicit dependency marker used by the application fixture.

/// Makes the provider crate an explicit dependency of the fixture application.
pub fn linked_marker() -> &'static str {
    "providers-linked"
}
