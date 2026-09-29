// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(feature = "config")]

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_config::Config;
use qubit_config::ConfigError;
use qubit_ioc::BuildError;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::config::deserialize_properties;
use qubit_ioc::config::get_value;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct ServiceProperties {
    port: u16,
}

#[derive(Debug, Deserialize, PartialEq)]
struct EndpointProperties {
    host: String,
    endpoint: String,
}

#[test]
fn test_with_config_shares_one_snapshot_with_factories() {
    let mut config = Config::new();
    config.set("service.port", 8080).expect("set the port");
    let mut builder = ContainerBuilder::new().with_config(config).expect("stage config");
    builder
        .register_factory::<Arc<Config>, _>(&[Dependency::of::<Config>()], |context| {
            Ok(Arc::new(context.get::<Config>().expect("declared config dependency")))
        })
        .expect("stage config consumer");

    let application = builder.build_all().expect("build configured graph");

    let context = application.context();
    let configured = context.get::<Config>().expect("published config");
    let injected = context.get::<Arc<Config>>().expect("injected config");
    assert!(Arc::ptr_eq(&configured, injected.as_ref()));
    assert_eq!(get_value::<i32>(&configured, "service.port").expect("read port"), 8080);
}

#[test]
fn test_with_config_conflicts_with_explicit_config_registration() {
    let mut builder = ContainerBuilder::new()
        .with_config(Config::new())
        .expect("stage config");
    builder
        .register_instance(Arc::new(Config::new()))
        .expect("stage duplicate config");

    assert!(
        matches!(builder.build_all(), Err(failure) if matches!(failure.cause(), BuildError::DuplicateBinding { .. }))
    );
}

#[test]
fn test_get_value_uses_direct_read_without_interpolation() {
    let mut config = Config::new();
    config.set("service.host", "example.test").expect("set host");
    config
        .set("service.url", "https://${service.host}")
        .expect("set literal URL");

    assert_eq!(
        get_value::<String>(&config, "service.url").expect("direct read"),
        "https://${service.host}"
    );
}

#[test]
fn test_scalar_and_structured_reads_leave_interpolation_explicit() {
    let mut config = Config::new();
    config.set("service.host", "localhost").expect("set host");
    config
        .set("service.endpoint", "${service.host}:8080")
        .expect("set endpoint");

    assert_eq!(
        get_value::<String>(&config, "service.endpoint").expect("direct read"),
        "${service.host}:8080"
    );
    assert_eq!(
        deserialize_properties::<EndpointProperties>(&config, "service").expect("structured read"),
        EndpointProperties {
            host: "localhost".to_owned(),
            endpoint: "${service.host}:8080".to_owned(),
        }
    );
    assert_eq!(
        config
            .get_interpolated::<String>("service.endpoint")
            .expect("explicit interpolation"),
        "localhost:8080"
    );
}

#[test]
fn test_get_value_preserves_config_error_and_path() {
    let config = Config::new();
    let error = get_value::<u16>(&config, "service.port").expect_err("missing port");
    let source = error.source().expect("configuration source");
    assert!(matches!(source.downcast_ref::<ConfigError>(),
        Some(ConfigError::PropertyNotFound(path)) if path == "service.port"));
}

#[test]
fn test_deserialize_properties_rejects_unknown_fields_and_keeps_paths() {
    let mut config = Config::new();
    config.set("service.port", 8080).expect("set port");
    config.set("service.extra", "unexpected").expect("set unknown field");

    let error =
        deserialize_properties::<ServiceProperties>(&config, "service").expect_err("unknown field must be rejected");
    let source = error.source().expect("configuration source");
    assert!(matches!(source.downcast_ref::<ConfigError>(),
        Some(ConfigError::UnknownProperties { paths })
            if paths == &["service.extra".to_owned()]));
}

#[test]
fn test_deserialize_properties_reads_structured_snapshot() {
    let mut config = Config::new();
    config.set("service.port", 8080).expect("set port");

    assert_eq!(
        deserialize_properties::<ServiceProperties>(&config, "service").expect("read service properties"),
        ServiceProperties { port: 8080 }
    );
}

#[test]
fn test_missing_config_is_reported_before_factory_runs() {
    let runs = Arc::new(AtomicUsize::new(0));
    let factory_runs = Arc::clone(&runs);
    let mut builder = ContainerBuilder::new();
    builder
        .register_factory::<u16, _>(&[Dependency::of::<Config>()], move |context| {
            factory_runs.fetch_add(1, Ordering::SeqCst);
            let config = context.get::<Config>().expect("declared config dependency");
            get_value::<u16>(&config, "service.port").map(Arc::new)
        })
        .expect("stage config consumer");

    let error = match builder.build_all() {
        Ok(_) => panic!("missing config must fail graph validation"),
        Err(error) => error,
    };
    assert!(matches!(error.cause(), BuildError::MissingDependency { dependency, .. }
        if dependency == &Dependency::of::<Config>()));
    assert_eq!(runs.load(Ordering::SeqCst), 0);
}
