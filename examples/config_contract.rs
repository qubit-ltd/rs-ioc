// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Shows the direct config reads used by IoC macros and explicit interpolation.

use std::error::Error;
use std::sync::Arc;

use qubit_config::Config;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::config::deserialize_properties;
use qubit_ioc::config::get_value;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct EndpointProperties {
    host: String,
    endpoint: String,
}

/// Builds and checks one config-backed application service.
fn main() -> Result<(), Box<dyn Error>> {
    let mut config = Config::new();
    config.set("service.host", "localhost")?;
    config.set("service.endpoint", "${service.host}:8080")?;

    assert_eq!(
        get_value::<String>(&config, "service.endpoint")?,
        "${service.host}:8080"
    );
    let properties: EndpointProperties = deserialize_properties(&config, "service")?;
    assert_eq!(properties.endpoint, "${service.host}:8080");
    assert_eq!(config.get_interpolated::<String>("service.endpoint")?, "localhost:8080");

    let mut builder = ContainerBuilder::new().with_config(config)?;
    builder.register_factory::<String, _>(&[Dependency::of::<Config>()], |context| {
        let config = context.get::<Config>().map_err(qubit_ioc::FactoryError::new)?;
        config
            .get_interpolated::<String>("service.endpoint")
            .map(Arc::new)
            .map_err(qubit_ioc::FactoryError::new)
    })?;
    let context = builder.build_all()?;
    assert_eq!(context.get::<String>()?.as_str(), "localhost:8080");
    Ok(())
}
