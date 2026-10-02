// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use trybuild::TestCases;

#[test]
fn test_managed_lifecycle_must_use() {
    let cases = TestCases::new();
    cases.compile_fail("tests/ui/lifecycle/ignored_shutdown.rs");
    cases.compile_fail("tests/ui/lifecycle/ignored_managed.rs");
    cases.compile_fail("tests/ui/lifecycle/asynchronous_requires_wait.rs");
    cases.pass("tests/ui/lifecycle/explicit_drop.rs");
}
