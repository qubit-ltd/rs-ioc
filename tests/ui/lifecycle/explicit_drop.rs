// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![deny(unused_must_use)]

use std::sync::Arc;

use qubit_ioc::ShutdownMode;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Managed;

fn main() {
    let builder = ContainerBuilder::new();
    drop(builder.build_all().unwrap().begin_shutdown(ShutdownMode::Immediate));
    drop(Managed::new(Arc::new(1_u8), |_| Ok(())));
}

// qubit-style: allow test-file-name
// This is a trybuild source fixture, not a test module.
