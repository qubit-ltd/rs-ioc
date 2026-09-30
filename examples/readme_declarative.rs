// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Assembles a declarative service whose dependency is bound to a trait object.

use std::error::Error;
use std::sync::Arc;

use qubit_ioc::Application;
use qubit_ioc::Component;
use qubit_ioc::Service;

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

fn main() -> Result<(), Box<dyn Error>> {
    let mut builder = Application::builder();
    builder.install::<English>()?;
    builder.install::<Greeter>()?;
    builder.root::<Greeter>();
    let application = builder.build()?;
    let context = application.context();
    assert_eq!(context.get::<Greeter>()?.greeting.text(), "hello");
    Ok(())
}
