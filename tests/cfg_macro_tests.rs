// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "macros")]

#[test]
fn test_cfg_attr_helper_is_rejected_with_actionable_error() {
    trybuild::TestCases::new().compile_fail("tests/ui/component/cfg_attr_helper.rs");
}

#[test]
#[cfg(not(feature = "config"))]
fn test_active_value_requires_config_feature() {
    trybuild::TestCases::new().compile_fail("tests/ui/config/value_requires_config.rs");
}
