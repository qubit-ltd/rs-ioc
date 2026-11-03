// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================

use qubit_ioc::ContainerBuilder;

#[qubit_ioc::Configuration]
mod group {
    #[cfg(any())]
    fn register_ioc() {}
}

fn main() {
    let mut builder = ContainerBuilder::new();
    group::register_ioc(&mut builder).unwrap();
}
