// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Checks that active injection errors point to the unsupported field type.

/// Runs the compile-fail contract for an active invalid injection.
#[test]
fn active_invalid_injection_has_a_precise_compile_error() {
    trybuild::TestCases::new().compile_fail("tests/ui/active_invalid_injection.rs");
}
