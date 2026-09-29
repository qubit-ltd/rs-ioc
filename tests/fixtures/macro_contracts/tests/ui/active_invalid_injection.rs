// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Supplies an active component field with an unsupported injection type.

use r#type::Component;

type MissingType = u32;

#[Component]
struct Unsupported {
    field: MissingType,
}

/// Provides the binary entry point for the compile-fail fixture.
fn main() {}
