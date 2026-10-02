// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::sync::Arc;

use qubit_ioc::Managed;

fn main() {
    let _resource = Managed::asynchronous(Arc::new(1_u8), |_| Ok(()));
}
