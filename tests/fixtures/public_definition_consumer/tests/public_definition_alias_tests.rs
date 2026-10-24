// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Integration coverage for the standalone public-definition consumer.

use std::process::Command;

#[test]
fn test_public_definition_alias_without_default_features() {
    let status = Command::new(env!("CARGO_BIN_EXE_ioc-public-definition-consumer"))
        .status()
        .expect("run public-definition consumer");

    assert!(status.success(), "consumer binary failed: {status}");
}
