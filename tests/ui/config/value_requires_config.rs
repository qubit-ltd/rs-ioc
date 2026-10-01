// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
// This is a trybuild source fixture, not a test module.

use qubit_ioc::Component;

#[Component]
struct RequiresConfig {
    #[value("service.port")]
    port: u16,
}

fn main() {}
