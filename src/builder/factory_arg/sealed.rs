// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Sealing marker for typed factory argument implementations.

/// Prevents external implementations from separating a type's request
/// declaration from its resolution behavior.
pub trait Sealed {
    // empty
}
