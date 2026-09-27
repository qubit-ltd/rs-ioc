// =============================================================================
//    Copyright (c) 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Greeting bean exposed by the provider fixture.

use std::sync::Arc;

use qubit_ioc::bean;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::RegistrationError;

use crate::AppService;

/// One bean generated from a function in a linked provider crate.
pub struct Greeting(
    /// Rendered greeting returned by the bean factory.
    pub String,
);

#[bean]
fn greeting(service: Arc<AppService>) -> Greeting {
    Greeting(format!("{}:{}", service.settings.label, service.primary.find(7)))
}

/// Installs the private marker generated for this module's greeting bean.
pub(crate) fn register_ioc(builder: &mut ContainerBuilder) -> Result<(), RegistrationError> {
    builder.install::<GreetingBean>()
}
