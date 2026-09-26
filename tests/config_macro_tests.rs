// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
#![cfg(all(feature = "macros", feature = "config"))]

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use qubit_config::Config;
use qubit_config::ConfigError;
use qubit_ioc::BuildError;
use qubit_ioc::ComponentDefinition;
use qubit_ioc::ConfigurationProperties;
use qubit_ioc::ContainerBuilder;
use qubit_ioc::Dependency;
use qubit_ioc::Service;
use qubit_ioc::bean;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
#[ConfigurationProperties(prefix = "service", id = "config.service", profile = "config_macro")]
struct ServiceSettings {
    port: u16,
    host: String,
}

#[derive(Debug, Deserialize, PartialEq)]
#[ConfigurationProperties(prefix = "", profile = "config_macro")]
struct RootSettings {
    enabled: bool,
}

#[Service(profile = "config_macro")]
struct ValueService {
    #[value("service.port")]
    port: u16,
    #[value("service.port")]
    port_copy: u16,
}

#[derive(Debug, PartialEq)]
struct ValueBean(u16, u16);

#[bean(profile = "config_macro")]
fn value_bean(#[value("service.port")] port: u16, #[value("service.port")] copy: u16) -> ValueBean {
    ValueBean(port, copy)
}

/// Creates a snapshot for the successful macro cases.
fn configured_snapshot() -> Config {
    let mut config = Config::new();
    config.set("service.port", 8140).expect("set port");
    config.set("service.host", "localhost").expect("set host");
    config.set("enabled", true).expect("set root flag");
    config
}

#[test]
fn test_configuration_properties_reads_subtree_and_root() {
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["config_macro"])
        .expect("activate macro profile")
        .with_config(configured_snapshot())
        .expect("register snapshot");
    builder.install::<ServiceSettings>().expect("install subtree settings");
    let context = builder.build_all().expect("deserialize properties");
    assert_eq!(
        *context.get_by_id::<ServiceSettings>("config.service").expect("subtree"),
        ServiceSettings {
            port: 8140,
            host: "localhost".to_owned()
        }
    );
    assert_eq!(
        <ServiceSettings as ComponentDefinition>::source().item,
        "ServiceSettings"
    );

    let mut root_config = Config::new();
    root_config.set("enabled", true).expect("set root flag");
    let mut root_builder = ContainerBuilder::new()
        .active_profiles(&["config_macro"])
        .expect("activate macro profile")
        .with_config(root_config)
        .expect("register root snapshot");
    root_builder.install::<RootSettings>().expect("install root settings");
    let root_context = root_builder.build_all().expect("deserialize root properties");
    assert_eq!(
        *root_context.get::<RootSettings>().expect("root"),
        RootSettings { enabled: true }
    );
}

#[test]
fn test_configuration_properties_rejects_unknown_field_with_config_source() {
    let mut config = configured_snapshot();
    config.set("service.extra", "unexpected").expect("set unknown property");
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["config_macro"])
        .expect("activate macro profile")
        .with_config(config)
        .expect("register snapshot");
    builder.install::<ServiceSettings>().expect("install settings");
    let error = builder.build_all().err().expect("unknown property must fail");
    match &error {
        BuildError::ConfigReadFailed {
            path_key, target, path, ..
        } => {
            assert_eq!(path_key, "service");
            assert_eq!(target, "ServiceSettings");
            assert_eq!(
                path.last().expect("failure path has binding").type_name(),
                std::any::type_name::<ServiceSettings>()
            );
        }
        other => panic!("expected configuration failure, got {other:?}"),
    }
    let source = error
        .source()
        .expect("factory wrapper")
        .source()
        .expect("config source");
    assert!(matches!(source.downcast_ref::<ConfigError>(),
        Some(ConfigError::UnknownProperties { paths })
            if paths == &["service.extra".to_owned()]));
}

#[test]
fn test_configuration_properties_missing_config_prevents_all_factories() {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    RUNS.store(0, Ordering::SeqCst);
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["config_macro"])
        .expect("activate macro profile");
    builder
        .register_factory::<u32, _>(&[], |_| {
            RUNS.fetch_add(1, Ordering::SeqCst);
            Ok(Arc::new(1))
        })
        .expect("register independent factory");
    builder.install::<ServiceSettings>().expect("install settings");
    let error = builder
        .build_all()
        .err()
        .expect("missing config must fail graph validation");
    assert!(matches!(error, BuildError::MissingDependency { dependency, .. }
        if dependency == Dependency::of::<Config>()));
    assert_eq!(RUNS.load(Ordering::SeqCst), 0);
}

#[test]
fn test_value_reads_component_field_and_bean_parameter() {
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["config_macro"])
        .expect("activate macro profile")
        .with_config(configured_snapshot())
        .expect("register snapshot");
    builder.install::<ValueService>().expect("install component");
    builder.install::<ValueBeanBean>().expect("install bean");
    let context = builder.build_all().expect("read both values");
    assert_eq!(context.get::<ValueService>().expect("component").port, 8140);
    assert_eq!(context.get::<ValueService>().expect("component").port_copy, 8140);
    assert_eq!(*context.get::<ValueBean>().expect("bean"), ValueBean(8140, 8140));
}

#[test]
fn test_value_missing_path_reports_target_and_original_config_error() {
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["config_macro"])
        .expect("activate macro profile")
        .with_config(Config::new())
        .expect("register empty snapshot");
    builder.install::<ValueService>().expect("install component");
    let error = builder.build_all().err().expect("missing value must fail");
    match &error {
        BuildError::ConfigReadFailed {
            path_key, target, path, ..
        } => {
            assert_eq!(path_key, "service.port");
            assert_eq!(target, "port");
            assert_eq!(
                path.last().expect("failure path has binding").type_name(),
                std::any::type_name::<ValueService>()
            );
        }
        other => panic!("expected configuration failure, got {other:?}"),
    }
    let source = error
        .source()
        .expect("factory wrapper")
        .source()
        .expect("config source");
    assert!(matches!(source.downcast_ref::<ConfigError>(),
        Some(ConfigError::PropertyNotFound(path)) if path == "service.port"));
}

#[test]
fn test_bean_value_missing_path_reports_parameter_name() {
    let mut builder = ContainerBuilder::new()
        .active_profiles(&["config_macro"])
        .expect("activate macro profile")
        .with_config(Config::new())
        .expect("register empty snapshot");
    builder.install::<ValueBeanBean>().expect("install bean");
    let error = builder.build_all().err().expect("missing parameter must fail");
    assert!(matches!(error, BuildError::ConfigReadFailed { path_key, target, .. }
        if path_key == "service.port" && target == "port"));
}

#[test]
fn test_value_and_inject_conflict_is_compile_error() {
    trybuild::TestCases::new().compile_fail("tests/ui/config/value_inject_conflict.rs");
}
