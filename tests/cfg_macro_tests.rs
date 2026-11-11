// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "macros")]

use trybuild::TestCases;

#[test]
fn test_inactive_malformed_helpers_preserve_conditions() {
    TestCases::new().pass("tests/ui/component/pass/inactive_malformed_helpers.rs");
}

#[test]
fn test_active_malformed_helpers_keep_diagnostics() {
    TestCases::new().compile_fail("tests/ui/component/active_malformed_helpers.rs");
}

#[test]
fn test_cfg_attr_helper_is_rejected_with_actionable_error() {
    TestCases::new().compile_fail("tests/ui/component/cfg_attr_helper.rs");
}

#[test]
fn test_inactive_configuration_register_ioc_is_ignored() {
    let cases = TestCases::new();
    cases.pass("tests/ui/configuration/inactive_register_ioc.rs");
    cases.pass("tests/ui/configuration/inactive_nested_register_ioc.rs");
}

#[test]
fn test_active_configuration_register_ioc_conflicts() {
    let cases = TestCases::new();
    cases.compile_fail("tests/ui/configuration/active_register_ioc.rs");
    cases.compile_fail("tests/ui/configuration/active_cfg_register_ioc.rs");
}

#[test]
#[cfg(not(feature = "config"))]
fn test_active_value_requires_config_feature() {
    TestCases::new().compile_fail("tests/ui/config/value_requires_config.rs");
}
