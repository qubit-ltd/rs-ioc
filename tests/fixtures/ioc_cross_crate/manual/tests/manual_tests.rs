// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Verifies manual assembly through the public API with no default IoC
//! features.

use qubit_ioc_fixture_manual::build_manual;

#[test]
fn test_manual_assembly_without_default_features() {
    let application = build_manual().expect("build explicit graph");
    let context = application.context();
    assert_eq!(*context.get::<u64>().expect("factory result"), 42);
}
