// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
use std::sync::Arc;

// Attribute expansion treats this same-package example as the runtime crate.
pub use qubit_ioc::{
    __private, ApplicationContext, BindingOptions, Component, ComponentDefinition, ContainerBuilder, DefinitionSource,
    Dependency, FactoryError, RegistrationError, Service,
};

trait Greeting: Send + Sync {
    fn text(&self) -> &'static str;
}

#[Component(bind = dyn Greeting, id = "example.greeting.english", primary)]
struct English;

impl Greeting for English {
    fn text(&self) -> &'static str {
        "hello"
    }
}

#[Service]
struct Greeter {
    greeting: Arc<dyn Greeting>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = ApplicationContext::builder();
    builder.install::<English>()?;
    builder.install::<Greeter>()?;
    builder.root::<Greeter>();
    let context = builder.build()?;
    assert_eq!(context.get::<Greeter>()?.greeting.text(), "hello");
    Ok(())
}
