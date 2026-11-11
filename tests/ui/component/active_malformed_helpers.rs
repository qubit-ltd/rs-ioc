// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use std::sync::Arc;

use qubit_ioc::Component;
use qubit_ioc::bean;

#[Component]
struct Example {
    #[cfg(all())]
    #[inject(id = "bad-id")]
    invalid_id: Arc<u8>,
    #[cfg(all())]
    #[inject]
    #[inject]
    duplicate: Arc<u8>,
    #[cfg(all())]
    #[inject(id = 1)]
    non_string: Arc<u8>,
    #[cfg(all())]
    #[value(1)]
    invalid_value: Arc<u8>,
    #[cfg(all())]
    #[inject]
    #[value("ignored")]
    conflict: Arc<u8>,
}

#[bean]
fn value(
    #[cfg(all())]
    #[inject(id = "bad-id")]
    invalid_id: Arc<u8>,
    #[cfg(all())]
    #[inject]
    #[inject]
    duplicate: Arc<u8>,
    #[cfg(all())]
    #[inject(id = 1)]
    non_string: Arc<u8>,
    #[cfg(all())]
    #[value(1)]
    invalid_value: Arc<u8>,
    #[cfg(all())]
    #[inject]
    #[value("ignored")]
    conflict: Arc<u8>,
) -> u32 {
    let _ = (invalid_id, duplicate, non_string, invalid_value, conflict);
    1
}

fn main() {}
