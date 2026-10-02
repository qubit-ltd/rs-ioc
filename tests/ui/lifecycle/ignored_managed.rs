// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![deny(unused_must_use)]

use std::sync::Arc;

use qubit_ioc::Managed;

fn main() {
    Managed::synchronous(Arc::new(1_u8), |_| Ok(()));
}

// This is a trybuild source fixture, not a test module.
