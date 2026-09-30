// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Installs function beans individually and through a configuration group.

use std::error::Error;
use std::sync::Arc;

use qubit_ioc::Configuration;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::bean;

struct DefaultValue(u8);
struct CustomValue(u8);

#[bean]
fn default_value() -> DefaultValue {
    DefaultValue(1)
}

#[bean(marker = CustomFactory)]
fn custom_value() -> Arc<CustomValue> {
    Arc::new(CustomValue(2))
}

#[Configuration]
mod grouped {
    #[qubit_ioc::bean]
    fn label() -> String {
        "ready".to_owned()
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut builder = ContainerBuilder::new();
    builder.install::<DefaultValueBean>()?;
    builder.install::<CustomFactory>()?;
    grouped::register_ioc(&mut builder)?;
    builder.root::<DefaultValue>();
    builder.root::<CustomValue>();
    builder.root::<String>();
    let application = builder.build()?;
    let context = application.context();
    assert_eq!(context.get::<DefaultValue>()?.0, 1);
    assert_eq!(context.get::<CustomValue>()?.0, 2);
    assert_eq!(context.get::<String>()?.as_str(), "ready");
    Ok(())
}
