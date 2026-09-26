// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::sync::Arc;

use qubit_ioc::ContainerBuilder;

#[derive(Default)]
struct Runtime;

impl Runtime {
    fn start(&self) {}
    fn shutdown(&self) {}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ContainerBuilder::new();
    builder.register_instance(Arc::new(Runtime))?;
    builder.root::<Runtime>();
    let context = builder.build()?;

    let runtime = context.get::<Runtime>()?;
    runtime.start();
    runtime.shutdown();
    Ok(())
}
